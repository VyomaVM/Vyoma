use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};
use vyoma_core::fs::VirtioFsManager;
use vyoma_core::proxy::ProxyManager;
use vyoma_core::qemu::{qemu_binary_name_for_arch, QemuTcgManager};
use vyoma_core::vmm::{AnyVmm, HypervisorMode, VmmBackend, VmmManager};
use vyoma_core::vtpm::VtpmManager;

use super::types::{ChConfig, VmNetworkConfig, VmRunRequest};
use crate::state::AppState;

/// Selected hypervisor binary for a single VM launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedHypervisor {
    CloudHypervisor(String),
    QemuTcg(String),
}

/// Locate an installed `qemu-system-{x86_64|aarch64}` binary if present.
pub fn find_qemu_binary() -> Option<String> {
    let bin = qemu_binary_name_for_arch(std::env::consts::ARCH);
    for dir in ["/usr/bin", "/usr/local/bin", "/bin"] {
        let p = format!("{}/{}", dir, bin);
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    if std::process::Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some(bin.to_string());
    }
    None
}

/// Decide between Cloud Hypervisor KVM and QEMU TCG software emulation.
pub fn select_hypervisor_runtime(
    mode: HypervisorMode,
    has_kvm: bool,
    ch_path: &str,
    ch_exists: bool,
    qemu_bin: Option<String>,
) -> Result<ResolvedHypervisor> {
    match mode {
        HypervisorMode::CloudHypervisor => {
            if !has_kvm {
                anyhow::bail!(
                    "KVM device /dev/kvm not found and hypervisor=cloud-hypervisor requires hardware KVM. Enable nested virtualization (WSL2/Lima) or use --hypervisor=auto / qemu-tcg."
                );
            }
            if !ch_exists {
                anyhow::bail!("Cloud Hypervisor binary not found at {}", ch_path);
            }
            Ok(ResolvedHypervisor::CloudHypervisor(ch_path.to_string()))
        }
        HypervisorMode::QemuTcg => match qemu_bin {
            Some(q) => Ok(ResolvedHypervisor::QemuTcg(q)),
            None => anyhow::bail!(
                "QEMU TCG mode requested, but '{}' was not found in PATH",
                qemu_binary_name_for_arch(std::env::consts::ARCH)
            ),
        },
        HypervisorMode::Auto => {
            if has_kvm && ch_exists {
                Ok(ResolvedHypervisor::CloudHypervisor(ch_path.to_string()))
            } else if let Some(q) = qemu_bin {
                if !has_kvm {
                    warn!("No /dev/kvm found; falling back to QEMU TCG software emulation ({})", q);
                } else {
                    warn!("Cloud Hypervisor not found at {}; falling back to QEMU TCG ({})", ch_path, q);
                }
                Ok(ResolvedHypervisor::QemuTcg(q))
            } else if !has_kvm {
                anyhow::bail!(
                    "KVM device /dev/kvm not found and no QEMU TCG binary ('{}') available. Enable nested virtualization (WSL2 or Apple M3+ Lima), install qemu-system, or connect via `vyoma remote` to a Linux KVM host.",
                    qemu_binary_name_for_arch(std::env::consts::ARCH)
                );
            } else {
                anyhow::bail!("Cloud Hypervisor binary not found at {}", ch_path);
            }
        }
    }
}

/// Determines whether TPM is needed based on policy and request.
fn needs_tpm(state: &AppState, _request: &VmRunRequest) -> bool {
    let policy = state.policy_manager.lock().unwrap();
    policy.must_verify_on_boot()
}

