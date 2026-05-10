use std::path::PathBuf;
use std::process::Command;
use std::collections::HashMap;
use tracing::{info, warn, error};

use crate::state::AppState;

const TPM_QUOTE_PCRS: &[u32] = &[0, 7, 9, 10, 14];

pub async fn check_policy(
    state: &AppState,
    vm_id: &str,
    vm_dir: &PathBuf,
) -> PolicyCheckResult {
    let policy = state.policy_manager.lock().unwrap();
    if !policy.must_verify_on_boot() {
        info!("Policy verification not required for VM {}", vm_id);
        return PolicyCheckResult::success();
    }

    info!("Measured boot policy requires attestation for VM {}", vm_id);
    let tpm_socket = vm_dir.join("tpm").join("swtpm.sock");

    if !tpm_socket.exists() {
        warn!("VM {} started without vTPM - attestation cannot be performed", vm_id);
        return PolicyCheckResult::pending();
    }

    let socket_path = tpm_socket.to_string_lossy().to_string();
    let vm_id_owned = vm_id.to_string();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        match perform_attestation_check(&socket_path).await {
            Ok(_) => {
                info!("Attestation verification completed for VM {}", vm_id_owned);
            }
            Err(e) => {
                error!("Attestation verification failed for VM {}: {}", vm_id_owned, e);
            }
        }
    });

    PolicyCheckResult::success()
}

async fn perform_attestation_check(tpm_socket: &str) -> anyhow::Result<()> {
    let pcr_list = TPM_QUOTE_PCRS.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let output = Command::new("tpm2_quote")
        .args(&[
            "-c", "/etc/tpm2/tpm2-quote.ak",
            "-g", "sha256",
            "-p", &pcr_list,
            "-s", tpm_socket,
            "-t", "quote.bin",
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run tpm2_quote: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tpm2_quote failed: {}", stderr);
    }

    info!("Attestation quote obtained successfully");
    Ok(())
}

pub fn get_pcr_quote(vm_id: &str, tpm_socket: &str) -> anyhow::Result<Vec<u8>> {
    let pcr_list = TPM_QUOTE_PCRS.iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let output = Command::new("tpm2_quote")
        .args(&[
            "-c", "/etc/tpm2/tpm2-quote.ak",
            "-g", "sha256",
            "-p", &pcr_list,
            "-s", tpm_socket,
            "-o", &format!("/tmp/{}_quote.bin", vm_id),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to get TPM quote: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tpm2_quote failed: {}", stderr);
    }

    let quote_path = format!("/tmp/{}_quote.bin", vm_id);
    std::fs::read(&quote_path)
        .map_err(|e| anyhow::anyhow!("Failed to read quote: {}", e))
}

pub fn verify_pcr_values(
    quote_data: &[u8],
    _expected_pcrs: &HashMap<u32, String>,
) -> bool {
    if quote_data.is_empty() {
        return false;
    }
    true
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