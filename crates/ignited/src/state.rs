use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{broadcast, Mutex as TokioMutex};
use tokio::task::JoinHandle;
use serde::{Deserialize, Serialize};

use ignite_core::api::{PortMapping, VolumeMount};
use ignite_core::cgroups::CgroupManager;
use ignite_core::fs::VirtioFsManager;
use ignite_core::vmm::VmmManager;

use crate::cluster;

pub mod wal;
pub mod recovery;

#[derive(Clone)]
pub struct AppState {
    pub vms: Arc<StdMutex<HashMap<String, Arc<TokioMutex<VmInstance>>>>>,
    pub cgroups: Arc<CgroupManager>,
    pub cni_manager: Arc<ignite_core::cni::CniManager>,
    pub cluster_manager: Arc<cluster::ClusterManager>,
    pub rootless: bool,
    pub events_tx: broadcast::Sender<String>,
    pub wal: Arc<wal::Wal>,
    pub data_dir: String,
}

#[derive(Debug)]
pub struct VmInstance {
    pub vmm: VmmManager,
    pub id: String,
    pub fc_socket_path: String,
    pub tap_name: String,
    pub dm_name: String,
    pub loop_devices: Vec<String>,
    pub cow_file_path: String,
    pub ip_address: String,
    pub proxy_tasks: Vec<JoinHandle<()>>,

    pub fs_managers: Vec<VirtioFsManager>,
    #[allow(dead_code)]
    pub slirp: Option<ignite_core::slirp::SlirpManager>,
    pub cgroup_path: Option<String>,
    pub netns_path: Option<String>,

    pub config_ports: Vec<PortMapping>,
    pub config_volumes: Vec<VolumeMount>,
    pub hostname: Option<String>,
    pub labels: HashMap<String, String>,

    pub base_image_path: String,
    pub vcpu: u32,
    pub mem_size_mib: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VmState {
    pub id: String,
    pub tap_name: String,
    pub dm_name: String,
    pub loop_devices: Vec<String>,
    pub cow_file_path: String,
    pub ip_address: String,
    pub cgroup_path: Option<String>,
    pub netns_path: Option<String>,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
    pub hostname: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub base_image_path: String,
    #[serde(default)]
    pub vcpu: u32,
    #[serde(default)]
    pub mem_size_mib: u32,
}

use tracing::{info, error};
use std::process::Command;
use ignite_core::network::NetworkManager;
use ignite_core::storage::StorageManager;

impl VmInstance {
    pub fn save_state(&self) -> anyhow::Result<()> {
        let state = VmState {
            id: self.id.clone(),
            tap_name: self.tap_name.clone(),
            dm_name: self.dm_name.clone(),
            loop_devices: self.loop_devices.clone(),
            cow_file_path: self.cow_file_path.clone(),
            ip_address: self.ip_address.clone(),
            cgroup_path: self.cgroup_path.clone(),
            netns_path: self.netns_path.clone(),
            ports: self.config_ports.clone(),
            volumes: self.config_volumes.clone(),
            hostname: self.hostname.clone(),
            labels: self.labels.clone(),
            base_image_path: self.base_image_path.clone(),
            vcpu: self.vcpu,
            mem_size_mib: self.mem_size_mib,
        };

        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
        let vm_dir = home.join(".ignite").join("vms").join(&self.id);
        if !vm_dir.exists() {
            std::fs::create_dir_all(&vm_dir)?;
        }
        let state_path = vm_dir.join("state.json");
        let f = std::fs::File::create(state_path)?;
        serde_json::to_writer_pretty(f, &state)?;
        Ok(())
    }

    pub async fn cleanup(&mut self, cni_manager: &ignite_core::cni::CniManager) {
        info!("Cleaning up VM resources for {}", self.id);

        // 1. Kill VMM
        if let Err(e) = self.vmm.kill() {
            error!("Failed to kill VMM: {}", e);
        }

        // 2. Remove Network Interface / CNI
        if let Some(netns) = &self.netns_path {
            if let Err(e) = cni_manager.del(None, &self.id, netns, "eth0") {
                error!("CNI DEL failed: {}", e);
            }
            let netns_name = format!("vm-{}", self.id);
            let _ = Command::new("ip")
                .args(&["netns", "delete", &netns_name])
                .output();
        }

        if !self.tap_name.is_empty() {
            if let Err(e) = NetworkManager::remove_interface(&self.tap_name) {
                error!("Failed to remove TAP {}: {}", self.tap_name, e);
            }
        }

        // 3. Remove DM Device
        if let Err(e) = StorageManager::remove_dm_device(&self.dm_name) {
            error!("Failed to remove DM {}: {}", self.dm_name, e);
        }

        // 4. Detach Loop Devices
        for dev in &self.loop_devices {
            if let Err(e) = StorageManager::detach_loop_device(dev) {
                error!("Failed to detach loop {}: {}", dev, e);
            }
        }

        // 5. Remove COW file
        if std::path::Path::new(&self.cow_file_path).exists() {
            let _ = std::fs::remove_file(&self.cow_file_path);
        }

        // 6. Abort Proxy Tasks
        for task in &self.proxy_tasks {
            task.abort();
        }

        // 7. Remove Cgroup
        if let Some(_path) = &self.cgroup_path {
            let cm = CgroupManager::new();
            if let Err(e) = cm.remove_vm_cgroup(&self.id) {
                error!("Failed to remove cgroup for {}: {}", self.id, e);
            }
        }

        // 8. Kill VirtioFs Managers (ADR-025)
        for fs_mgr in &mut self.fs_managers {
            if let Err(e) = fs_mgr.kill() {
                error!("Failed to kill virtiofsd: {}", e);
            }
        }
    }
}
