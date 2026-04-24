use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use tower_http::cors::CorsLayer;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
// use tokio_stream::StreamExt;
use futures::stream::Stream;
use ignite_core::api::{PortMapping, VolumeMount};
use ignite_core::cgroups::CgroupManager;
use ignite_core::fs::VirtioFsManager;
use ignite_core::proxy::ProxyManager;
use ignite_core::vmm::VmmManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use tracing::{error, info, warn};

use crate::cluster;
use crate::dns;
use crate::ui;
use axum::body::Body;
use flate2::read::GzDecoder;
use ignite_core::builder::{IgniteFile, Instruction};
use std::process::Command;
use tar::Archive;
use tokio::task::JoinHandle;
use tokio_util::io::StreamReader;
// use futures::StreamExt as FuturesStreamExt;

use crate::state::{AppState, VmInstance, VmState, wal::WalEntry};




pub async fn shutdown_signal(state: AppState) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let _ = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Signal received, starting graceful shutdown...");

    let ids: Vec<String> = {
        let map = state.vms.lock().unwrap();
        map.keys().cloned().collect()
    };

    if !ids.is_empty() {
        info!("Cleaning up {} active VMs...", ids.len());
        for id in ids {
            let vm_arc = {
                let mut map = state.vms.lock().unwrap();
                map.remove(&id)
            };
            if let Some(vm_mutex) = vm_arc {
                let mut vm = vm_mutex.lock().await;
                vm.cleanup(&state.cni_manager).await;

                // WAL: Log VM stop
        if let Err(e) = state.wal.append(&WalEntry::vm_stop(id.clone())) {
            error!("Failed to write WAL entry: {}", e);
        }
            }
        }
    }
    info!("Graceful shutdown complete.");
}

pub async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
pub struct RunRequest {
    image: String,
    #[serde(default = "default_vcpu")]
    vcpu: u32,
    #[serde(default = "default_mem")]
    mem_size_mib: u32,
    #[serde(default)]
    ports: Vec<PortMapping>,
    #[serde(default)]
    volumes: Vec<VolumeMount>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default)]
    base_image_path: String,
}

fn default_vcpu() -> u32 {
    1
}
fn default_mem() -> u32 {
    512
}

#[derive(Serialize)]
pub struct RunResponse {
    vm_id: String,
    status: String,
    ip_address: String,
}

use ignite_core::layers::LayerManager;
use ignite_core::network::NetworkManager;
use ignite_core::oci::OciManager;
use ignite_core::storage::StorageManager;

