mod agent;
mod boot;
mod config;
mod image;
mod network;
mod policy;
mod state;
pub mod types;

use anyhow::{Context, Result};
use tracing::{error, info};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;

use crate::state::{AppState, VmInstance, wal::WalEntry};
use types::*;

pub struct VmService {
    state: AppState,
}

impl VmService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub async fn run_vm(&self, request: VmRunRequest) -> Result<VmRunResponse> {
        info!("VmService: Starting VM for image {}", request.image);

        let home = dirs::home_dir().context("No home dir")?;
        let ignite_root = home.join(".ignite");
        let images_root = ignite_root.join("images");
        let vms_root = ignite_root.join("vms");

        std::fs::create_dir_all(&images_root)?;
        std::fs::create_dir_all(&vms_root)?;

        let vm_uuid = uuid::Uuid::new_v4();
        let vm_id = vm_uuid.to_string();
        let cid = (vm_uuid.as_fields().0 % 1000000) + 3;
        let vm_dir = vms_root.join(&vm_id);
        std::fs::create_dir_all(&vm_dir).context("Failed to create VM dir")?;

        crate::api::handlers::git_init(&vm_dir);

        let prepared_image = image::prepare_image(&request.image).await?;

        let storage = storage::prepare_storage(
            &self.state,
            &prepared_image.path,
            &vm_dir,
            &vm_id,
        ).await?;

        let network_config = network::setup_network(
            &self.state,
            &vm_id,
            &request.networks,
        ).await?;

        let agent_config = agent::prepare_agent(
            &storage.dm_device_path,
            &vm_dir,
            &prepared_image.config,
            &self.state,
        ).await?;

        let ch_config = config::build_ch_config(
            &self.state,
            &vm_id,
            &cid,
            &vm_dir,
            &storage.dm_device_path,
            &network_config,
        );

        let (vmm, proxy_tasks, slirp_mgr, fs_managers) = boot::start_vm(
            &ch_config,
            &network_config,
            &request,
            &self.state,
        ).await?;

        policy::check_policy(
            &self.state,
            &vm_id,
            &vm_dir,
        ).await;

        let instance = VmInstance {
            vmm,
            id: vm_id.clone(),
            ch_socket_path: ch_config.socket_path.clone(),
            tap_name: if self.state.rootless {
                String::new()
            } else {
                network_config.primary_tap.clone()
            },
            dm_name: storage.dm_name.clone(),
            loop_devices: storage.loop_devices.clone(),
            cow_file_path: storage.cow_file_path.clone(),
            ip_address: network_config.ip_address.clone(),
            proxy_tasks,
            fs_managers,
            slirp: slirp_mgr,
            cgroup_path: setup_cgroups(&self.state, &vm_id, request.vcpu, request.mem_size_mib)?,
            netns_path: network_config.netns_path.clone(),
            config_ports: request.ports.clone(),
            config_volumes: request.volumes.clone(),
            hostname: request.hostname.clone(),
            labels: request.labels.clone(),
            base_image_path: prepared_image.path.to_string_lossy().to_string(),
            vcpu: request.vcpu,
            mem_size_mib: request.mem_size_mib,
            networks: request.networks.clone(),
        };

        instance.save_state().context("Failed to save state")?;

        {
            let mut vms = self.state.vms.lock().unwrap();
            vms.insert(vm_id.clone(), Arc::new(tokio::sync::Mutex::new(instance)));
        }

        if let Err(e) = self.state.wal.append(&WalEntry::vm_create(vm_id.clone())) {
            error!("Failed to write WAL entry: {}", e);
        }
        if let Err(e) = self.state.wal.append(&WalEntry::vm_start(vm_id.clone())) {
            error!("Failed to write WAL entry: {}", e);
        }

        let _ = self.state.events_tx.send(serde_json::json!({
            "type": "vm_start",
            "id": vm_id.clone(),
            "name": request.labels.get("vyoma.service").unwrap_or(&vm_id)
        }).to_string());

        Ok(VmRunResponse {
            vm_id,
            status: "Running".to_string(),
            ip_address: network_config.ip_address,
        })
    }
}

fn setup_cgroups(
    state: &AppState,
    vm_id: &str,
    vcpu: u32,
    mem_size_mib: u32,
) -> Result<Option<String>> {
    match state.cgroups.create_vm_cgroup(vm_id) {
        Ok(path) => {
            let quota = vcpu * 100;
            if let Err(e) = state.cgroups.set_cpu_limit(vm_id, quota) {
                error!("Failed to set cpu limit: {}", e);
            }
            let mem_bytes = (mem_size_mib as u64) * 1024 * 1024;
            if let Err(e) = state.cgroups.set_memory_limit(vm_id, mem_bytes) {
                error!("Failed to set memory limit: {}", e);
            }
            Ok(Some(path))
        }
        Err(e) => {
            error!("Failed to create cgroup: {}", e);
            Ok(None)
        }
    }
}