pub async fn start_vm(
    state: &AppState,
    ch_config: &ChConfig,
    network_config: &VmNetworkConfig,
    request: &VmRunRequest,
) -> Result<(AnyVmm, Vec<JoinHandle<()>>, Vec<VirtioFsManager>, Option<VtpmManager>)> {
    let use_tpm = needs_tpm(state, request);
    let mut vtpm_manager: Option<VtpmManager> = None;

    if !std::path::Path::new(&ch_config.kernel_path).exists() {
        anyhow::bail!("Kernel binary not found at {}", ch_config.kernel_path);
    }

    let selection = select_hypervisor_runtime(
        state.hypervisor_mode,
        std::path::Path::new("/dev/kvm").exists(),
        &ch_config.ch_path,
        std::path::Path::new(&ch_config.ch_path).exists(),
        find_qemu_binary(),
    )?;

    // Start vTPM if needed for measured boot attestation
    if use_tpm {
        info!("Starting vTPM for measured boot attestation");
        let vm_id_str = request.labels.get("vm_id").cloned()
            .unwrap_or_else(|| ch_config.socket_path.split('/').last().unwrap_or("unknown").to_string());
        let base_dir = std::path::Path::new(&ch_config.socket_path).parent()
            .unwrap_or(std::path::Path::new("/tmp"));
        match VtpmManager::new(vm_id_str.as_str(), base_dir) {
            Ok(mut vtpm) => {
                if let Err(e) = vtpm.start() {
                    error!("Failed to start vTPM: {}, continuing without TPM", e);
                } else {
                    info!("vTPM started at {}", vtpm.socket_path());
                    vtpm_manager = Some(vtpm);
                }
            }
            Err(e) => {
                error!("Failed to create vTPM manager: {}, continuing without TPM", e);
            }
        }
    }

    let mut vmm: AnyVmm = match &selection {
        ResolvedHypervisor::CloudHypervisor(ch_bin) => {
            info!("Spawning Cloud Hypervisor in KVM mode...");
            let mut ch = VmmManager::new(&ch_config.socket_path);
            ch.start_daemon(ch_bin, None, false)
                .context("Failed to start Cloud Hypervisor")?;
            AnyVmm::CloudHypervisor(ch)
        }
        ResolvedHypervisor::QemuTcg(qemu_bin) => {
            info!("Configuring QEMU TCG software emulation backend ({})...", qemu_bin);
            let mut qemu = QemuTcgManager::new(&ch_config.socket_path);
            qemu.start_daemon(qemu_bin, None, false)
                .context("Failed to configure QEMU TCG")?;
            AnyVmm::QemuTcg(qemu)
        }
    };

    vmm.set_boot_source(&ch_config.kernel_path, &ch_config.boot_args, ch_config.initramfs_path.as_deref()).await
        .context("Boot source")?;

    vmm.add_drive("rootfs", &ch_config.rootfs_path, true).await
        .context("Add drive")?;

    if !network_config.primary_tap.is_empty() {
        vmm.add_network_interface("eth0", &network_config.primary_tap, None).await
            .context("Add net (primary)")?;
    }

    for (idx, network_info) in network_config.network_infos.iter().enumerate().skip(1) {
        let ifname = format!("eth{}", idx);
        if let Err(e) = vmm.add_network_interface(&ifname, &network_info.tap_name, None).await {
            error!("Failed to add network interface {}: {}", ifname, e);
        } else {
            info!("Added network interface {} with TAP {}", ifname, network_info.tap_name);
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

    // Configure TPM if vTPM was started
    if let Some(ref vtpm) = vtpm_manager {
        vmm.set_tpm(&vtpm.socket_path()).await
            .context("Failed to configure TPM")?;
        info!("Configured vTPM at {}", vtpm.socket_path());
    }

    vmm.set_machine_config(request.vcpu, request.mem_size_mib).await
        .context("Machine config")?;

    let serial_path = std::path::PathBuf::from(&ch_config.socket_path)
        .parent().unwrap()
        .join("serial.log");
    vmm.enable_serial(&serial_path.to_string_lossy()).await?;

    match &mut vmm {
        AnyVmm::CloudHypervisor(ch) => {
            ch.start_instance().await.context("Start instance")?;
        }
        AnyVmm::QemuTcg(qemu) => {
            qemu.spawn_configured().context("Start QEMU TCG instance")?;
        }
    }

    let mut proxy_tasks = Vec::new();
    for mapping in &request.ports {
        let handle = ProxyManager::start_vsock_proxy(
            mapping.host_port,
            ch_config.vsock_path.clone(),
            mapping.vm_port,
        );
        proxy_tasks.push(handle);
    }

    Ok((vmm, proxy_tasks, fs_managers, vtpm_manager))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hypervisor_selection_matrix() {
        // 1. Native KVM -> prefers Cloud Hypervisor
        let res = select_hypervisor_runtime(
            HypervisorMode::Auto,
            true,
            "/var/lib/vyoma/bin/cloud-hypervisor",
            true,
            Some("/usr/bin/qemu-system-aarch64".to_string()),
        )
        .unwrap();
        assert_eq!(
            res,
            ResolvedHypervisor::CloudHypervisor("/var/lib/vyoma/bin/cloud-hypervisor".to_string())
        );

        // 2. Non-KVM (Apple M1/M2 Lima VM) -> falls back to QEMU TCG
        let res = select_hypervisor_runtime(
            HypervisorMode::Auto,
            false,
            "/var/lib/vyoma/bin/cloud-hypervisor",
            true,
            Some("/usr/bin/qemu-system-aarch64".to_string()),
        )
        .unwrap();
        assert_eq!(
            res,
            ResolvedHypervisor::QemuTcg("/usr/bin/qemu-system-aarch64".to_string())
        );

        // 3. Explicit cloud-hypervisor fails loudly if /dev/kvm is absent
        assert!(select_hypervisor_runtime(
            HypervisorMode::CloudHypervisor,
            false,
            "/var/lib/vyoma/bin/cloud-hypervisor",
            true,
            Some("/usr/bin/qemu-system-x86_64".to_string()),
        )
        .is_err());
    }
}