pub async fn run_vm(
    State(state): State<AppState>,
    Json(payload): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!("Received request to run image: {}", payload.image);

    // 1. Setup Directories
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let ignite_root = home.join(".ignite");
    let images_root = ignite_root.join("images");
    let vms_root = ignite_root.join("vms");

    std::fs::create_dir_all(&images_root)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    std::fs::create_dir_all(&vms_root)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Helper to parse CNI IP
    fn parse_cni_ip(res: &serde_json::Value) -> anyhow::Result<String> {
        let ips = res
            .get("ips")
            .and_then(|v| v.as_array())
            .ok_or(anyhow::anyhow!("No IPs in CNI result"))?;
        let first = ips.first().ok_or(anyhow::anyhow!("Empty IP list"))?;
        let addr = first
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or(anyhow::anyhow!("No address field"))?;
        // addr is CIDR (e.g. 172.16.0.5/24). Split it.
        let ip = addr.split('/').next().unwrap_or(addr);
        Ok(ip.to_string())
    }

    // 2. Pull Image & Prepare Base
    let base_image_file = ensure_image_locally(&payload.image).await?;
    let base_image_path_str = base_image_file.to_string_lossy().to_string();
    info!("Using base image at {:?}", base_image_file);

    // 3. VM Specific Setup
    let vm_id = uuid::Uuid::new_v4().to_string();
    let vm_dir = vms_root.join(&vm_id);
    std::fs::create_dir_all(&vm_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Initialize Git Repo for Time Travel
    if let Err(e) = git_init(&vm_dir) {
        error!("Failed to init git repo in {:?}: {}", vm_dir, e);
    }

    // Storage Setup
    let (dm_path, loop_devices, cow_file_str) = if state.rootless {
        // Rootless: Simple Copy (No DM/Loop)
        let vm_disk = vm_dir.join("disk.ext4");
        info!("Rootless: Copying base image to {:?}", vm_disk);
        std::fs::copy(&base_image_file, &vm_disk).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Rootless copy: {}", e),
            )
        })?;

        // We use the disk file as the "dm_path" (Firecracker treats it as root block device)
        (
            vm_disk.to_string_lossy().to_string(),
            Vec::new(),
            vm_disk.to_string_lossy().to_string(),
        )
    } else {
        // Privileged: DM Snapshot (COW)
        let cow_file = vm_dir.join("diff.cow");
        let size_mb = 2048;
        StorageManager::create_cow_file(&cow_file, size_mb)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Loop base: {}", e),
            )
        })?;
        let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Loop cow: {}", e),
            )
        })?;

        let dm_name = format!("ign-{}", vm_id);
        let size_sectors = size_mb * 1024 * 1024 / 512;
        let dm_path =
            StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("DM create: {}", e),
                    )
                })?;

        (
            dm_path,
            vec![base_loop, cow_loop],
            cow_file.to_string_lossy().to_string(),
        )
    };

    // Legacy bindings for compilation compatibility if needed
    let dm_name = if state.rootless {
        "rootless".to_string()
    } else {
        format!("ign-{}", vm_id)
    };

    // --- Inject OCI Config via debugfs ---
    let config_path = base_image_file.parent().unwrap().join("ignite-config.json");
    let mut envs = vec![];
    let mut oci_cmd = vec!["/bin/sh".to_string()];
    let mut workdir = "/".to_string();

    if let Ok(config_str) = std::fs::read_to_string(&config_path) {
        if let Ok(oci_config) = serde_json::from_str::<ignite_core::oci::OciImageConfig>(&config_str) {
            oci_cmd = oci_config.full_command();
            if let Some(e) = oci_config.env {
                envs = e;
            }
            if let Some(wd) = oci_config.working_dir {
                if !wd.is_empty() {
                    workdir = wd;
                }
            }
        }
    }

    let mut init_script = String::new();
    init_script.push_str("#!/bin/sh\n");
    init_script.push_str("set -e\n"); // break on errors
    init_script.push_str("mount -t proc proc /proc || true\n");
    init_script.push_str("mount -t sysfs sys /sys || true\n");
    init_script.push_str("mount -t devtmpfs dev /dev || true\n");

    for e in &envs {
        init_script.push_str(&format!("export \"{}\"\n", e.replace('"', "\\\"")));
    }
    
    init_script.push_str(&format!("mkdir -p {}\n", workdir));
    init_script.push_str(&format!("cd {}\n", workdir));
    
    // Very basic shell quoting for execution
    let cmd_str = oci_cmd.into_iter().map(|s| format!("\"{}\"", s.replace('"', "\\\""))).collect::<Vec<_>>().join(" ");
    init_script.push_str(&format!("exec {}\n", cmd_str));

    let temp_init_path = vm_dir.join("ignite-init.sh");
    if let Err(e) = std::fs::write(&temp_init_path, init_script) {
        warn!("Failed to write ignite-init.sh: {}", e);
    } else {
        // debugfs Injection
        let write_debugfs = format!("cd /sbin\nrm ignite-init\nwrite {} ignite-init\nsif ignite-init mode 0755\n", temp_init_path.to_string_lossy());
        let _ = Command::new("sudo")
            .args(&["debugfs", "-w", "-R", &write_debugfs.replace('\n', " -R "), &dm_path])
            .status();
        
        info!("Injected /sbin/ignite-init for {}. CMD: {}", vm_id, cmd_str);
    }
    // -------------------------------------

    // Network Setup
    let (vm_ip, tap_name, netns_path_final) = if state.rootless {
        // Rootless (Slirp4netns)
        // IP is handled by Slirp (usually 10.0.2.15)
        // TAP name is just a label for VmInstance, inside NetNS it is "tap0"
        ("10.0.2.15".to_string(), "tap0".to_string(), None)
    } else {
        // Network (CNI Integration)
        // 1. Create NetNS
        let netns_name = format!("vm-{}", vm_id);
        let netns_path = format!("/var/run/netns/{}", netns_name);

        // Ensure /var/run/netns exists
        std::fs::create_dir_all("/var/run/netns")
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Create netns: ip netns add <name> (Requires root, which we have)
        let _ = Command::new("ip")
            .args(&["netns", "add", &netns_name])
            .output()
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create netns: {}", e),
                )
            })?;

        // 2. Call CNI ADD
        let ifname = "eth0";
        let cni_result = state
            .cni_manager
            .add(None, &vm_id, &netns_path, ifname)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("CNI ADD failed: {}", e),
                )
            })?;

        // 3. Parse Result to get IP
        let ip = parse_cni_ip(&cni_result).unwrap_or_else(|e| {
            error!("Failed to parse CNI IP: {}", e);
            "172.16.0.0".to_string()
        });
        info!("CNI assigned IP: {}", ip);

        // Legacy Tap Name (Host Side for Hybrid)
        let tap_legacy = format!("tap{}", &vm_id[0..8]);

        (ip, tap_legacy, Some(netns_path))
    };

    // 3.5 Cgroups
    let cgroup_path = match state.cgroups.create_vm_cgroup(&vm_id) {
        Ok(p) => Some(p),
        Err(e) => {
            error!("Failed to create cgroup: {}", e);
            None
        }
    };

    if let Some(_) = &cgroup_path {
        // vCPU limit (if payload.vcpu > 0)
        // Assume VCPU 1 = 100%
        let quota = payload.vcpu * 100; // 1 vCPU = 100%
        if let Err(e) = state.cgroups.set_cpu_limit(&vm_id, quota) {
            error!("Failed to set cpu limit: {}", e);
        }

        let mem_bytes = (payload.mem_size_mib as u64) * 1024 * 1024;
        if let Err(e) = state.cgroups.set_memory_limit(&vm_id, mem_bytes) {
            error!("Failed to set memory limit: {}", e);
        }
    }

    // 3.6 Start Port Proxies
    let mut proxy_tasks = Vec::new();
    if !state.rootless {
        for mapping in &payload.ports {
            let handle =
                ProxyManager::start_proxy(mapping.host_port, vm_ip.clone(), mapping.vm_port);
            proxy_tasks.push(handle);
        }
    } else {
        info!("Rootless: Skipping ProxyManager. Ports are mapped by slirp4netns.");
    }

    let boot_args = format!("console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw ip={}::{}:255.255.255.0:{}:eth0:off:{} init=/sbin/ignite-init", 
        vm_ip, "172.16.0.1", vm_id, "172.16.0.1");
    // "ip=<client-ip>:<server-ip>:<gw-ip>:<netmask>:<hostname>:<device>:<autoconf>:<dns0-ip>"
    // Correct kernel format: ip=172.16.0.X::172.16.0.1:255.255.255.0:myvm:eth0:off:172.16.0.1

    // 4. Firecracker VMM
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    let mut vmm = VmmManager::new(&socket_path);

    // Kernel and Firecracker paths from data_dir
    let kernel_path = format!("{}/bin/vmlinux", state.data_dir);
    let firecracker_path = format!("{}/bin/firecracker", state.data_dir);
    if !std::path::Path::new(&kernel_path).exists() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Kernel binary not found at {}", kernel_path).into(),
        ));
    }
    if !std::path::Path::new(&firecracker_path).exists() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Firecracker binary not found at {}", firecracker_path).into(),
        ));
    }

    let mut slirp_mgr = None;

    if state.rootless {
        // Rootless Start
        info!("Spawning Firecracker in Rootless Mode (unshare -r -n)...");
        if let Err(e) = vmm.start_daemon(&firecracker_path, None, true) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start FC (Rootless): {}", e),
            ));
        }

        // Get PID for Slirp
        let pid = vmm
            .get_pid()
            .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "FC PID not found".into()))?;

        // Spawn Slirp
        let socket_path = vm_dir.join("slirp.sock");
        let mut slirp =
            ignite_core::slirp::SlirpManager::new(socket_path.to_string_lossy().as_ref());
        if let Err(e) = slirp.spawn(pid, "tap0", &payload.ports) {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start slirp: {}", e),
            ));
        }
        slirp_mgr = Some(slirp);

        // Config FC
        if let Err(e) = vmm.set_boot_source(&kernel_path, &boot_args).await {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Boot source: {}", e),
            ));
        }

        if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Add drive: {}", e),
            ));
        }

        // Network: Interface inside NetNS is "tap0" created by Slirp
        if let Err(e) = vmm.add_network_interface("eth0", "tap0", None).await {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Add net (rootless): {}", e),
            ));
        }
    } else {
        // Root/CNI Start (Hybrid Mode)
        if let Err(e) = vmm.start_daemon(&firecracker_path, None, false) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start FC: {}", e),
            ));
        }

        // Config
        if let Err(e) = vmm.set_boot_source(&kernel_path, &boot_args).await {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Boot source: {}", e),
            ));
        }

        if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Add drive: {}", e),
            ));
        }

        // Network: Interface on Host is `tap_name`
        if let Err(e) = vmm.add_network_interface("eth0", &tap_name, None).await {
            let _ = vmm.kill();
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add net: {}", e)));
        }
    }

    // Add Volumes
    let mut fs_managers = Vec::new();
    for (idx, vol) in payload.volumes.iter().enumerate() {
        let tag = format!("vol{}", idx);
        let socket_path = vm_dir.join(format!("fs_{}.sock", idx));

        let mut fs_mgr = VirtioFsManager::new(&tag, socket_path.to_string_lossy().as_ref());
        // Start virtiofsd
        if let Err(e) = fs_mgr.start(&vol.host_path) {
            let _ = vmm.kill();
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to start virtiofsd for {}: {}", vol.host_path, e),
            ));
        }

        // Add to FC
        if let Err(e) = vmm
            .add_file_system(&tag, socket_path.to_string_lossy().as_ref(), &tag)
            .await
        {
            let _ = vmm.kill();
            let _ = fs_mgr.kill();
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add fs: {}", e)));
        }

        fs_managers.push(fs_mgr);
    }

    if let Err(e) = vmm
        .set_machine_config(payload.vcpu, payload.mem_size_mib)
        .await
    {
        let _ = vmm.kill();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Machine config: {}", e),
        ));
    }

    if let Err(e) = vmm.start_instance().await {
        let _ = vmm.kill();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Start instance: {}", e),
        ));
    }

    // Success - Store State

    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        fc_socket_path: socket_path.clone(),
        tap_name: if state.rootless {
            String::new()
        } else {
            tap_name.to_string()
        },
        dm_name: dm_name.clone(),
        loop_devices,
        cow_file_path: cow_file_str,
        ip_address: vm_ip.clone(),
        proxy_tasks,
        fs_managers,
        slirp: slirp_mgr,

        cgroup_path,
        netns_path: netns_path_final,
        config_ports: payload.ports.clone(),
        config_volumes: payload.volumes.clone(),
        hostname: payload.hostname.clone(),
        labels: payload.labels.clone(),
        base_image_path: base_image_path_str,
        vcpu: payload.vcpu,
        mem_size_mib: payload.mem_size_mib,
    };

    instance.save_state().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save state: {}", e),
        )
    })?;

    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(instance)));

    // WAL: Log VM creation and start
    if let Err(e) = state.wal.append(&WalEntry::vm_create(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }
    if let Err(e) = state.wal.append(&WalEntry::vm_start(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }
    }

    let _ = state.events_tx.send(serde_json::json!({
        "type": "vm_start",
        "id": vm_id,
        "name": payload.labels.get("ignite.service").unwrap_or(&vm_id)
    }).to_string());

    Ok(Json(RunResponse {
        vm_id,
        status: "Running".to_string(),
        ip_address: vm_ip,
    }))
}

