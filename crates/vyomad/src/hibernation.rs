use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HibernationInfo {
    pub vm_id: String,
    pub hib_dir: PathBuf,
    pub snap_path: PathBuf,
    pub mem_path: PathBuf,
    pub preserved_ip: Option<IpAddr>,
    pub tap_device: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl HibernationInfo {
    pub fn new(vm_id: String, hib_dir: PathBuf, snap_path: PathBuf, mem_path: PathBuf) -> Self {
        Self {
            vm_id,
            hib_dir,
            snap_path,
            mem_path,
            preserved_ip: None,
            tap_device: None,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_ip(mut self, ip: IpAddr) -> Self {
        self.preserved_ip = Some(ip);
        self
    }

    pub fn with_tap_device(mut self, device: String) -> Self {
        self.tap_device = Some(device);
        self
    }

    pub fn hib_dir(&self) -> &PathBuf {
        &self.hib_dir
    }

    pub fn is_valid(&self) -> bool {
        self.hib_dir.exists() && self.snap_path.exists() && self.mem_path.exists()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmStatus {
    Running {
        pid: u32,
        fc_socket: PathBuf,
    },
    Stopped,
    Hibernated {
        hib_dir: PathBuf,
        snap_path: PathBuf,
        mem_path: PathBuf,
    },
    Paused,
}

pub struct VmState {
    pub vm_id: String,
    pub status: VmStatus,
    pub ip: Option<IpAddr>,
    pub tap_device: Option<String>,
    pub vcpus: u32,
    pub memory_mb: u64,
}

impl VmState {
    pub fn new(vm_id: String) -> Self {
        Self {
            vm_id,
            status: VmStatus::Stopped,
            ip: None,
            tap_device: None,
            vcpus: 0,
            memory_mb: 0,
        }
    }

    pub fn is_hibernated(&self) -> bool {
        matches!(self.status, VmStatus::Hibernated { .. })
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, VmStatus::Running { .. })
    }

    pub fn hibernate(&mut self, hib_info: HibernationInfo) -> Result<(), String> {
        if !self.is_running() {
            return Err("VM is not running".to_string());
        }

        let hib_dir = hib_info.hib_dir.clone();
        let snap_path = hib_info.snap_path.clone();
        let mem_path = hib_info.mem_path.clone();

        self.status = VmStatus::Hibernated {
            hib_dir,
            snap_path,
            mem_path,
        };

        info!("VM {} hibernated successfully", self.vm_id);
        Ok(())
    }

    pub fn resume(&mut self, fc_socket: PathBuf, pid: u32) -> Result<(), String> {
        if !self.is_hibernated() {
            return Err("VM is not hibernated".to_string());
        }

        self.status = VmStatus::Running { pid, fc_socket };

        info!("VM {} resumed from hibernation", self.vm_id);
        Ok(())
    }
}

pub struct HibernationManager {
    hibernating_vms: HashMap<String, HibernationInfo>,
    hibernation_dir: PathBuf,
}

impl HibernationManager {
    pub fn new(hibernation_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&hibernation_dir).ok();

        Self {
            hibernating_vms: HashMap::new(),
            hibernation_dir,
        }
    }

    pub fn prepare_hibernation(&self, vm_id: &str) -> Result<HibernationInfo, String> {
        let hib_dir = self.hibernation_dir.join(vm_id);
        std::fs::create_dir_all(&hib_dir)
            .map_err(|e| format!("Failed to create hibernation directory: {}", e))?;

        let snap_path = hib_dir.join("vm.snap");
        let mem_path = hib_dir.join("vm.mem");

        let info = HibernationInfo::new(vm_id.to_string(), hib_dir, snap_path, mem_path);

        info!("Prepared hibernation for VM {}", vm_id);

        Ok(info)
    }

    pub fn store_hibernation_info(&mut self, info: HibernationInfo) {
        self.hibernating_vms.insert(info.vm_id.clone(), info);
    }

    pub fn get_hibernation_info(&self, vm_id: &str) -> Option<&HibernationInfo> {
        self.hibernating_vms.get(vm_id)
    }

    pub fn remove_hibernation_info(&mut self, vm_id: &str) -> Option<HibernationInfo> {
        self.hibernating_vms.remove(vm_id)
    }

    pub fn list_hibernating_vms(&self) -> Vec<String> {
        self.hibernating_vms.keys().cloned().collect()
    }

    pub fn cleanup_hibernation_files(&self, vm_id: &str) -> Result<(), String> {
        let info = self
            .hibernating_vms
            .get(vm_id)
            .ok_or("No hibernation info found")?;

        if info.hib_dir.exists() {
            std::fs::remove_dir_all(&info.hib_dir)
                .map_err(|e| format!("Failed to cleanup hibernation files: {}", e))?;
        }

        info!("Cleaned up hibernation files for VM {}", vm_id);

        Ok(())
    }

    pub fn validate_hibernation(&self, vm_id: &str) -> Result<bool, String> {
        let info = self
            .hibernating_vms
            .get(vm_id)
            .ok_or("No hibernation info found")?;

        Ok(info.is_valid())
    }
}

impl Default for HibernationManager {
    fn default() -> Self {
        Self::new(PathBuf::from("/var/lib/vyoma/hibernation"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_hibernation_info_creation() {
        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        );

        assert_eq!(info.vm_id, "vm-1");
        assert!(info.preserved_ip.is_none());
    }

    #[test]
    fn test_hibernation_info_with_ip() {
        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        )
        .with_ip(Ipv4Addr::new(172, 16, 0, 2).into());

        assert!(info.preserved_ip.is_some());
    }

    #[test]
    fn test_hibernation_info_with_tap() {
        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        )
        .with_tap_device("tap0".to_string());

        assert!(info.tap_device.is_some());
    }

    #[test]
    fn test_vm_state_creation() {
        let state = VmState::new("vm-1".to_string());

        assert_eq!(state.vm_id, "vm-1");
        assert!(!state.is_running());
        assert!(!state.is_hibernated());
    }

    #[test]
    fn test_vm_state_hibernate() {
        let mut state = VmState::new("vm-1".to_string());
        state.status = VmStatus::Running {
            pid: 1234,
            fc_socket: PathBuf::from("/tmp/fc.sock"),
        };

        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        );

        state.hibernate(info).unwrap();

        assert!(state.is_hibernated());
    }

    #[test]
    fn test_vm_state_resume() {
        let mut state = VmState::new("vm-1".to_string());
        state.status = VmStatus::Hibernated {
            hib_dir: PathBuf::from("/hib/vm-1"),
            snap_path: PathBuf::from("/hib/vm-1/vm.snap"),
            mem_path: PathBuf::from("/hib/vm-1/vm.mem"),
        };

        state.resume(PathBuf::from("/tmp/fc.sock"), 5678).unwrap();

        assert!(state.is_running());
    }

    #[test]
    fn test_hibernate_non_running_vm() {
        let mut state = VmState::new("vm-1".to_string());

        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        );

        let result = state.hibernate(info);
        assert!(result.is_err());
    }

    #[test]
    fn test_hibernate_manager_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = HibernationManager::new(temp_dir.path().to_path_buf());

        let vms = manager.list_hibernating_vms();
        assert!(vms.is_empty());
    }

    #[test]
    fn test_prepare_hibernation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = HibernationManager::new(temp_dir.path().to_path_buf());

        let result = manager.prepare_hibernation("vm-1");
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.vm_id, "vm-1");
    }

    #[test]
    fn test_store_and_get_hibernation_info() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut manager = HibernationManager::new(temp_dir.path().to_path_buf());

        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        );

        manager.store_hibernation_info(info.clone());

        let retrieved = manager.get_hibernation_info("vm-1");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_remove_hibernation_info() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut manager = HibernationManager::new(temp_dir.path().to_path_buf());

        let info = HibernationInfo::new(
            "vm-1".to_string(),
            PathBuf::from("/hib/vm-1"),
            PathBuf::from("/hib/vm-1/vm.snap"),
            PathBuf::from("/hib/vm-1/vm.mem"),
        );

        manager.store_hibernation_info(info);

        let removed = manager.remove_hibernation_info("vm-1");
        assert!(removed.is_some());
        assert!(manager.get_hibernation_info("vm-1").is_none());
    }
}
