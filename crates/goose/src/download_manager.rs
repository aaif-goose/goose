use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tracing::info;
use utoipa::ToSchema;

fn partial_path_for(destination: &Path) -> PathBuf {
    destination.with_extension(
        destination
            .extension()
            .map(|e| format!("{}.part", e.to_string_lossy()))
            .unwrap_or_else(|| "part".to_string()),
    )
}

/// Remove any leftover `.part` files in the given directory.
pub fn cleanup_partial_downloads(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "part") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DownloadProgress {
    /// Model ID being downloaded
    pub model_id: String,
    /// Download status
    pub status: DownloadStatus,
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download progress percentage (0-100)
    pub progress_percent: f32,
    /// Download speed in bytes per second
    pub speed_bps: Option<u64>,
    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
    /// Whether the background download task has exited
    #[serde(skip)]
    pub task_exited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Downloading,
    Completed,
    Failed,
    Cancelled,
}

type DownloadMap = Arc<Mutex<HashMap<String, DownloadProgress>>>;

pub struct DownloadManager {
    downloads: DownloadMap,
}

impl Default for DownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            downloads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_progress(&self, model_id: &str) -> Option<DownloadProgress> {
        self.downloads.lock().ok()?.get(model_id).cloned()
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        let mut downloads = self
            .downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("Failed to acquire lock"))?;

        if let Some(progress) = downloads.get_mut(model_id) {
            progress.status = DownloadStatus::Cancelled;
            Ok(())
        } else {
            anyhow::bail!("Download not found")
        }
    }

    pub async fn download_model(
        &self,
        model_id: String,
        url: String,
        destination: PathBuf,
        on_complete: Option<Box<dyn FnOnce() + Send + 'static>>,
    ) -> Result<()> {
        info!(model_id = %model_id, url = %url, destination = ?destination, "Starting model download");
        {
            let mut downloads = self
                .downloads
                .lock()
                .map_err(|_| anyhow::anyhow!("Failed to acquire lock"))?;

            if let Some(existing) = downloads.get(&model_id) {
                if existing.status == DownloadStatus::Downloading {
                    anyhow::bail!("Download already in progress");
                }
                if existing.status == DownloadStatus::Cancelled && !existing.task_exited {
                    anyhow::bail!(
                        "Download is being cancelled; wait for it to finish before restarting"
                    );
                }
            }

            downloads.insert(
                model_id.clone(),
                DownloadProgress {
                    model_id: model_id.clone(),
                    status: DownloadStatus::Downloading,
                    bytes_downloaded: 0,
                    total_bytes: 0,
                    progress_percent: 0.0,
                    speed_bps: None,
                    eta_seconds: None,
                    error: None,
                    task_exited: false,
                },
            );
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
        }

        let downloads = self.downloads.clone();
        let model_id_clone = model_id.clone();

        let destination_for_cleanup = destination.clone();

        // Download in background task
        tokio::spawn(async move {
            match Self::download_file(&url, &destination, &downloads, &model_id_clone).await {
                Ok(_) => {
                    info!(model_id = %model_id_clone, "Download completed successfully");
                    if let Ok(mut downloads) = downloads.lock() {
                        if let Some(progress) = downloads.get_mut(&model_id_clone) {
                            progress.status = DownloadStatus::Completed;
                            progress.progress_percent = 100.0;
                            progress.task_exited = true;
                        }
                    }

                    if let Some(callback) = on_complete {
                        callback();
                    }
                }
                Err(e) => {
                    // Clean up partial file on failure
                    let partial = partial_path_for(&destination_for_cleanup);
                    let _ = tokio::fs::remove_file(&partial).await;

                    if let Ok(mut downloads) = downloads.lock() {
                        if let Some(progress) = downloads.get_mut(&model_id_clone) {
                            if progress.status != DownloadStatus::Cancelled {
                                progress.status = DownloadStatus::Failed;
                            }
                            progress.error = Some(e.to_string());
                            progress.task_exited = true;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    const MAX_RETRIES: u32 = 10;
    const RETRY_BASE_DELAY: std::time::Duration = std::time::Duration::from_secs(2);
    const RETRY_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(60);

    fn is_cancelled(downloads: &DownloadMap, model_id: &str) -> bool {
        if let Ok(downloads) = downloads.lock() {
            if let Some(progress) = downloads.get(model_id) {
                return progress.status == DownloadStatus::Cancelled;
            }
        }
        false
    }

    async fn download_file(
        url: &str,
        destination: &PathBuf,
        downloads: &DownloadMap,
        model_id: &str,
    ) -> Result<(), anyhow::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()?;

        let partial_path = partial_path_for(destination);
        let mut retries = 0u32;

        // Check for existing partial file to resume
        let mut bytes_downloaded: u64 = if partial_path.exists() {
            tokio::fs::metadata(&partial_path).await?.len()
        } else {
            0
        };

        // Get total size with a HEAD request first (so we know even before first chunk)
        let total_bytes = {
            let head_resp = client
                .head(url)
                .send()
                .await
                .ok()
                .and_then(|r| r.content_length());
            head_resp.unwrap_or(0)
        };

        if let Ok(mut dl) = downloads.lock() {
            if let Some(progress) = dl.get_mut(model_id) {
                progress.total_bytes = total_bytes;
                progress.bytes_downloaded = bytes_downloaded;
                if total_bytes > 0 {
                    progress.progress_percent =
                        (bytes_downloaded as f64 / total_bytes as f64 * 100.0) as f32;
                }
            }
        }

        // If already fully downloaded from a previous partial, just rename
        if total_bytes > 0 && bytes_downloaded >= total_bytes {
            tokio::fs::rename(&partial_path, destination).await?;
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        // bytes_at_start tracks how many bytes we had when timing began (for speed calc)
        let bytes_at_start = bytes_downloaded;

        loop {
            if Self::is_cancelled(downloads, model_id) {
                let _ = tokio::fs::remove_file(&partial_path).await;
                anyhow::bail!("Download cancelled");
            }

            // Build request with Range header for resume
            let mut request = client.get(url);
            if bytes_downloaded > 0 {
                request = request.header("Range", format!("bytes={}-", bytes_downloaded));
            }

            let response = match request.send().await {
                Ok(r) => r,
                Err(e) => {
                    if retries >= Self::MAX_RETRIES {
                        anyhow::bail!("Download failed after {} retries: {}", retries, e);
                    }
                    retries += 1;
                    let delay = std::cmp::min(
                        Self::RETRY_BASE_DELAY * 2u32.saturating_pow(retries - 1),
                        Self::RETRY_MAX_DELAY,
                    );
                    info!(model_id = %model_id, retry = retries, delay_secs = ?delay.as_secs(), error = %e, "Retrying download after connection error");
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            let status = response.status();
            if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                // Server can't satisfy range — file may be complete or something is off.
                // If partial file is at least total_bytes, treat as done.
                if total_bytes > 0 && bytes_downloaded >= total_bytes {
                    break;
                }
                // Otherwise restart from scratch
                bytes_downloaded = 0;
                let _ = tokio::fs::remove_file(&partial_path).await;
                continue;
            }

            if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
                if retries >= Self::MAX_RETRIES {
                    anyhow::bail!("Failed to download: HTTP {}", status);
                }
                retries += 1;
                let delay = std::cmp::min(
                    Self::RETRY_BASE_DELAY * 2u32.saturating_pow(retries - 1),
                    Self::RETRY_MAX_DELAY,
                );
                info!(model_id = %model_id, retry = retries, http_status = %status, "Retrying download after HTTP error");
                tokio::time::sleep(delay).await;
                continue;
            }

            // Update total_bytes from Content-Range or Content-Length if not yet known
            if total_bytes == 0 {
                let new_total = if bytes_downloaded > 0 {
                    // Parse Content-Range: bytes 1234-5678/9999
                    response
                        .headers()
                        .get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.rsplit('/').next())
                        .and_then(|s| s.parse::<u64>().ok())
                } else {
                    response.content_length()
                };
                if let Some(t) = new_total {
                    if let Ok(mut dl) = downloads.lock() {
                        if let Some(progress) = dl.get_mut(model_id) {
                            progress.total_bytes = t;
                        }
                    }
                }
            }

            // Open file for appending (or create)
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)
                .await?;

            // Truncate to bytes_downloaded in case file grew beyond our tracking
            let file_len = tokio::fs::metadata(&partial_path).await?.len();
            if file_len != bytes_downloaded {
                file.set_len(bytes_downloaded).await?;
            }

            let mut stream_error = false;
            let mut resp = response;

            loop {
                let chunk_result = resp.chunk().await;
                match chunk_result {
                    Ok(Some(chunk)) => {
                        if Self::is_cancelled(downloads, model_id) {
                            let _ = tokio::fs::remove_file(&partial_path).await;
                            anyhow::bail!("Download cancelled");
                        }

                        file.write_all(&chunk).await?;
                        bytes_downloaded += chunk.len() as u64;

                        let elapsed = start_time.elapsed().as_secs_f64();
                        let bytes_this_session = bytes_downloaded.saturating_sub(bytes_at_start);
                        let speed_bps = if elapsed > 0.0 {
                            Some((bytes_this_session as f64 / elapsed) as u64)
                        } else {
                            None
                        };

                        let current_total = if let Ok(dl) = downloads.lock() {
                            dl.get(model_id)
                                .map(|p| p.total_bytes)
                                .unwrap_or(total_bytes)
                        } else {
                            total_bytes
                        };

                        let eta_seconds = if let Some(speed) = speed_bps {
                            if speed > 0 && current_total > 0 {
                                Some(current_total.saturating_sub(bytes_downloaded) / speed)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Ok(mut dl) = downloads.lock() {
                            if let Some(progress) = dl.get_mut(model_id) {
                                progress.bytes_downloaded = bytes_downloaded;
                                progress.progress_percent = if current_total > 0 {
                                    (bytes_downloaded as f64 / current_total as f64 * 100.0) as f32
                                } else {
                                    0.0
                                };
                                progress.speed_bps = speed_bps;
                                progress.eta_seconds = eta_seconds;
                            }
                        }
                    }
                    Ok(None) => break, // Stream finished
                    Err(e) => {
                        info!(model_id = %model_id, bytes = bytes_downloaded, error = %e, "Download stream interrupted, will retry");
                        stream_error = true;
                        break;
                    }
                }
            }

            file.flush().await?;
            drop(file);

            if stream_error {
                if retries >= Self::MAX_RETRIES {
                    anyhow::bail!(
                        "Download failed after {} retries due to stream interruption",
                        retries
                    );
                }
                retries += 1;
                let delay = std::cmp::min(
                    Self::RETRY_BASE_DELAY * 2u32.saturating_pow(retries - 1),
                    Self::RETRY_MAX_DELAY,
                );
                info!(model_id = %model_id, retry = retries, delay_secs = ?delay.as_secs(), "Retrying download with resume");
                tokio::time::sleep(delay).await;
                continue;
            }

            break;
        }

        tokio::fs::rename(&partial_path, destination).await?;
        Ok(())
    }

    pub fn clear_completed(&self, model_id: &str) {
        if let Ok(mut downloads) = self.downloads.lock() {
            if let Some(progress) = downloads.get(model_id) {
                let is_terminal = progress.status == DownloadStatus::Completed
                    || progress.status == DownloadStatus::Failed
                    || progress.status == DownloadStatus::Cancelled;
                if is_terminal && progress.task_exited {
                    downloads.remove(model_id);
                }
            }
        }
    }
}

static DOWNLOAD_MANAGER: once_cell::sync::Lazy<DownloadManager> =
    once_cell::sync::Lazy::new(DownloadManager::new);

pub fn get_download_manager() -> &'static DownloadManager {
    &DOWNLOAD_MANAGER
}