pub async fn stop_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to stop VM: {}", id);

    let vm_arc = {
        let mut vms = state.vms.lock().unwrap();
        vms.remove(&id)
    };

    if let Some(vm_mutex) = vm_arc {
        let mut vm = vm_mutex.lock().await;
        vm.cleanup(&state.cni_manager).await;

        // WAL: Log VM stop
        if let Err(e) = state.wal.append(&WalEntry::vm_stop(id.clone())) {
            error!("Failed to write WAL entry: {}", e);
        }
        
        let _ = state.events_tx.send(serde_json::json!({
            "type": "vm_stop",
            "id": id
        }).to_string());
        
        Ok(format!("VM {} stopped and cleaned up", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

pub async fn pause_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to pause VM: {}", id);

    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;
        info!("Pause VM {}: fc_socket_path={}", id, vm.fc_socket_path);
        vm.vmm
            .pause_instance_with_socket(&vm.fc_socket_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} paused", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

pub async fn resume_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to resume VM: {}", id);

    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;
        vm.vmm
            .resume_instance_with_socket(&vm.fc_socket_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} resumed", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

pub async fn snapshot_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to snapshot VM: {}", id);

    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;

        // Define paths
        let home =
            dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
        let vm_dir = home.join(".ignite").join("vms").join(&id);
        let snapshot_path = vm_dir.join("snapshot.snap");
        let mem_path = vm_dir.join("memory.mem");
        let fc_socket = vm.fc_socket_path.clone();

        // 1. Pause VM (Required for snapshot)
        vm.vmm.pause_instance_with_socket(&fc_socket).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to pause: {}", e),
            )
        })?;

        // 2. Create Snapshot
        let snap_result = vm
            .vmm
            .create_snapshot(
                snapshot_path.to_string_lossy().as_ref(),
                mem_path.to_string_lossy().as_ref(),
            )
            .await;

        // 3. Resume VM (Always resume, even if snapshot failed)
        if let Err(e) = vm.vmm.resume_instance_with_socket(&fc_socket).await {
            error!("Failed to resume VM {} after snapshot: {}", id, e);
        }

        snap_result.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Snapshot failed: {}", e),
            )
        })?;

        // Commit snapshot to Git
        if let Err(e) = git_commit(&vm_dir, &format!("Snapshot {}", id)) {
            error!("Failed to git commit snapshot: {}", e);
        }

        Ok(format!("Snapshot created for VM {}", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

#[derive(Deserialize)]
pub struct RestoreRequest {
    snapshot_path: String,
    mem_path: String,
    cow_path: String, // Path to the existing COW file to restore from
    // For MVP, we presume the disk state (COW) is already in a known location or passed here.
    #[allow(dead_code)]
    original_vm_id: String, // To find the original disk resources
}

pub async fn restore_vm(
    State(state): State<AppState>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!(
        "Request to restore VM from snapshot: {}",
        payload.snapshot_path
    );

    // 1. Setup New VM State
    // We need to CLONE the disk state or reusing it?
    // "Teleportation" usually implies moving.
    // "Restoration" (Backup) implies reusing.

    // Let's implement a "Clone from Snapshot" flow.
    // We need a backing disk. We can look up the original VM's base image path if we had persistence.
    // Since we don't have DB persistence, this is tricky.
    // HACK: We will assume we are restoring `alpine:latest` for this MVP demo,
    // reusing the same base image path logic as run_vm.

    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let ignite_root = home.join(".ignite");
    let images_root = ignite_root.join("images");
    let vms_root = ignite_root.join("vms");

    // Assume alpine:latest base for now (Simplification for MVP Validation)
    let safe_image_name = "alpine_latest";
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");

    // 2. New VM ID & Dir
    let vm_id = uuid::Uuid::new_v4().to_string();
    let vm_dir = vms_root.join(&vm_id);
    std::fs::create_dir_all(&vm_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. New COW File
    let cow_file = vm_dir.join("diff.cow");

    // Copy existing COW state directly
    info!("Restoring disk state from {}", payload.cow_path);
    std::fs::copy(&payload.cow_path, &cow_file).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to copy COW file: {}", e),
        )
    })?;

    // We don't create empty cow file anymore
    // StorageManager::create_cow_file(&cow_file, size_mb)...

    // 4. Setup Storage Stack
    let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Loop base: {}", e),
        )
    })?;
    let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Loop cow: {}", e),
        )
    })?;

    let dm_name = format!("ign-{}", vm_id);
    let size_mb = 2048; // Must match original
    let size_sectors = size_mb * 1024 * 1024 / 512;
    let dm_path = StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DM create: {}", e),
            )
        })?;

    // 5. Setup Network
    // Ensure bridge exists (idempotent-ish)
    let bridge_name = "ign0";
    let bridge_cidr = "172.16.0.1/24";
    // We skip bridge setup here assuming it's up from previous run, or we should just call it safe.
    // NetworkManager::setup_bridge(bridge_name, bridge_cidr)...

    let tap_name = format!("tap{}", &vm_id[0..8]);
    NetworkManager::setup_tap(&tap_name, bridge_name).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("TAP setup: {}", e),
        )
    })?;

    let random_octet = rand::random::<u8>();
    let safe_octet = std::cmp::max(2, std::cmp::min(254, random_octet));
    let vm_ip = format!("172.16.0.{}", safe_octet);

    // 6. Firecracker VMM
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    let mut vmm = VmmManager::new(&socket_path);

    if let Err(e) = vmm.start_daemon(&format!("{}/bin/firecracker", state.data_dir), None, state.rootless) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start FC: {}", e),
        ));
    }

    // 7. Load Snapshot INSTEAD of Boot Source
    if let Err(e) = vmm
        .load_snapshot(&payload.snapshot_path, &payload.mem_path)
        .await
    {
        let _ = vmm.kill();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Load snapshot: {}", e),
        ));
    }

    // 8. Add Drives (Must match configuration of snapped VM usually, but we are attaching NEW cow)
    // Firecracker snapshot restoration often involves re-attaching block devices.
    // The device ID "rootfs" must match.
    if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
        let _ = vmm.kill();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Add drive: {}", e),
        ));
    }

    // 9. Add Network
    if let Err(e) = vmm.add_network_interface("eth0", &tap_name, None).await {
        let _ = vmm.kill();
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add net: {}", e)));
    }

    // 10. Resume
    if let Err(e) = vmm.resume_instance().await {
        let _ = vmm.kill();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Resume instance: {}", e),
        ));
    }

    // Success - Store State
    let proxy_tasks = Vec::new(); // No ports restored for now
    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        fc_socket_path: socket_path.clone(),
        tap_name: tap_name.clone(),
        dm_name: dm_name.clone(),
        slirp: None,
        loop_devices: vec![base_loop, cow_loop],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        ip_address: vm_ip.clone(),
        proxy_tasks,

        fs_managers: Vec::new(),
        cgroup_path: None, // Restored VMs need cgroups too? Yes.
        // For MVP, simplistic restore skips cgroup enforcement or needs logic duplication.
        // TODO: Isolate create_resources logic.
        netns_path: None, // Simplified restore lacks CNI for now
        config_ports: vec![],
        config_volumes: vec![],
        hostname: None,
        labels: HashMap::new(),
        base_image_path: String::new(),
        vcpu: 1,
        mem_size_mib: 512,
    };

    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(instance)));

    // WAL: Log VM creation and start
    if let Err(e) = state.wal.append(&WalEntry::vm_create(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }
    if let Err(e) = state.wal.append(&WalEntry::vm_start(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }
    }

    Ok(Json(RunResponse {
        vm_id,
        status: "Restored".to_string(),
        ip_address: vm_ip,
    }))
}

