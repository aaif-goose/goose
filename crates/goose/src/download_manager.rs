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

    async fn download_file(
        url: &str,
        destination: &PathBuf,
        downloads: &DownloadMap,
        model_id: &str,
    ) -> Result<(), anyhow::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .read_timeout(std::time::Duration::from_secs(60))
            .build()?;
        let mut response = client.get(url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download: HTTP {}", response.status());
        }

        let total_bytes = response.content_length().unwrap_or(0);

        {
            if let Ok(mut downloads) = downloads.lock() {
                if let Some(progress) = downloads.get_mut(model_id) {
                    progress.total_bytes = total_bytes;
                }
            }
        }

        let partial_path = partial_path_for(destination);
        let mut file = tokio::fs::File::create(&partial_path).await?;
        let mut bytes_downloaded = 0u64;
        let start_time = std::time::Instant::now();

        while let Some(chunk) = response.chunk().await? {
            // Check if cancelled
            let should_cancel = {
                if let Ok(downloads) = downloads.lock() {
                    if let Some(progress) = downloads.get(model_id) {
                        progress.status == DownloadStatus::Cancelled
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if should_cancel {
                let _ = tokio::fs::remove_file(&partial_path).await;
                anyhow::bail!("Download cancelled");
            }

            file.write_all(&chunk).await?;
            bytes_downloaded += chunk.len() as u64;

            // Update progress
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed_bps = if elapsed > 0.0 {
                Some((bytes_downloaded as f64 / elapsed) as u64)
            } else {
                None
            };

            let eta_seconds = if let Some(speed) = speed_bps {
                if speed > 0 && total_bytes > 0 {
                    Some(total_bytes.saturating_sub(bytes_downloaded) / speed)
                } else {
                    None
                }
            } else {
                None
            };

            if let Ok(mut downloads) = downloads.lock() {
                if let Some(progress) = downloads.get_mut(model_id) {
                    progress.bytes_downloaded = bytes_downloaded;
                    progress.progress_percent = if total_bytes > 0 {
                        (bytes_downloaded as f64 / total_bytes as f64 * 100.0) as f32
                    } else {
                        0.0
                    };
                    progress.speed_bps = speed_bps;
                    progress.eta_seconds = eta_seconds;
                }
            }
        }

        file.flush().await?;
        drop(file);
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
