use std::path::PathBuf;
use tracing::{error, info};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMigrationData {
    pub destination_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
}

pub struct Teleporter {
    vm_id: String,
    target_addr: String, // Expects format like "192.168.1.10"
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
        info!("Starting native Cloud Hypervisor TCP Teleportation to {}", self.target_addr);
        
        let client = Client::builder()
            .unix_socket(ch_socket_path)
            .build()
            .map_err(|e| format!("Failed to build socket client: {}", e))?;

        let destination_url = format!("tcp:{}:9000", self.target_addr);
        let config = SendMigrationData {
            destination_url,
            local: None,
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

        info!("Teleportation session completed successfully via native TCP!");
        Ok(())
    }
}