#[derive(Serialize)]
struct VmSummary {
    id: String,
    ip_address: String,
    hostname: Option<String>,
    labels: HashMap<String, String>,

    // For restart
    base_image_path: String,
    vcpu: u32,
    mem_size_mib: u32,
}

#[derive(Serialize)]
pub struct ListResponse {
    vms: Vec<VmSummary>,
}

#[derive(Serialize)]
pub struct PullResponse {
    status: String,
    path: String,
}

#[derive(Deserialize)]
pub struct PullRequest {
    image: String,
}

pub async fn pull_image_handler(
    Json(payload): Json<PullRequest>,
) -> Result<Json<PullResponse>, (StatusCode, String)> {
    info!("Handling Pull request for {}", payload.image);

    let path = ensure_image_locally(&payload.image).await?;

    Ok(Json(PullResponse {
        status: "Image pulled successfully".to_string(),
        path: path.to_string_lossy().to_string(),
    }))
}

pub async fn list_vms(State(state): State<AppState>) -> Json<ListResponse> {
    let instances: Vec<Arc<TokioMutex<VmInstance>>> = {
        let vms_map = state.vms.lock().unwrap();
        vms_map.values().cloned().collect()
    };

    let mut summaries = Vec::new();
    for arc_inst in instances {
        let inst = arc_inst.lock().await;
        summaries.push(VmSummary {
            id: inst.id.clone(),
            ip_address: inst.ip_address.clone(),
            hostname: inst.hostname.clone(),
            labels: inst.labels.clone(),
            base_image_path: inst.base_image_path.clone(),
            vcpu: inst.vcpu,
            mem_size_mib: inst.mem_size_mib,
        });
    }

    Json(ListResponse { vms: summaries })
}

