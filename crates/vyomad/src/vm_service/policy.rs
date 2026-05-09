use tracing::{info, warn};
use std::path::PathBuf;

use crate::state::AppState;

pub async fn check_policy(
    state: &AppState,
    vm_id: &str,
    vm_dir: &PathBuf,
) {
    let policy = state.policy_manager.lock().unwrap();
    if policy.must_verify_on_boot() {
        info!("Measured boot policy requires attestation for VM {}", vm_id);
        let tpm_socket = vm_dir.join("tpm").join("swtpm.sock");

        if tpm_socket.exists() {
            let vm_id_clone = vm_id.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                info!("Attestation check for VM {} (policy: required)", vm_id_clone);
            });
        } else {
            warn!("VM {} started without vTPM - attestation cannot be performed", vm_id);
        }
    }
}

pub struct PolicyCheckResult {
    pub passed: bool,
    pub attestation_pending: bool,
    pub error: Option<String>,
}

impl PolicyCheckResult {
    pub fn success() -> Self {
        Self {
            passed: true,
            attestation_pending: false,
            error: None,
        }
    }

    pub fn pending() -> Self {
        Self {
            passed: false,
            attestation_pending: true,
            error: None,
        }
    }

    pub fn failed(error: String) -> Self {
        Self {
            passed: false,
            attestation_pending: false,
            error: Some(error),
        }
    }
}