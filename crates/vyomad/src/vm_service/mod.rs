mod agent;
pub mod boot;
pub mod build;
pub mod config;
pub mod image;
pub(crate) mod network;
pub mod policy;
pub mod state;
pub mod types;
pub mod storage;
pub mod mocks;
#[cfg(test)]
mod stage_tests;

use std::sync::Arc;
use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::state::{AppState, VmInstance, VmStatus, wal::WalEntry};
use crate::vm_service::policy::update_vm_status;
use types::{VmRunRequest, VmRunResponse, PreparedStorage, VmNetworkConfig};

struct VmCreationContext {
    state: Arc<AppState>,
    vm_id: String,
    vm_dir: Option<std::path::PathBuf>,
    storage: Option<PreparedStorage>,
    network_config: Option<VmNetworkConfig>,
    vm_created: bool,
}

impl VmCreationContext {
    fn new(state: Arc<AppState>, vm_id: String) -> Self {
        Self {
            state,
            vm_id,
            vm_dir: None,
            storage: None,
            network_config: None,
            vm_created: false,
        }
    }

    async fn cleanup_on_failure(&mut self) {
        let vm_id = &self.vm_id;
        warn!("Cleaning up resources after failure for VM {}", vm_id);

        if let Some(ref network) = self.network_config {
            info!("Cleaning up network for VM {}", vm_id);
            let networks: Vec<String> = if network.network_infos.is_empty() {
                vec![]
            } else {
                network.network_infos.iter()
                    .map(|n| n.network_name.clone())
                    .collect()
            };
            let _ = network::cleanup_network(&self.state, vm_id, &networks, &network.netns_path).await;
        }

        if let Some(ref storage) = self.storage {
            info!("Cleaning up storage for VM {}", vm_id);
            let _ = storage::cleanup_storage(storage).await;
        }

        if let Some(ref vm_dir) = self.vm_dir {
            if vm_dir.exists() {
                info!("Cleaning up VM directory {:?}", vm_dir);
                let _ = std::fs::remove_dir_all(vm_dir);
            }
        }

        info!("Cleanup completed for VM {}", vm_id);
    }
}