// Git Helper Functions
fn git_init(path: &std::path::Path) -> std::io::Result<()> {
    Command::new("git").arg("init").current_dir(path).output()?;

    Command::new("git")
        .args(&["config", "user.email", "ignite@daemon"])
        .current_dir(path)
        .output()?;

    Command::new("git")
        .args(&["config", "user.name", "Ignite Daemon"])
        .current_dir(path)
        .output()?;
    Ok(())
}

fn git_commit(path: &std::path::Path, message: &str) -> std::io::Result<()> {
    Command::new("git")
        .arg("add")
        .arg(".")
        .current_dir(path)
        .output()?;

    Command::new("git")
        .arg("commit")
        .arg("--allow-empty")
        .arg("-m")
        .arg(message)
        .current_dir(path)
        .output()?;
    Ok(())
}

pub async fn stream_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    info!("Request to stream logs for VM: {}", id);

    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;
        let rx = vm.vmm.subscribe_logs();
        use tokio_stream::StreamExt;

        // Transform broadcast receiver into SSE stream
        let stream = BroadcastStream::new(rx).filter_map(|try_msg| {
            match try_msg {
                Ok(msg) => Some(Ok(Event::default().data(msg))),
                Err(_) => None, // Skip missed messages
            }
        });

        Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

pub async fn inspect_vm_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    // 1. Check running
    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;
        // ... build json ...
        let val = serde_json::json!({
            "id": vm.id,
            "tap_name": vm.tap_name,
            "dm_name": vm.dm_name,
            "loop_devices": vm.loop_devices,
            "cow_file_path": vm.cow_file_path,
            "ip_address": vm.ip_address,
            "cgroup_path": vm.cgroup_path,
            "netns_path": vm.netns_path,
            "ports": vm.config_ports,
            "volumes": vm.config_volumes,
            "hostname": vm.hostname,
            "labels": vm.labels,
            "base_image_path": vm.base_image_path,
            "vcpu": vm.vcpu,
            "mem_size_mib": vm.mem_size_mib,
        });
        return Ok(val.to_string());
    }

    // 2. Check disk (stopped)
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let state_file = home
        .join(".ignite")
        .join("vms")
        .join(&id)
        .join("state.json");
    if state_file.exists() {
        let f = std::fs::File::open(state_file)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let s: serde_json::Value = serde_json::from_reader(f)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        return Ok(s.to_string());
    }

    Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
}

