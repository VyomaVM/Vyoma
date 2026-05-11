use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMigrationData {
    pub destination_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    #[serde(rename = "state")]
    pub state: String,
    #[serde(rename = "memory")]
    pub memory: Option<VmMemoryInfo>,
    #[serde(rename = "migration")]
    pub migration: Option<MigrationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMemoryInfo {
    #[serde(rename = "total_bytes")]
    pub total_bytes: u64,
    #[serde(rename = "shared_bytes")]
    pub shared_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationInfo {
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "total_bytes")]
    pub total_bytes: Option<u64>,
    #[serde(rename = "transferred_bytes")]
    pub transferred_bytes: Option<u64>,
    #[serde(rename = "dirty_bytes")]
    pub dirty_bytes: Option<u64>,
    #[serde(rename = "dirty_rate")]
    pub dirty_rate: Option<u64>,
    #[serde(rename = "round")]
    pub round: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub status: String,
    pub total_pages: u64,
    pub transferred_pages: u64,
    pub dirty_pages: u64,
    pub dirty_rate_pages_per_sec: u64,
    pub round: u32,
    pub completed: bool,
    pub error: Option<String>,
}

pub type ProgressCallback = Box<dyn Fn(MigrationProgress) + Send + Sync>;

pub struct Teleporter {
    vm_id: String,
    target_addr: String,
}

impl Teleporter {
    pub fn new(vm_id: String, target_addr: String, _memory_size_bytes: u64) -> Self {
        info!("Initializing Teleporter for VM {}", vm_id);
        Self {
            vm_id,
            target_addr,
        }
    }

    pub async fn teleport_vm(&self, _memory_file: PathBuf, _state_file: PathBuf, ch_socket_path: &str) -> Result<(), String> {
        self.teleport_vm_with_config(ch_socket_path, None, None).await
    }

    pub async fn teleport_vm_with_config(
        &self,
        ch_socket_path: &str,
        bandwidth_mbps: Option<u32>,
        progress_callback: Option<Box<dyn Fn(MigrationProgress) + Send + Sync>>,
    ) -> Result<(), String> {
        info!(
            "Starting live migration to {} with bandwidth limit: {:?} Mbps",
            self.target_addr, bandwidth_mbps
        );

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let destination_url = format!("tcp:{}:9000", self.target_addr);
        let config = SendMigrationData {
            destination_url,
            local: Some(false),
            bandwidth: bandwidth_mbps,
        };

        let response = client
            .request(Method::PUT, "http://localhost/api/v1/vm.send-migration")
            .json(&config)
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            error!("Send migration failed: {}", err_text);
            return Err(format!("Send migration failed: {}", err_text));
        }

        info!("Live migration initiated successfully, waiting for completion...");

        self.wait_for_migration_complete(ch_socket_path, progress_callback)
            .await?;

        info!("Live migration completed successfully!");
        Ok(())
    }

    pub async fn wait_for_migration_complete(
        &self,
        ch_socket_path: &str,
        progress_callback: Option<Box<dyn Fn(MigrationProgress) + Send + Sync>>,
    ) -> Result<(), String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let poll_interval = Duration::from_millis(500);
        let max_wait_time = Duration::from_secs(600);
        let start_time = std::time::Instant::now();

        let page_size = 4096u64;

        loop {
            if start_time.elapsed() > max_wait_time {
                return Err("Migration timeout: exceeded 10 minutes".to_string());
            }

            let response = client
                .request(Method::GET, "http://localhost/api/v1/vm.info")
                .send()
                .await
                .map_err(|e| format!("Failed to query vm.info: {}", e))?;

            if !response.status().is_success() {
                sleep(poll_interval).await;
                continue;
            }

            let vm_info: VmInfo = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse vm.info response: {}", e))?;

            let migration_status = vm_info
                .migration
                .as_ref()
                .map(|m| m.status.as_str())
                .unwrap_or("unknown");

            let total_bytes = vm_info.memory.as_ref().map(|m| m.total_bytes).unwrap_or(0);
            let total_pages = total_bytes.saturating_div(page_size);

            let (transferred_bytes, dirty_bytes, dirty_rate, round) = if let Some(mig) = &vm_info.migration {
                (
                    mig.transferred_bytes.unwrap_or(0),
                    mig.dirty_bytes.unwrap_or(0),
                    mig.dirty_rate.unwrap_or(0),
                    mig.round.unwrap_or(0),
                )
            } else {
                (0, 0, 0, 0)
            };

            let transferred_pages = transferred_bytes.saturating_div(page_size);
            let dirty_pages = dirty_bytes.saturating_div(page_size);
            let completed = migration_status == "completed";
            let error = if migration_status == "failed" {
                Some("Migration failed".to_string())
            } else {
                None
            };

            let progress = MigrationProgress {
                status: migration_status.to_string(),
                total_pages,
                transferred_pages,
                dirty_pages,
                dirty_rate_pages_per_sec: dirty_rate.saturating_div(page_size),
                round,
                completed,
                error: error.clone(),
            };

            if let Some(ref callback) = progress_callback {
                callback(progress.clone());
            }

            let err_msg = error.clone();
            if completed {
                info!("Migration completed!");
                return Ok(());
            }

            if let Some(e) = err_msg {
                error!("Migration failed: {}", e);
                return Err(e);
            }

            match migration_status {
                "completed" => {
                    info!("Migration completed!");
                    return Ok(());
                }
                "failed" => {
                    error!("Migration failed");
                    return Err("Migration failed".to_string());
                }
                "active" | "Setup" | "PreEmpty" | "PreCopy" | "Install" => {
                    info!(
                        "Migration ongoing: round {}, transferred {:.2}%, dirty ~{:.2}%",
                        round,
                        if total_pages > 0 {
                            (transferred_pages as f64 / total_pages as f64) * 100.0
                        } else {
                            0.0
                        },
                        if total_pages > 0 {
                            (dirty_pages as f64 / total_pages as f64) * 100.0
                        } else {
                            0.0
                        }
                    );
                }
                _ => {
                    warn!("Unknown migration status: {}", migration_status);
                }
            }

            sleep(poll_interval).await;
        }
    }
}