pub async fn run_vm(state: Arc<AppState>, request: VmRunRequest) -> Result<VmRunResponse> {
    info!("VmService: Starting VM for image {}", request.image);

    let home = dirs::home_dir().context("No home dir")?;
    let vyoma_root = home.join(".vyoma");
    let images_root = vyoma_root.join("images");
    let vms_root = vyoma_root.join("vms");

    std::fs::create_dir_all(&images_root)?;
    std::fs::create_dir_all(&vms_root)?;

    let vm_uuid = uuid::Uuid::new_v4();
    let vm_id = vm_uuid.to_string();
    let cid = (vm_uuid.as_fields().0 % 1000000) + 3;
    let vm_dir = vms_root.join(&vm_id);

    let mut ctx = VmCreationContext::new(Arc::clone(&state), vm_id.clone());
    ctx.vm_dir = Some(vm_dir.clone());

    std::fs::create_dir_all(&vm_dir).context("Failed to create VM dir")?;

    let _ = git_init(&vm_dir);

    let prepared_image = match image::prepare_image(&request.image).await {
        Ok(img) => img,
        Err(e) => {
            ctx.cleanup_on_failure().await;
            return Err(e);
        }
    };

    let storage = match storage::prepare_storage(
        &state,
        &prepared_image.rootfs_sqfs_path,
        &vm_dir,
        &vm_id,
    ).await {
        Ok(s) => {
            ctx.storage = Some(s.clone());
            s
        }
        Err(e) => {
            ctx.cleanup_on_failure().await;
            return Err(e);
        }
    };

    let network_config = match network::setup_network(
        &state,
        &vm_id,
        &request.networks,
    ).await {
        Ok(n) => {
            ctx.network_config = Some(n.clone());
            n
        }
        Err(e) => {
            ctx.cleanup_on_failure().await;
            return Err(e);
        }
    };

    let agent_config = agent::prepare_agent(
        &state,
        &storage.dm_device_path,
        &vm_dir,
        &prepared_image.config,
    ).await?;

    let kernel_path = image::resolve_kernel_from_manifest(&prepared_image.manifest, &state.data_dir)
        .unwrap_or_else(|| image::get_default_kernel_path(&state.data_dir));

    let ch_config = config::build_ch_config(
        &state,
        &vm_id,
        &cid,
        &vm_dir,
        &storage.dm_device_path,
        &network_config,
        &agent_config,
        &kernel_path,
    );

    let (vmm, proxy_tasks, slirp_mgr, fs_managers, mut vtpm_manager) = match boot::start_vm(
        &state,
        &ch_config,
        &network_config,
        &request,
    ).await {
        Ok(result) => result,
        Err(e) => {
            ctx.cleanup_on_failure().await;
            return Err(e);
        }
    };

    // Start attestation check asynchronously
    let state_clone = Arc::clone(&state);
    let vm_id_clone = vm_id.clone();
    let vm_dir_clone = vm_dir.clone();
    let image_name = prepared_image.manifest.as_ref()
        .and_then(|m| m.labels.get("vyoma.image.name"))
        .unwrap_or(&vm_id)
        .clone();
    let tpm_socket_path = vtpm_manager.as_ref().map(|vtpm| vtpm.socket_path().to_string());

    // Check if attestation is required before spawning
    let needs_attestation = {
        let policy = state.policy_manager.lock().unwrap();
        policy.must_verify_on_boot()
    };

    if needs_attestation {
        info!("Measured boot attestation required for VM {}, TPM socket: {:?}", vm_id, tpm_socket_path);
        tokio::spawn(async move {
            if let Err(e) = policy::check_policy_and_perform_attestation(
                state_clone,
                vm_id_clone,
                vm_dir_clone,
                image_name,
                tpm_socket_path,
            ).await {
                error!("Attestation process failed: {}", e);
            }
        });
    } else {
        // No attestation required, mark as running immediately
        // But still stop the vTPM if one was started
        if let Some(vtpm) = &mut vtpm_manager {
            let _ = vtpm.stop();
        }
        update_vm_status(&state, &vm_id, VmStatus::Running).await
            .map_err(|e| anyhow::anyhow!("Failed to update VM status: {}", e))?;
    }

    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        status: VmStatus::PendingAttestation,
        ch_socket_path: ch_config.socket_path.clone(),
        tap_name: if state.rootless {
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
        vtpm_manager,
        cgroup_path: setup_cgroups(&state, &vm_id, request.vcpu, request.mem_size_mib)?,
        netns_path: network_config.netns_path.clone(),
        config_ports: request.ports.clone(),
        config_volumes: request.volumes.clone(),
        hostname: request.hostname.clone(),
        labels: request.labels.clone(),
        base_image_path: prepared_image.rootfs_sqfs_path.to_string_lossy().to_string(),
        vcpu: request.vcpu,
        mem_size_mib: request.mem_size_mib,
        networks: request.networks.clone(),
    };

    instance.save_state().context("Failed to save state")?;

    let status_string = match &instance.status {
        VmStatus::PendingAttestation => "PendingAttestation".to_string(),
        VmStatus::Running => "Running".to_string(),
        VmStatus::Error { reason } => format!("Error: {}", reason),
    };

    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(tokio::sync::Mutex::new(instance)));
    }

    let _ = state.events_tx.send(serde_json::json!({
        "type": "vm_start",
        "id": vm_id.clone(),
        "name": request.labels.get("vyoma.service").unwrap_or(&vm_id)
    }).to_string());

    Ok(VmRunResponse {
        vm_id,
        status: status_string,
        ip_address: network_config.ip_address,
    })
}

fn git_init(path: &std::path::Path) -> std::io::Result<()> {
    let git_dir = path.join(".git");
    if git_dir.exists() {
        return Ok(());
    }
    std::fs::create_dir(&git_dir)?;
    std::fs::write(git_dir.join("config"), "[core]\n\trepositoryformatversion = 0\n")?;
    std::fs::write(git_dir.join("description"), "Vyoma VM\n")?;
    std::fs::create_dir(git_dir.join("objects"))?;
    std::fs::create_dir(git_dir.join("refs"))?;
    Ok(())
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