pub async fn build_image(
    State(_state): State<AppState>,
    body: Body,
) -> Result<String, (StatusCode, String)> {
    info!("Received build request");

    // 1. Stream body to a temp file (tar.gz)
    let temp_dir =
        tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tar_path = temp_dir.path().join("context.tar.gz");

    {
        // Convert Body stream to AsyncRead
        use futures::StreamExt;
        let stream = body
            .into_data_stream()
            .map(|b| b.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
        let mut reader = StreamReader::new(stream);
        let mut file = tokio::fs::File::create(&tar_path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tokio::io::copy(&mut reader, &mut file)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // 2. Unpack
    let tar_file = std::fs::File::open(&tar_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let decoder = GzDecoder::new(tar_file);
    let mut archive = Archive::new(decoder);
    let context_dir = temp_dir.path().join("context");
    std::fs::create_dir(&context_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    archive.unpack(&context_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Unpack failed: {}", e),
        )
    })?;

    // 3. Parse Ignitefile
    let ignitefile_path = context_dir.join("Ignitefile");
    if !ignitefile_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Ignitefile not found in build context".to_string(),
        ));
    }

    let ignite_file = IgniteFile::parse(&ignitefile_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse error: {}", e)))?;

    // 4. Build Process
    // We need to track the "current image base".
    // 1. FROM -> Pull image, setup as current base.
    //    We need to work on a COPY of the base, not the shared base.
    //    So we create a new "build artifact" (a raw ext4 file).

    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let images_root = home.join(".ignite").join("images");
    std::fs::create_dir_all(&images_root)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let build_id = uuid::Uuid::new_v4().to_string();
    let image_store_path = images_root.join(&build_id);
    std::fs::create_dir_all(&image_store_path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let current_image_path = image_store_path.join("base.ext4");

    let current_image_path = image_store_path.join("base.ext4");

    // Track if we have a base
    let mut has_base = false;
    let mut oci_config = ignite_core::oci::OciImageConfig::default();

    for instr in ignite_file.instructions {
        match instr {
            Instruction::From(image) => {
                info!("Building FROM {}", image);

                let base_cache = ensure_image_locally(&image).await?;

                // Copy base cache to current_image_path
                std::fs::copy(&base_cache, &current_image_path).map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Failed to copy base: {}", e),
                    )
                })?;
                has_base = true;
            }
            Instruction::Run(cmd) => {
                if !has_base {
                    return Err((StatusCode::BAD_REQUEST, "RUN before FROM".to_string()));
                }
                info!("RUN: {}", cmd);

                // Mount image
                let loop_device = StorageManager::setup_loop_device(&current_image_path)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                // Mount the loop device to a temp dir
                let mount_point = tempfile::tempdir()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_status = Command::new("sudo")
                    .args(&["mount", &loop_device, mount_point.path().to_str().unwrap()])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                if !mount_status.success() {
                    let _ = StorageManager::detach_loop_device(&loop_device);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to mount for RUN".to_string(),
                    ));
                }

                // Prepare resolv.conf for networking in chroot
                // COPY host resolv.conf to chroot
                let _ = std::fs::copy(
                    "/etc/resolv.conf",
                    mount_point.path().join("etc/resolv.conf"),
                );

                // Execute chroot
                // cmd string might need splitting or sh -c
                let chroot_status = Command::new("sudo")
                    .args(&[
                        "chroot",
                        mount_point.path().to_str().unwrap(),
                        "/bin/sh",
                        "-c",
                        &cmd,
                    ])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                // Unmount
                let umount_status = Command::new("sudo")
                    .args(&["umount", mount_point.path().to_str().unwrap()])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                let _ = StorageManager::detach_loop_device(&loop_device);

                if !chroot_status.success() {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        format!("RUN command failed: {}", cmd),
                    ));
                }
            }
            Instruction::Copy { src, dest } => {
                if !has_base {
                    return Err((StatusCode::BAD_REQUEST, "COPY before FROM".to_string()));
                }
                info!("COPY {} -> {}", src, dest);

                // Mount
                let loop_device = StorageManager::setup_loop_device(&current_image_path)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_point = tempfile::tempdir()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_status = Command::new("sudo")
                    .args(&["mount", &loop_device, mount_point.path().to_str().unwrap()])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                if !mount_status.success() {
                    let _ = StorageManager::detach_loop_device(&loop_device);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to mount for COPY".to_string(),
                    ));
                }

                // Perform Copy
                // src is relative to context_dir
                let src_path = context_dir.join(&src);
                // dest is relative to mount_point
                // handle absolute dest paths by stripping leading /
                let safe_dest = dest.trim_start_matches('/');
                let dest_path = mount_point.path().join(safe_dest);

                // Ensure parent dir exists
                if let Some(parent) = dest_path.parent() {
                    let _ = std::fs::create_dir_all(parent); // Ignore error (might need sudo?)
                }

                // Use sudo cp to preserve permissions if simple copy fails?
                // Or just fs::copy since we are running as root (daemon)
                // Daemon runs as root?
                // Yes, per prerequisites.
                if src_path.is_dir() {
                    // Recursive copy not implemented in std::fs
                    // Use Command cp -r
                    let cp_status = Command::new("cp")
                        .arg("-r")
                        .arg(&src_path)
                        .arg(&dest_path)
                        .status()
                        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    if !cp_status.success() {
                        let _ = Command::new("sudo")
                            .args(&["umount", mount_point.path().to_str().unwrap()])
                            .status();
                        let _ = StorageManager::detach_loop_device(&loop_device);
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to copy dir".to_string(),
                        ));
                    }
                } else {
                    match std::fs::copy(&src_path, &dest_path) {
                        Ok(_) => {}
                        Err(e) => {
                            // clean up
                            let _ = Command::new("sudo")
                                .args(&["umount", mount_point.path().to_str().unwrap()])
                                .status();
                            let _ = StorageManager::detach_loop_device(&loop_device);
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Failed copy file: {}", e),
                            ));
                        }
                    }
                }

                let _ = Command::new("sudo")
                    .args(&["umount", mount_point.path().to_str().unwrap()])
                    .status();
                let _ = StorageManager::detach_loop_device(&loop_device);
            }
            Instruction::Cmd(args) => {
                info!("CMD {:?}", args);
                oci_config.cmd = Some(args);
            }
            Instruction::Entrypoint(args) => {
                info!("ENTRYPOINT {:?}", args);
                oci_config.entrypoint = Some(args);
            }
            Instruction::Env { key, value } => {
                info!("ENV {}={}", key, value);
                let current_envs = oci_config.env.get_or_insert_with(Vec::new);
                current_envs.push(format!("{}={}", key, value));
            }
        }
    }

    // Save the built configuration
    let config_path = image_store_path.join("ignite-config.json");
    if let Ok(json_str) = serde_json::to_string_pretty(&oci_config) {
        if let Err(e) = std::fs::write(&config_path, json_str) {
            warn!("Failed to write ignite-config.json: {}", e);
        }
    }

    // Done. The `current_image_path` is the result.
    // Move it to images dir? Or return ID?
    // For now return path.
    Ok(build_id)
}

pub async fn ensure_image_locally(
    image_name: &str,
) -> Result<std::path::PathBuf, (StatusCode, String)> {
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let images_root = home.join(".ignite").join("images");
    std::fs::create_dir_all(&images_root)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let safe_image_name = image_name.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");

    if !base_image_file.exists() {
        info!("Image {} not found locally. Pulling...", image_name);
        std::fs::create_dir_all(&image_store_path)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut oci = OciManager::new();
        let manifest_json = oci
            .pull_manifest(image_name)
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Pull failed: {}", e)))?;

        let layers = oci
            .parse_layers(&manifest_json)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Ok(config_digest) = oci.parse_config_digest(&manifest_json) {
            info!("Fetching OCI config blob: {}", config_digest);
            if let Ok(config) = oci.pull_config_blob(image_name, &config_digest).await {
                let config_path = image_store_path.join("ignite-config.json");
                if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                    if let Err(e) = std::fs::write(&config_path, json_str) {
                        warn!("Failed to write ignite-config.json: {}", e);
                    } else {
                        info!("Saved OCI configuration to {:?}", config_path);
                    }
                }
            }
        }

        let temp_unpack_dir =
            tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        for digest in layers {
            let layer_data = oci.pull_layer(image_name, &digest).await.map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    format!("Failed layer {}: {}", digest, e),
                )
            })?;
            LayerManager::unpack_layer(&layer_data, temp_unpack_dir.path()).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unpack failed: {}", e),
                )
            })?;
        }

        let size_mb = 2048;
        StorageManager::create_empty_file(&base_image_file, size_mb)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::format_ext4(&base_image_file)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::populate_image(&base_image_file, temp_unpack_dir.path()).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Populate failed: {}", e),
            )
        })?;
    } else {
        info!("Image found locally at {:?}", base_image_file);
    }

    Ok(base_image_file)
}

