use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error};

use crate::state::{AppState, VmStatus};
use vyoma_core::unified_attest::{UnifiedAttestationManager, UnifiedAttestationRequest};
use vyoma_core::attest::AttestationRequest;
use vyoma_image::{VmifConverter, SignedManifest};

pub async fn check_policy_and_perform_attestation(
    state: Arc<AppState>,
    vm_id: String,
    vm_dir: PathBuf,
    image_name: String,
) -> Result<(), String> {
    let policy = state.policy_manager.lock().unwrap();
    if !policy.must_verify_on_boot() {
        info!("Policy verification not required for VM {}", vm_id);

        // Update VM status to Running since no attestation needed
        update_vm_status(&state, &vm_id, VmStatus::Running).await?;
        return Ok(());
    }

    info!("Measured boot policy requires attestation for VM {}", vm_id);

    // Set timeout for attestation
    let attestation_timeout = Duration::from_secs(policy.get_config().measured_boot.verification_timeout_secs);

    let result = timeout(attestation_timeout, perform_attestation_check(state.clone(), vm_id.clone(), vm_dir, image_name)).await;

    match result {
        Ok(attestation_result) => {
            match attestation_result {
                Ok(_) => {
                    info!("Attestation verification succeeded for VM {}", vm_id);
                    update_vm_status(&state, &vm_id, VmStatus::Running).await?;
                    Ok(())
                }
                Err(e) => {
                    error!("Attestation verification failed for VM {}: {}", vm_id, e);
                    update_vm_status(&state, &vm_id, VmStatus::Error { reason: format!("Attestation failed: {}", e) }).await?;
                    Err(e)
                }
            }
        }
        Err(_) => {
            let error_msg = format!("Attestation timeout after {} seconds", attestation_timeout.as_secs());
            error!("{} for VM {}", error_msg, vm_id);
            update_vm_status(&state, &vm_id, VmStatus::Error { reason: error_msg.clone() }).await?;
            Err(error_msg)
        }
    }
}

async fn perform_attestation_check(
    state: Arc<AppState>,
    vm_id: String,
    vm_dir: PathBuf,
    image_name: String,
) -> Result<(), String> {
    // POL-2: Load and verify signed manifest signature
    let signed_manifest = load_and_verify_manifest(&image_name)
        .map_err(|e| format!("Failed to load/verify manifest: {}", e))?;

    let expected_pcrs = signed_manifest.manifest.measured_boot.pcr_policy
        .as_ref()
        .ok_or_else(|| "Manifest does not contain PCR policy - image was not built with --measured".to_string())?;

    // POL-3: Perform TPM attestation
    let tpm_socket = vm_dir.join("tpm").join("swtpm.sock");
    if !tpm_socket.exists() {
        return Err("vTPM socket not found - VM was not started with TPM".to_string());
    }

    let socket_path = tpm_socket.to_string_lossy().to_string();

    // Get TPM quote
    let quote_data = get_pcr_quote(&vm_id, &socket_path)
        .map_err(|e| format!("Failed to get TPM quote: {}", e))?;

    // For now, use a simple verification - in production this would use proper TPM verification
    // TODO: Implement proper TPM quote verification
    if verify_pcr_values(&quote_data, expected_pcrs) {
        info!("TPM attestation verification succeeded for VM {}", vm_id);
        Ok(())
    } else {
        Err("PCR values do not match expected values".to_string())
    }
}

fn load_and_verify_manifest(image_name: &str) -> Result<SignedManifest, String> {
    // Find the image directory
    let home = dirs::home_dir()
        .ok_or_else(|| "No home directory".to_string())?;
    let images_dir = home.join(".ignite").join("images");
    let image_dir = images_dir.join(image_name.replace('/', "_").replace(':', "_"));

    if !image_dir.exists() {
        return Err(format!("Image directory not found: {:?}", image_dir));
    }

    // Try to load signed manifest
    let sig_path = image_dir.join("vyoma.toml.sig");
    if !sig_path.exists() {
        return Err("Signed manifest not found - image was not signed during build".to_string());
    }

    let signed_manifest = VmifConverter::load_signed_manifest(&sig_path)
        .map_err(|e| format!("Failed to load signed manifest: {}", e))?;

    // Verify signature using configured trusted keys
    // TODO: Implement signature verification against trust policy

    Ok(signed_manifest)
}

async fn update_vm_status(state: &AppState, vm_id: &str, new_status: VmStatus) -> Result<(), String> {
    let vms = state.vms.lock().unwrap();
    if let Some(vm_arc) = vms.get(vm_id) {
        let mut vm = vm_arc.lock().await;
        vm.status = new_status.clone();

        // Save updated state to disk
        vm.save_state()
            .map_err(|e| format!("Failed to save VM state: {}", e))?;

        info!("Updated VM {} status to {:?}", vm_id, new_status);
        Ok(())
    } else {
        Err(format!("VM {} not found in state", vm_id))
    }
}