use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tracing::{info, error};
use vyoma_core::fs::VirtioFsManager;
use vyoma_core::proxy::ProxyManager;
use vyoma_core::slirp::SlirpManager;
use vyoma_core::vmm::VmmManager;

use super::types::{ChConfig, VmNetworkConfig, VmRunRequest};
use crate::state::AppState;

pub async fn start_vm(
    ch_config: &ChConfig,
    network_config: &VmNetworkConfig,
    request: &VmRunRequest,
    state: &AppState,
) -> Result<(VmmManager, Vec<JoinHandle<()>>, Option<SlirpManager>, Vec<VirtioFsManager>)> {
    let mut vmm = VmmManager::new(&ch_config.socket_path);

    if !std::path::Path::new(&ch_config.kernel_path).exists() {
        anyhow::bail!("Kernel binary not found at {}", ch_config.kernel_path);
    }
    if !std::path::Path::new(&ch_config.ch_path).exists() {
        anyhow::bail!("Cloud Hypervisor binary not found at {}", ch_config.ch_path);
    }

    let mut slirp_mgr = None;

    if state.rootless {
        info!("Spawning Cloud Hypervisor in Rootless Mode...");
        
        vmm.start_daemon(&ch_config.ch_path, None, true)
            .context("Failed to start Cloud Hypervisor (Rootless)")?;

        let pid = vmm.get_pid()
            .context("FC PID not found")?;

        let socket_path = std::path::PathBuf::from(&ch_config.socket_path)
            .parent().unwrap()
            .join("slirp.sock");
        let mut slirp = SlirpManager::new(socket_path.to_string_lossy().as_ref());
        slirp.spawn(pid, "tap0", &request.ports)
            .context("Failed to start slirp")?;
        slirp_mgr = Some(slirp);

        vmm.set_boot_source(&ch_config.kernel_path, &ch_config.boot_args, ch_config.initramfs_path.as_deref()).await
            .context("Boot source")?;
        
        vmm.add_drive("rootfs", &ch_config.rootfs_path, true).await
            .context("Add drive")?;

        vmm.add_network_interface("eth0", "tap0", None).await
            .context("Add net (rootless)")?;
    } else {
        info!("Spawning Cloud Hypervisor in Privileged Mode...");

        vmm.start_daemon(&ch_config.ch_path, None, false)
            .context("Failed to start Cloud Hypervisor")?;

        vmm.set_boot_source(&ch_config.kernel_path, &ch_config.boot_args, ch_config.initramfs_path.as_deref()).await
            .context("Boot source")?;

        vmm.add_drive("rootfs", &ch_config.rootfs_path, true).await
            .context("Add drive")?;

        vmm.add_network_interface("eth0", &network_config.primary_tap, None).await
            .context("Add net (primary)")?;

        for (idx, network_info) in network_config.network_infos.iter().enumerate().skip(1) {
            let ifname = format!("eth{}", idx);
            if let Err(e) = vmm.add_network_interface(&ifname, &network_info.tap_name, None).await {
                error!("Failed to add network interface {}: {}", ifname, e);
            } else {
                info!("Added network interface {} with TAP {}", ifname, network_info.tap_name);
            }
        }
    }

    let vsock_path = ch_config.vsock_path.to_string_lossy().to_string();
    vmm.add_vsock(ch_config.vsock_cid, &vsock_path).await
        .context("Add vsock")?;

    let mut fs_managers = Vec::new();
    for (idx, vol) in request.volumes.iter().enumerate() {
        let tag = format!("vol{}", idx);
        let socket_path = std::path::PathBuf::from(&ch_config.socket_path)
            .parent().unwrap()
            .join(format!("fs_{}.sock", idx));

        let mut fs_mgr = VirtioFsManager::new(&tag, socket_path.to_string_lossy().as_ref());
        
        if let Err(e) = fs_mgr.start(&vol.host_path) {
            let _ = vmm.kill();
            anyhow::bail!("Failed to start virtiofsd for {}: {}", vol.host_path, e);
        }

        vmm.add_file_system(&tag, socket_path.to_string_lossy().as_ref(), &tag).await
            .context("Add fs")?;

        fs_managers.push(fs_mgr);
    }

    vmm.set_machine_config(request.vcpu, request.mem_size_mib).await
        .context("Machine config")?;

    vmm.start_instance().await
        .context("Start instance")?;

    let mut proxy_tasks = Vec::new();
    if !state.rootless {
        for mapping in &request.ports {
            let handle = ProxyManager::start_proxy(
                mapping.host_port,
                network_config.ip_address.clone(),
                mapping.vm_port,
            );
            proxy_tasks.push(handle);
        }
    }

    Ok((vmm, proxy_tasks, slirp_mgr, fs_managers))
}