pub async fn initialize_state(state: &AppState) {
    info!("Recovery: Scanning for existing VMs...");
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let vms_dir = home.join(".ignite").join("vms");
    if !vms_dir.exists() {
        return;
    }

    let entries = match std::fs::read_dir(vms_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }

        // Check for state.json
        let state_path = entry.path().join("state.json");
        if !state_path.exists() {
            continue;
        } // Not a managed VM or incomplete

        // Load State
        let content = match std::fs::read_to_string(&state_path) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read state for {:?}: {}", entry.path(), e);
                continue;
            }
        };

        let vm_state: VmState = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to parse state for {:?}: {}", entry.path(), e);
                continue;
            }
        };

        // Reconstruct VmmManager
        let socket_path = format!("/tmp/firecracker_{}.socket", vm_state.id);
        let vmm = VmmManager::new(&socket_path);

        // Check Alive
        if vmm.check_alive().await {
            info!("Recovery: VM {} is ALIVE. Adopting...", vm_state.id);

            // Reconstruct Proxy Tasks - Restart them
            let mut proxy_tasks = Vec::new();
            for p in &vm_state.ports {
                let t =
                    ProxyManager::start_proxy(p.host_port, vm_state.ip_address.clone(), p.vm_port);
                proxy_tasks.push(t);
            }

            // Reconstruct FS Managers (Stateless)
            let mut fs_managers = Vec::new();
            for (idx, _vol) in vm_state.volumes.iter().enumerate() {
                let tag = format!("vol{}", idx);
                let path = entry.path().join(format!("fs_{}.sock", idx));
                fs_managers.push(VirtioFsManager::new(&tag, path.to_string_lossy().as_ref()));
            }

            let instance = VmInstance {
                vmm,
                id: vm_state.id.clone(),
                fc_socket_path: socket_path.clone(),
                tap_name: vm_state.tap_name,
                dm_name: vm_state.dm_name,
                loop_devices: vm_state.loop_devices,
                cow_file_path: vm_state.cow_file_path,
                ip_address: vm_state.ip_address,
                proxy_tasks,
                fs_managers,
                slirp: None,
                cgroup_path: vm_state.cgroup_path,
                netns_path: vm_state.netns_path,
                config_ports: vm_state.ports,
                config_volumes: vm_state.volumes,
                hostname: vm_state.hostname,
                labels: vm_state.labels.clone(),
                base_image_path: vm_state.base_image_path.clone(),
                vcpu: vm_state.vcpu,
                mem_size_mib: vm_state.mem_size_mib,
            };

            state
                .vms
                .lock()
                .unwrap()
                .insert(vm_state.id, Arc::new(TokioMutex::new(instance)));
        } else {
            info!(
                "Recovery: VM {} found but DEAD. cleaning up artifacts...",
                vm_state.id
            );
            // Cleanup dead VM
            let mut instance = VmInstance {
                vmm,
                id: vm_state.id.clone(),
                fc_socket_path: socket_path,
                tap_name: vm_state.tap_name,
                dm_name: vm_state.dm_name,
                loop_devices: vm_state.loop_devices,
                cow_file_path: vm_state.cow_file_path,
                ip_address: vm_state.ip_address,
                proxy_tasks: vec![],
                fs_managers: vec![],
                slirp: None,
                cgroup_path: vm_state.cgroup_path,
                netns_path: vm_state.netns_path,
                config_ports: vec![],
                config_volumes: vec![],
                hostname: vm_state.hostname,
                labels: vm_state.labels,
                base_image_path: vm_state.base_image_path,
                vcpu: vm_state.vcpu,
                mem_size_mib: vm_state.mem_size_mib,
            };
            // We await cleanup
            instance.cleanup(&state.cni_manager).await;

            // Also delete the state file so we don't loop on it next time?
            // cleanup() removes the VM dir?
            // VM dir is in ~/.ignite/vms/<id>. cleanup usually removes things but maybe not the dir itself?
            // ignite_core::storage/vmm doesn't remove the VM home dir automatically?
            // Let's check  logic.
            // It calls remove_dm_device, detach loop, etc.
            // But it doesn't remove the state.json or the ID directory.

            // Let's remove the directory to be clean.
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub async fn start_process_monitor(state: AppState) {
    tokio::spawn(async move {
        // Check every 5 seconds
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            interval.tick().await;

            let ids: Vec<String> = {
                let map = state.vms.lock().unwrap();
                map.keys().cloned().collect()
            };

            for id in ids {
                // Get VM Arc
                let vm_arc = {
                    let map = state.vms.lock().unwrap();
                    map.get(&id).cloned()
                };

                if let Some(vm_mutex) = vm_arc {
                    let mut vm = vm_mutex.lock().await;

                    // 1. Check Firecracker
                    match vm.vmm.try_wait() {
                        Ok(Some(status)) => {
                            let mut msg = format!(
                                "Monitor: VM {} Firecracker process EXITED (Reaped): {}",
                                id, status
                            );

                            // Check for OOM
                            if let Ok(count) = state.cgroups.get_oom_kill_count(&id) {
                                if count > 0 {
                                    msg.push_str(&format!(
                                        " [WARNING: OOM Kill Detected: {}]",
                                        count
                                    ));
                                }
                            }
                            error!("{}", msg);
                        }
                        Err(e) => error!("Monitor Check Error for VMM {}: {}", id, e),
                        Ok(None) => {}
                    }

                    // 2. Check VirtioFS
                    let fs_count = vm.fs_managers.len();
                    for idx in 0..fs_count {
                        // Check status with scoped mutable borrow
                        let exit_status = {
                            let fs = &mut vm.fs_managers[idx];
                            match fs.try_wait() {
                                Ok(Some(s)) => Some(s),
                                Err(e) => {
                                    error!("Monitor Check Error for FS {}: {}", id, e);
                                    None
                                }
                                Ok(None) => None,
                            }
                        };

                        if let Some(status) = exit_status {
                            warn!(
                                "Monitor: VM {} VirtioFS (vol{}) EXITED with {}. Restarting...",
                                id, idx, status
                            );

                            // Get Config (Immutable borrow OK now)
                            let host_path = vm.config_volumes.get(idx).map(|v| v.host_path.clone());

                            if let Some(path) = host_path {
                                let fs = &mut vm.fs_managers[idx]; // Re-borrow mutably
                                if let Err(e) = fs.start(&path) {
                                    error!(
                                        "Monitor: FAILED to auto-restart VirtioFS for VM {}: {}",
                                        id, e
                                    );
                                } else {
                                    info!("Monitor: RESTARTED VirtioFS for VM {} (vol{})", id, idx);
                                }
                            } else {
                                error!(
                                    "Monitor: Cannot restart VirtioFS (vol{}), config not found.",
                                    idx
                                );
                            }
                        }
                    }
                }
            }
        }
    });
}

