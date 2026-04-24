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

/// Remove orphaned `.part` files in the given directory (and one level of subdirectories).
/// Preserves `.part` files whose final destination is in `registered_paths` so that
/// in-progress shard downloads can resume after a restart.
pub fn cleanup_partial_downloads(
    dir: &Path,
    registered_paths: &std::collections::HashSet<PathBuf>,
) {
    let should_keep = |part_path: &Path| -> bool {
        // Derive the final path by stripping the trailing ".part" extension
        let final_path = part_path.with_extension("");
        registered_paths.contains(&final_path)
    };

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "part") && !should_keep(&path) {
                let _ = std::fs::remove_file(&path);
            }
            if path.is_dir() {
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if sub_path.extension().is_some_and(|e| e == "part")
                            && !should_keep(&sub_path)
                        {
                            let _ = std::fs::remove_file(&sub_path);
                        }
                    }
                }
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
        self.download_model_sharded(model_id, vec![(url, destination)], 0, on_complete)
            .await
    }

    pub async fn download_model_sharded(
        &self,
        model_id: String,
        files: Vec<(String, PathBuf)>,
        total_size_hint: u64,
        on_complete: Option<Box<dyn FnOnce() + Send + 'static>>,
    ) -> Result<()> {
        info!(model_id = %model_id, file_count = files.len(), "Starting model download");
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
                    total_bytes: total_size_hint,
                    progress_percent: 0.0,
                    speed_bps: None,
                    eta_seconds: None,
                    error: None,
                    task_exited: false,
                },
            );
        }

        // Create parent directories for all files
        for (_, dest) in &files {
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create directory: {}", e))?;
            }
        }

        let downloads = self.downloads.clone();
        let model_id_clone = model_id.clone();
        let files_for_cleanup: Vec<PathBuf> = files.iter().map(|(_, d)| d.clone()).collect();

        tokio::spawn(async move {
            let result =
                Self::download_files_sequentially(&files, &downloads, &model_id_clone).await;

            match result {
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
                    for dest in &files_for_cleanup {
                        let partial = partial_path_for(dest);
                        let _ = tokio::fs::remove_file(&partial).await;
                    }

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

    async fn cancellable_sleep(
        delay: std::time::Duration,
        downloads: &DownloadMap,
        model_id: &str,
    ) -> Result<(), anyhow::Error> {
        let check_interval = std::time::Duration::from_millis(500);
        let start = std::time::Instant::now();
        while start.elapsed() < delay {
            if Self::is_cancelled(downloads, model_id) {
                anyhow::bail!("Download cancelled");
            }
            let remaining = delay.saturating_sub(start.elapsed());
            tokio::time::sleep(std::cmp::min(check_interval, remaining)).await;
        }
        Ok(())
    }

    fn is_cancelled(downloads: &DownloadMap, model_id: &str) -> bool {
        if let Ok(downloads) = downloads.lock() {
            if let Some(progress) = downloads.get(model_id) {
                return progress.status == DownloadStatus::Cancelled;
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    /// Download multiple files sequentially, tracking cumulative progress under one model_id.
    async fn download_files_sequentially(
        files: &[(String, PathBuf)],
        downloads: &DownloadMap,
        model_id: &str,
    ) -> Result<(), anyhow::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(120))
            .build()?;

        // HEAD each file to get accurate total size. Only replace the hint if
        // every file returned a size; partial results would underestimate.
        let mut total: u64 = 0;
        let mut all_resolved = true;
        for (url, _) in files {
            let size = client
                .head(url)
                .send()
                .await
                .ok()
                .and_then(|r| r.content_length())
                .unwrap_or(0);
            if size == 0 {
                all_resolved = false;
            }
            total += size;
        }
        if all_resolved && total > 0 {
            if let Ok(mut dl) = downloads.lock() {
                if let Some(progress) = dl.get_mut(model_id) {
                    progress.total_bytes = total;
                }
            }
        }

        let start_time = std::time::Instant::now();
        let mut cumulative_bytes: u64 = 0;
        // Account for already-downloaded shards
        for (_, dest) in files {
            let partial = partial_path_for(dest);
            if dest.exists() {
                if let Ok(meta) = tokio::fs::metadata(dest).await {
                    cumulative_bytes += meta.len();
                }
            } else if partial.exists() {
                if let Ok(meta) = tokio::fs::metadata(&partial).await {
                    cumulative_bytes += meta.len();
                }
            }
        }
        let bytes_at_start = cumulative_bytes;

        for (url, destination) in files {
            if Self::is_cancelled(downloads, model_id) {
                anyhow::bail!("Download cancelled");
            }

            // Skip already-completed shards
            if destination.exists() {
                continue;
            }

            Self::download_one_file(
                &client,
                url,
                destination,
                downloads,
                model_id,
                &mut cumulative_bytes,
                start_time,
                bytes_at_start,
            )
            .await?;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn download_one_file(
        client: &reqwest::Client,
        url: &str,
        destination: &Path,
        downloads: &DownloadMap,
        model_id: &str,
        cumulative_bytes: &mut u64,
        start_time: std::time::Instant,
        bytes_at_start: u64,
    ) -> Result<(), anyhow::Error> {
        let partial_path = partial_path_for(destination);
        let mut retries = 0u32;

        let mut file_bytes: u64 = if partial_path.exists() {
            tokio::fs::metadata(&partial_path).await?.len()
        } else {
            0
        };

        // Get this file's total size
        let mut file_total: u64 = client
            .head(url)
            .send()
            .await
            .ok()
            .and_then(|r| r.content_length())
            .unwrap_or(0);

        // If partial matches expected size exactly, promote it
        if file_total > 0 && file_bytes == file_total {
            tokio::fs::rename(&partial_path, destination).await?;
            // cumulative_bytes already accounts for this file from the pre-scan
            return Ok(());
        }

        // If partial is oversized or remote changed, discard and re-download
        if file_total > 0 && file_bytes > file_total {
            info!(model_id = %model_id, file_bytes, file_total, "Partial file oversized, re-downloading");
            *cumulative_bytes = cumulative_bytes.saturating_sub(file_bytes);
            file_bytes = 0;
            let _ = tokio::fs::remove_file(&partial_path).await;
        }

        loop {
            if Self::is_cancelled(downloads, model_id) {
                let _ = tokio::fs::remove_file(&partial_path).await;
                anyhow::bail!("Download cancelled");
            }

            let mut request = client.get(url);
            if file_bytes > 0 {
                request = request.header("Range", format!("bytes={}-", file_bytes));
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
                    Self::cancellable_sleep(delay, downloads, model_id).await?;
                    continue;
                }
            };

            let status = response.status();
            if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                if file_total > 0 && file_bytes == file_total {
                    break;
                }
                *cumulative_bytes = cumulative_bytes.saturating_sub(file_bytes);
                file_bytes = 0;
                let _ = tokio::fs::remove_file(&partial_path).await;
                continue;
            }

            if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
                let is_transient = status.is_server_error()
                    || status == reqwest::StatusCode::REQUEST_TIMEOUT
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS;

                if !is_transient || retries >= Self::MAX_RETRIES {
                    anyhow::bail!("Failed to download: HTTP {}", status);
                }
                retries += 1;
                let delay = std::cmp::min(
                    Self::RETRY_BASE_DELAY * 2u32.saturating_pow(retries - 1),
                    Self::RETRY_MAX_DELAY,
                );
                info!(model_id = %model_id, retry = retries, http_status = %status, "Retrying download after transient HTTP error");
                Self::cancellable_sleep(delay, downloads, model_id).await?;
                continue;
            }

            if file_bytes > 0 && status == reqwest::StatusCode::OK {
                info!(model_id = %model_id, "Server ignored Range header, restarting file from scratch");
                // Subtract already-counted partial bytes from cumulative
                *cumulative_bytes = cumulative_bytes.saturating_sub(file_bytes);
                file_bytes = 0;
                let _ = tokio::fs::remove_file(&partial_path).await;
            }

            // If HEAD didn't return this file's size, learn it from the GET response.
            // This block only fires once per file (file_total stays non-zero after),
            // so retries don't double-count. Since download_files_sequentially's HEAD
            // pass contributed 0 for this file, we add the discovered size to the
            // shared total so progress/ETA are accurate.
            if file_total == 0 {
                let new_file_total = if file_bytes > 0 {
                    response
                        .headers()
                        .get("content-range")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.rsplit('/').next())
                        .and_then(|s| s.parse::<u64>().ok())
                } else {
                    response.content_length()
                };
                if let Some(t) = new_file_total {
                    file_total = t;
                    if let Ok(mut dl) = downloads.lock() {
                        if let Some(progress) = dl.get_mut(model_id) {
                            progress.total_bytes = progress.total_bytes.saturating_add(t);
                        }
                    }
                }
            }

            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)
                .await?;

            let file_len = tokio::fs::metadata(&partial_path).await?.len();
            if file_len != file_bytes {
                file.set_len(file_bytes).await?;
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
                        let chunk_len = chunk.len() as u64;
                        file_bytes += chunk_len;
                        *cumulative_bytes += chunk_len;

                        let elapsed = start_time.elapsed().as_secs_f64();
                        let bytes_this_session = cumulative_bytes.saturating_sub(bytes_at_start);
                        let speed_bps = if elapsed > 0.0 {
                            Some((bytes_this_session as f64 / elapsed) as u64)
                        } else {
                            None
                        };

                        let current_total = if let Ok(dl) = downloads.lock() {
                            dl.get(model_id).map(|p| p.total_bytes).unwrap_or(0)
                        } else {
                            0
                        };

                        let eta_seconds = if let Some(speed) = speed_bps {
                            if speed > 0 && current_total > 0 {
                                Some(current_total.saturating_sub(*cumulative_bytes) / speed)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Ok(mut dl) = downloads.lock() {
                            if let Some(progress) = dl.get_mut(model_id) {
                                progress.bytes_downloaded = *cumulative_bytes;
                                progress.progress_percent = if current_total > 0 {
                                    (*cumulative_bytes as f64 / current_total as f64 * 100.0) as f32
                                } else {
                                    0.0
                                };
                                progress.speed_bps = speed_bps;
                                progress.eta_seconds = eta_seconds;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        info!(model_id = %model_id, bytes = *cumulative_bytes, error = %e, "Download stream interrupted, will retry");
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
                Self::cancellable_sleep(delay, downloads, model_id).await?;
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
