use std::path::PathBuf;
use tracing::{error, info};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveMigrationData {
    pub receiver_url: String,
}

pub struct TeleportReceiver {
    session_id: String,
    listen_addr: String, // e.g. "0.0.0.0"
}

impl TeleportReceiver {
    pub fn new(_memory_file: PathBuf, _state_file: PathBuf, listen_addr: String) -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            listen_addr,
        }
    }

    pub async fn start_receiving(&self, ch_socket_path: &str) -> Result<(), String> {
        info!("Instructing Cloud Hypervisor to listen for migration on TCP port 9000");

        let client = Client::builder()
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let receiver_url = format!("tcp:{}:9000", self.listen_addr);
        let config = ReceiveMigrationData {
            receiver_url,
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
}
