use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error};

use super::wal::Wal;
use super::AppState;
use super::VmState;
use vyoma_core::vmm::VmmManager;

#[derive(Debug, Clone)]
pub enum VmRecoveryStatus {
    Running,
    Stopped,
    Crashed,
    Unknown,
}

#[derive(Debug)]
pub struct RecoveredVm {
    pub vm_id: String,
    pub state: VmState,
    pub status: VmRecoveryStatus,
}

pub struct Recovery;

impl Recovery {
    pub async fn recover_on_startup(home: &Path, wal: &Wal, state: &AppState) -> Result<Vec<RecoveredVm>> {
        let mut recovered = Vec::new();
        
        let vms_dir = home.join(".vyoma").join("vms");
        if !vms_dir.exists() {
            info!("No vms directory found, skipping recovery");
            return Ok(recovered);
        }

        let active_vms: HashMap<String, u64> = wal.get_active_vm_ids()
            .into_iter()
            .collect();

        if active_vms.is_empty() {
            info!("No active VMs in WAL, skipping recovery");
            return Ok(recovered);
        }

        info!("WAL indicates {} active VMs to check", active_vms.len());
        
        for (vm_id, _) in active_vms {
            let vm_dir = vms_dir.join(&vm_id);
            
            if !vm_dir.exists() {
                warn!("VM {} active in WAL but directory missing, skipping", vm_id);
                continue;
            }

            let state_file = vm_dir.join("state.json");
            if !state_file.exists() {
                warn!("No state.json for VM {}, cannot recover", vm_id);
                continue;
            }

            let state_content = match std::fs::read_to_string(&state_file) {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to read state for {}: {}", vm_id, e);
                    continue;
                }
            };
            
            let vm_state: VmState = match serde_json::from_str(&state_content) {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to parse state for {}: {}", vm_id, e);
                    continue;
                }
            };

            let status = Self::check_vm_status(&vm_id, &vm_dir).await;
            
            match status {
                VmRecoveryStatus::Running => {
                    info!("VM {} is ALIVE. Adopting...", vm_id);
                    recovered.push(RecoveredVm {
                        vm_id: vm_id.clone(),
                        state: vm_state,
                        status: VmRecoveryStatus::Running,
                    });
                }
                VmRecoveryStatus::Crashed => {
                    warn!("VM {} is DEAD. Cleaning up orphaned resources...", vm_id);
                    if let Err(e) = Self::cleanup_orphaned_resources(&vm_dir).await {
                        error!("Failed to cleanup {}: {}", vm_id, e);
                    }
                }
                _ => {
                    info!("VM {} status: {:?}", vm_id, status);
                }
            }
        }

        info!("Recovery complete: {} VMs adopted", recovered.len());
        Ok(recovered)
    }

    async fn check_vm_status(vm_id: &str, vm_dir: &Path) -> VmRecoveryStatus {
        let ch_socket = vm_dir.join("ch.sock");
        if !ch_socket.exists() {
            return VmRecoveryStatus::Crashed;
        }

        let socket_path = ch_socket.to_string_lossy().to_string();
        
        let check_future = async {
            let vmm = VmmManager::new(&socket_path);
            vmm.check_alive().await
        };
        
        match timeout(Duration::from_secs(3), check_future).await {
            Ok(true) => VmRecoveryStatus::Running,
            Ok(false) => VmRecoveryStatus::Crashed,
            Err(_) => {
                warn!("VM {} check timed out", vm_id);
                VmRecoveryStatus::Unknown
            }
        }
    }

    pub async fn cleanup_orphaned_resources(vm_dir: &Path) -> Result<()> {
        use vyoma_core::network::NetworkManager;
        use vyoma_core::storage::StorageManager;
        use vyoma_core::cgroups::CgroupManager;

        info!("Cleaning up orphaned resources in {:?}", vm_dir);

        let state_file = vm_dir.join("state.json");
        if !state_file.exists() {
            return Ok(());
        }

        let state_content = std::fs::read_to_string(&state_file)?;
        let state: VmState = serde_json::from_str(&state_content)?;

        if !state.tap_name.is_empty() {
            if let Err(e) = NetworkManager::remove_interface(&state.tap_name) {
                warn!("Failed to remove TAP {}: {}", state.tap_name, e);
            }
        }

        if !state.dm_name.is_empty() {
            if let Err(e) = StorageManager::remove_dm_device(&state.dm_name) {
                warn!("Failed to remove DM {}: {}", state.dm_name, e);
            }
        }

        for dev in &state.loop_devices {
            if let Err(e) = StorageManager::detach_loop_device(dev) {
                warn!("Failed to detach loop {}: {}", dev, e);
            }
        }

        if let Some(ref cgroup_path) = state.cgroup_path {
            let cm = CgroupManager::new();
            if let Err(e) = cm.remove_vm_cgroup(&state.id) {
                warn!("Failed to remove cgroup: {}", e);
            }
        }

        info!("Orphaned resource cleanup complete for {}", state.id);
        Ok(())
    }
}
