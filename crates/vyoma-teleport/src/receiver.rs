use std::path::PathBuf;
use std::time::Duration;
use tracing::{error, info, warn};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;

use crate::sender::{VmInfo, MigrationProgress};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveMigrationData {
    pub receiver_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveMigrationConfig {
    pub receiver_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_source_ips: Option<Vec<String>>,
}

pub struct TeleportReceiver {
    session_id: String,
    listen_addr: String,
}

impl TeleportReceiver {
    pub fn new(_memory_file: PathBuf, _state_file: PathBuf, listen_addr: String) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            listen_addr,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn start_receiving(&self, ch_socket_path: &str) -> Result<(), String> {
        self.start_receiving_with_config(ch_socket_path, None).await
    }

    pub async fn start_receiving_with_config(
        &self,
        ch_socket_path: &str,
        trusted_source_ips: Option<Vec<String>>,
    ) -> Result<(), String> {
        info!(
            "Instructing Cloud Hypervisor to listen for migration on TCP port 9000, trusted sources: {:?}",
            trusted_source_ips
        );

        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let receiver_url = format!("tcp:{}:9000", self.listen_addr);
        let config = ReceiveMigrationConfig {
            receiver_url,
            trusted_source_ips,
        };

        let response = client
            .request(Method::PUT, "http://localhost/api/v1/vm.receive-migration")
            .json(&config)
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if !response.status().is_success() {
            let err_text = response.text().await.unwrap_or_default();
            error!("Receive migration failed: {}", err_text);
            return Err(format!("Receive migration failed: {}", err_text));
        }

        info!("Cloud Hypervisor is now receiving migration on native TCP!");
        Ok(())
    }

    pub async fn wait_for_incoming_migration(
        &self,
        ch_socket_path: &str,
        timeout: Duration,
    ) -> Result<MigrationProgress, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let poll_interval = Duration::from_millis(500);
        let start_time = std::time::Instant::now();
        let page_size = 4096u64;

        loop {
            if start_time.elapsed() > timeout {
                return Err("Migration receive timeout".to_string());
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
                .map_err(|e| format!("Failed to parse vm.info: {}", e))?;

            let state = &vm_info.state;
            let total_bytes = vm_info.memory.as_ref().map(|m| m.total_bytes).unwrap_or(0);
            let total_pages = total_bytes.saturating_div(page_size);

            let completed = state == "Running" || state == "Paused";

            if completed {
                let progress = MigrationProgress {
                    status: "completed".to_string(),
                    total_pages,
                    transferred_pages: total_pages,
                    dirty_pages: 0,
                    dirty_rate_pages_per_sec: 0,
                    round: 1,
                    completed: true,
                    error: None,
                };
                info!("Incoming migration completed, VM is now {:?}", state);
                return Ok(progress);
            }

            warn!("Waiting for incoming migration... VM state: {}", state);
            sleep(poll_interval).await;
        }
    }
}
