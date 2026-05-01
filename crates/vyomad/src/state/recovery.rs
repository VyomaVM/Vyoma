use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn, error};

use super::wal::Wal;
use super::VmState;

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
    pub fn recover_on_startup(home: &Path, wal: &Wal) -> Result<Vec<RecoveredVm>> {
        let mut recovered = Vec::new();
        
        let vms_dir = home.join(".ignite").join("vms");
        if !vms_dir.exists() {
            info!("No vms directory found, skipping recovery");
            return Ok(recovered);
        }

        info!("Scanning for VMs in {:?}", vms_dir);
        
        for entry in std::fs::read_dir(&vms_dir)? {
            let entry = entry?;
            let vm_dir = entry.path();
            
            if !vm_dir.is_dir() {
                continue;
            }

            let vm_id = vm_dir.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            
            let Some(vm_id) = vm_id else {
                warn!("Skipping invalid vm directory: {:?}", vm_dir);
                continue;
            };

            let state_file = vm_dir.join("state.json");
            if !state_file.exists() {
                warn!("No state.json for VM {}, skipping", vm_id);
                continue;
            }

            let state_content = std::fs::read_to_string(&state_file)?;
            let state: VmState = serde_json::from_str(&state_content)
                .map_err(|e| anyhow!("Failed to parse state.json for {}: {}", vm_id, e))?;

            let status = Self::determine_vm_status(&vm_id, &state, wal);
            
            info!("VM {} recovered with status: {:?}", vm_id, status);
            
            recovered.push(RecoveredVm {
                vm_id,
                state,
                status,
            });
        }

        info!("Recovery complete: {} VMs processed", recovered.len());
        Ok(recovered)
    }

    fn determine_vm_status(vm_id: &str, state: &VmState, wal: &Wal) -> VmRecoveryStatus {
        let entries = wal.get_vm_entries(vm_id);
        
        if entries.is_empty() {
            return VmRecoveryStatus::Unknown;
        }

        let last_entry = entries.last();
        
        match last_entry {
            Some(super::wal::WalEntry::VmCreate { .. }) => VmRecoveryStatus::Stopped,
            Some(super::wal::WalEntry::VmStart { .. }) => {
                if Self::is_process_alive(vm_id) {
                    VmRecoveryStatus::Running
                } else {
                    VmRecoveryStatus::Crashed
                }
            },
            Some(super::wal::WalEntry::VmStop { .. }) => VmRecoveryStatus::Stopped,
            Some(super::wal::WalEntry::VmDestroy { .. }) => VmRecoveryStatus::Stopped,
            Some(super::wal::WalEntry::VmCheckpoint { .. }) => VmRecoveryStatus::Stopped,
            None => VmRecoveryStatus::Unknown,
        }
    }

    fn is_process_alive(vm_id: &str) -> bool {
        let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
        std::path::Path::new(&socket_path).exists()
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

        // Clean up TAP interface
        if !state.tap_name.is_empty() {
            if let Err(e) = NetworkManager::remove_interface(&state.tap_name) {
                warn!("Failed to remove TAP {}: {}", state.tap_name, e);
            }
        }

        // Clean up DM device
        if !state.dm_name.is_empty() {
            if let Err(e) = StorageManager::remove_dm_device(&state.dm_name) {
                warn!("Failed to remove DM {}: {}", state.dm_name, e);
            }
        }

        // Detach loop devices
        for dev in &state.loop_devices {
            if let Err(e) = StorageManager::detach_loop_device(dev) {
                warn!("Failed to detach loop {}: {}", dev, e);
            }
        }

        // Remove cgroup
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