#[derive(Deserialize)]
pub struct CreateNetworkRequest {
    name: String,
    subnet: String,
    #[serde(default = "default_driver")]
    driver: String,
}

fn default_driver() -> String {
    "bridge".to_string()
}

pub async fn list_networks_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    state
        .cni_manager
        .list_networks()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn create_network_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateNetworkRequest>,
) -> Result<String, (StatusCode, String)> {
    match payload.driver.as_str() {
        "bridge" => state
            .cni_manager
            .create_network(&payload.name, &payload.subnet)
            .map(|_| format!("Network {} created", payload.name))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        "overlay" => state
            .cni_manager
            .create_overlay_network(&payload.name, &payload.subnet)
            .map(|_| format!("Overlay Network {} created", payload.name))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown driver: {}", payload.driver),
        )),
    }
}

pub async fn delete_network_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<String, (StatusCode, String)> {
    state
        .cni_manager
        .delete_network(&name)
        .map(|_| format!("Network {} deleted", name))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct JoinRequest {
    seed_ip: String,
}

pub async fn swarm_init_handler(State(state): State<AppState>) -> Result<String, (StatusCode, String)> {
    let id = state.cluster_manager.init();
    Ok(format!("Initialized Swarm Node: {}", id))
}

pub async fn swarm_join_handler(
    State(state): State<AppState>,
    Json(payload): Json<JoinRequest>,
) -> Result<String, (StatusCode, String)> {
    state
        .cluster_manager
        .join(&payload.seed_ip)
        .await
        .map(|_| format!("Joined swarm via seed {}", payload.seed_ip))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub assigned: cluster::NodeInfo,
    pub peers: Vec<cluster::NodeInfo>,
}

pub async fn swarm_register_handler(
    State(state): State<AppState>,
    Json(node): Json<cluster::NodeInfo>,
) -> Json<RegisterResponse> {
    // If we are Seed (or Acting Leader), we allocate.
    // Logic: cluster_manager.handle_registration(node)
    let (assigned, peers) = state.cluster_manager.handle_registration(node);
    Json(RegisterResponse { assigned, peers })
}


pub async fn swarm_nodes_handler(State(state): State<AppState>) -> Json<Vec<cluster::NodeInfo>> {
    Json(state.cluster_manager.list_nodes())
}

pub async fn events_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use tokio_stream::StreamExt;
    let rx = state.events_tx.subscribe();
    let stream = BroadcastStream::new(rx);

    Sse::new(
        stream.map(|msg| {
            match msg {
                Ok(data) => Ok(Event::default().data(data)),
                Err(_) => Ok(Event::default().comment("missed message")),
            }
        })
    )
    .keep_alive(axum::response::sse::KeepAlive::default())
}

// --- Extended API Handlers for Web UI ---

pub async fn list_images_handler() -> Json<Vec<String>> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let images_root = home.join(".ignite").join("images");
    let mut images = Vec::new();
    if let Ok(entries) = std::fs::read_dir(images_root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                 if let Ok(name) = entry.file_name().into_string() {
                     images.push(name);
                 }
            }
        }
    }
    Json(images)
}



#[derive(Serialize)]
pub struct VolumeInfo {
    name: String,
    path: String,
}

pub async fn list_volumes_handler() -> Json<Vec<VolumeInfo>> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let vol_root = home.join(".ignite").join("volumes");
    let mut vols = Vec::new();
    if let Ok(entries) = std::fs::read_dir(vol_root) {
        for entry in entries.flatten() {
             if let Ok(name) = entry.file_name().into_string() {
                  vols.push(VolumeInfo { name: name, path: entry.path().to_string_lossy().to_string() });
             }
        }
    }
    Json(vols)
}

#[derive(Deserialize)]
pub struct TeleportRequest {
    pub vm_id: String,
    pub target_node_ip: String,
}

pub async fn teleport_handler(
    State(state): State<AppState>,
    Json(payload): Json<TeleportRequest>,
) -> Result<String, (StatusCode, String)> {
    info!("Handling POST /teleport for VM {} to {}", payload.vm_id, payload.target_node_ip);
    
    // 1. Get VM
    let vm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&payload.vm_id).cloned()
    };

    if let Some(vm_mutex) = vm_arc {
        let vm = vm_mutex.lock().await;

        let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
        let vm_dir = home.join(".ignite").join("vms").join(&payload.vm_id);
        let snapshot_path = vm_dir.join("snapshot.snap");
        let mem_path = vm_dir.join("memory.mem");
        let fc_socket = vm.fc_socket_path.clone();

        // Pause and snapshot
        info!("Pausing VM {} for teleportation...", payload.vm_id);
        vm.vmm.pause_instance_with_socket(&fc_socket).await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to pause: {}", e))
        })?;

        info!("Creating memory snapshot...");
        vm.vmm.create_snapshot(
            snapshot_path.to_string_lossy().as_ref(),
            mem_path.to_string_lossy().as_ref(),
        ).await.map_err(|e| {
            // Attempt to resume if snapshot fails
            let _ = futures::executor::block_on(vm.vmm.resume_instance_with_socket(&fc_socket));
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Snapshot failed: {}", e))
        })?;
        
        let mem_size = vm.mem_size_mib as u64 * 1024 * 1024;
        
        // Spawn Teleport Sender
        let target_url = format!("http://{}:7071", payload.target_node_ip);
        let teleporter = ignite_teleport::sender::Teleporter::new(payload.vm_id.clone(), target_url, mem_size);
        
        // Teleport the VM memory & state asynchronously
        let teleport_vm_id = payload.vm_id.clone();
        tokio::spawn(async move {
            match teleporter.teleport_vm(mem_path, snapshot_path).await {
                Ok(_) => info!("Teleportation of VM {} succeeded!", teleport_vm_id),
                Err(e) => error!("Teleportation of VM {} failed: {}", teleport_vm_id, e),
            }
        });

        Ok(format!("Teleportation initiated for VM {}", payload.vm_id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}
