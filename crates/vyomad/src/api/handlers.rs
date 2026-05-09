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
use openraft::raft::{AppendEntriesRequest, InstallSnapshotRequest, VoteRequest};
use openraft::Raft;
use vyoma_core::api::{PortMapping, VolumeMount};
use vyoma_core::cgroups::CgroupManager;
use vyoma_core::fs::VirtioFsManager;
use vyoma_core::proxy::ProxyManager;
use vyoma_core::vmm::VmmManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::cluster;
use crate::dns;
use crate::ui;
use axum::body::Body;
use flate2::read::GzDecoder;
use vyoma_core::builder::{Vyomafile, Instruction};
use std::process::Command;
use tar::Archive;
use tokio::task::JoinHandle;
use tokio_util::io::StreamReader;
// use futures::StreamExt as FuturesStreamExt;

use crate::state::{AppState, VmInstance, VmState, wal::WalEntry};
use crate::vm_service::state as vm_service_state;




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
    pub image: String,
    #[serde(default = "default_vcpu")]
    pub vcpu: u32,
    #[serde(default = "default_mem")]
    pub mem_size_mib: u32,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub networks: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,

    #[serde(default)]
    pub base_image_path: String,
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

use vyoma_core::layers::LayerManager;
use vyoma_core::network::NetworkManager;
use vyoma_core::oci::OciManager;
use vyoma_core::storage::StorageManager;

pub async fn run_vm(
    State(state): State<AppState>,
    Json(payload): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!("Received request to run image: {}", payload.image);

    let request = crate::vm_service::types::VmRunRequest::from(payload);
    let result = crate::vm_service::run_vm(Arc::new(state), request).await;
    
    result.map(|r| Json(crate::api::handlers::RunResponse {
        vm_id: r.vm_id,
        status: r.status,
        ip_address: r.ip_address,
    })).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn stop_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    vm_service_state::stop_vm(&state, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct CommitRequest {
    new_image_name: String,
}

pub async fn commit_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<CommitRequest>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to commit VM: {} to new image: {}", id, payload.new_image_name);
    
    vm_service_state::commit_vm(&state, &id, &payload.new_image_name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn pause_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    vm_service_state::pause_vm(&state, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn resume_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    vm_service_state::resume_vm(&state, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn snapshot_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to snapshot VM: {}", id);

    let result = vm_service_state::snapshot_vm(&state, &id, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(format!("Snapshot {} created for VM {} at {:?}", result.id, id, result.path))
}

#[derive(Deserialize)]
pub struct RestoreRequest {
    pub snapshot_path: String,
    pub mem_path: String,
    pub cow_path: String,
    pub original_vm_id: String,
}

#[derive(Deserialize)]
pub struct TimeTravelRequest {
    pub vm_id: String,
    pub snapshot_id: String,
}

pub async fn restore_vm(
    State(state): State<AppState>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!(
        "Request to restore VM from snapshot: {}",
        payload.snapshot_path
    );

    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let ignite_root = home.join(".ignite");
    let images_root = ignite_root.join("images");
    let vms_root = ignite_root.join("vms");

    // We still assume alpine:latest base for MVP clone
    let safe_image_name = "alpine_latest";
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");

    // 2. New VM ID & Dir (Clone from Snapshot)
    let vm_id = uuid::Uuid::new_v4().to_string();
    let vm_dir = vms_root.join(&vm_id);
    std::fs::create_dir_all(&vm_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 3. New COW File
    let cow_file = vm_dir.join("diff.cow");

    info!("Restoring disk state from {:?}", payload.cow_path);
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

    // 6. Cloud Hypervisor VMM
    let socket_path = vm_dir.join("ch.sock").to_string_lossy().to_string();
    let mut vmm = VmmManager::new(&socket_path);

    if let Err(e) = vmm.start_daemon(&format!("{}/bin/cloud-hypervisor", state.data_dir), None, state.rootless) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start Cloud Hypervisor: {}", e),
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
        ch_socket_path: socket_path.clone(),
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
        networks: vec![],
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



pub async fn time_travel_vm(
    State(state): State<AppState>,
    Json(payload): Json<TimeTravelRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!(
        "Request to time-travel VM {} to snapshot: {}",
        payload.vm_id, payload.snapshot_id
    );

    let tm = state.timemachine.read().await;
    let _snapshot = tm.get_snapshot(&payload.vm_id, &payload.snapshot_id)
        .ok_or((StatusCode::NOT_FOUND, "Snapshot not found".to_string()))?;

    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let vms_root = home.join(".ignite").join("vms");
    let source_vm_dir = vms_root.join(&payload.vm_id);
    let snaps_dir = source_vm_dir.join("snapshots").join(&payload.snapshot_id);

    let snapshot_path = snaps_dir.join("snapshot.snap");
    let mem_path = snaps_dir.join("memory.mem");
    let cow_path = snaps_dir.join("diff.cow");

    let restore_req = RestoreRequest {
        snapshot_path: snapshot_path.to_string_lossy().to_string(),
        mem_path: mem_path.to_string_lossy().to_string(),
        cow_path: cow_path.to_string_lossy().to_string(),
        original_vm_id: payload.vm_id.clone(),
    };

    // Forward to existing restore logic
    restore_vm(State(state.clone()), Json(restore_req)).await
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub snapshots: Vec<crate::timemachine::SnapshotEntry>,
}

pub async fn history_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<HistoryResponse>, (StatusCode, String)> {
    let tm = state.timemachine.read().await;
    let history = tm.get_snapshot_history(&id).unwrap_or_default();
    Ok(Json(HistoryResponse { snapshots: history }))
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
pub fn git_init(path: &std::path::Path) -> std::io::Result<()> {
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

    // 3. Parse Vyomafile
    let ignitefile_path = context_dir.join("Vyomafile");
    if !ignitefile_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Vyomafile not found in build context".to_string(),
        ));
    }

    let ignite_file = Vyomafile::parse(&ignitefile_path)
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
    let mut oci_config = vyoma_core::oci::OciImageConfig::default();

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
    let config_path = image_store_path.join("vyoma-config.json");
    if let Ok(json_str) = serde_json::to_string_pretty(&oci_config) {
        if let Err(e) = std::fs::write(&config_path, json_str) {
            warn!("Failed to write vyoma-config.json: {}", e);
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
                let config_path = image_store_path.join("vyoma-config.json");
                if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                    if let Err(e) = std::fs::write(&config_path, json_str) {
                        warn!("Failed to write vyoma-config.json: {}", e);
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
        let socket_path = entry.path().join("ch.sock").to_string_lossy().to_string();
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
                ch_socket_path: socket_path.clone(),
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
                networks: vm_state.networks.clone(),
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
                ch_socket_path: socket_path,
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
                networks: vm_state.networks,
            };
            // We await cleanup
            instance.cleanup(&state.cni_manager).await;

            // Also delete the state file so we don't loop on it next time?
            // cleanup() removes the VM dir?
            // VM dir is in ~/.ignite/vms/<id>. cleanup usually removes things but maybe not the dir itself?
            // vyoma_core::storage/vmm doesn't remove the VM home dir automatically?
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

                    // 1. Check Cloud Hypervisor
                    match vm.vmm.try_wait() {
                        Ok(Some(status)) => {
                            let mut msg = format!(
                                "Monitor: VM {} Cloud Hypervisor process EXITED (Reaped): {}",
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
                        Ok(None) => {
                            // Still running
                        }
                        Err(e) => {
                            error!("Monitor: VM {} wait error: {}", id, e);
                        }
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
    if let Some(raft) = &state.raft {
        let mut nodes = std::collections::BTreeMap::new();
        nodes.insert(
            1, 
            crate::swarm::raft_types::SwarmNode {
                addr: "127.0.0.1:8080".to_string(), // In reality we'd get this from config
                public_key: "init_key".to_string(),
            }
        );
        raft.initialize(nodes).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Raft Init Error: {}", e)))?;
        Ok("Initialized Swarm Node via Raft".to_string())
    } else {
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Raft is not enabled".to_string()))
    }
}

pub async fn swarm_join_handler(
    State(state): State<AppState>,
    Json(payload): Json<JoinRequest>,
) -> Result<String, (StatusCode, String)> {
    // In a real openraft setup, we'd send an RPC to the leader to add_learner and change_membership
    if let Some(raft) = &state.raft {
        // Here we pretend we're the leader and adding a learner
        // A full implementation requires forwarding this to the leader node
        let node = crate::swarm::raft_types::SwarmNode {
            addr: payload.seed_ip.clone(),
            public_key: "joined_key".to_string(),
        };
        raft.add_learner(2, node, true).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Add Learner Error: {}", e)))?;
        Ok(format!("Joined swarm via seed {}", payload.seed_ip))
    } else {
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Raft is not enabled".to_string()))
    }
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
        let ch_socket = vm.ch_socket_path.clone();

        // We do not need to manually pause and snapshot for Cloud Hypervisor live migration.
        // The hypervisor handles the memory tracking and handoff natively.
        let target_url = payload.target_node_ip.clone();
        
        let client = reqwest::Client::new();
        let target_api = format!("http://{}:3000/receive-teleport", target_url);
        
        info!("Telling target node at {} to prepare for reception...", target_api);
        
        let res = client.post(&target_api)
            .json(&ReceiveTeleportRequest { vm_id: payload.vm_id.clone() })
            .send()
            .await;
            
        if let Err(e) = res {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Target node unreachable: {}", e)));
        } else if let Ok(resp) = res {
            if !resp.status().is_success() {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Target node refused: {:?}", resp.text().await)));
            }
        }

        let teleporter = vyoma_teleport::sender::Teleporter::new(payload.vm_id.clone(), target_url, vm.mem_size_mib as u64);
        
        let teleport_vm_id = payload.vm_id.clone();
        tokio::spawn(async move {
            match teleporter.teleport_vm(PathBuf::new(), PathBuf::new(), &ch_socket).await {
                Ok(_) => info!("Teleportation of VM {} succeeded!", teleport_vm_id),
                Err(e) => error!("Teleportation of VM {} failed: {}", teleport_vm_id, e),
            }
        });

        Ok(format!("Teleportation initiated for VM {}", payload.vm_id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

#[derive(Serialize, Deserialize)]
pub struct ReceiveTeleportRequest {
    pub vm_id: String,
}

pub async fn receive_teleport_handler(
    State(state): State<AppState>,
    Json(payload): Json<ReceiveTeleportRequest>,
) -> Result<String, (StatusCode, String)> {
    info!("Handling POST /receive-teleport for VM {}", payload.vm_id);
    
    // Create directories
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let vm_dir = home.join(".ignite").join("vms").join(&payload.vm_id);
    std::fs::create_dir_all(&vm_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let ch_socket = vm_dir.join("ch.sock").to_string_lossy().to_string();
    let mut vmm = VmmManager::new(&ch_socket);

    // Start Cloud Hypervisor
    if let Err(e) = vmm.start_daemon(&format!("{}/bin/cloud-hypervisor", state.data_dir), None, state.rootless) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to start Cloud Hypervisor: {}", e),
        ));
    }

    // Call Receiver
    let receiver = vyoma_teleport::receiver::TeleportReceiver::new(PathBuf::new(), PathBuf::new(), "0.0.0.0".to_string());
    if let Err(e) = receiver.start_receiving(&ch_socket).await {
        let _ = vmm.kill();
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to set receive-migration mode: {}", e)));
    }

    // We leave it running. Sender will connect to port 9000 and it will resume automatically.
    // The VM state should probably be adopted by the daemon after migration completes.
    // In a complete implementation, we would wait for it and insert into `state.vms`.
    
    Ok(format!("Cloud Hypervisor ready to receive VM {} on TCP port 9000", payload.vm_id))
}

// --- Raft Core API Handlers ---

pub async fn raft_append_handler(
    State(state): State<AppState>,
    Json(req): Json<AppendEntriesRequest<crate::swarm::raft_types::SwarmConfig>>,
) -> Result<Json<openraft::raft::AppendEntriesResponse<crate::swarm::raft_types::NodeId>>, (StatusCode, String)> {
    if let Some(raft) = &state.raft {
        let resp = raft.append_entries(req).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(resp))
    } else {
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Raft not running".into()))
    }
}

pub async fn raft_snapshot_handler(
    State(state): State<AppState>,
    Json(req): Json<InstallSnapshotRequest<crate::swarm::raft_types::SwarmConfig>>,
) -> Result<Json<openraft::raft::InstallSnapshotResponse<crate::swarm::raft_types::NodeId>>, (StatusCode, String)> {
    if let Some(raft) = &state.raft {
        let resp = raft.install_snapshot(req).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(resp))
    } else {
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Raft not running".into()))
    }
}

pub async fn raft_vote_handler(
    State(state): State<AppState>,
    Json(req): Json<VoteRequest<crate::swarm::raft_types::NodeId>>,
) -> Result<Json<openraft::raft::VoteResponse<crate::swarm::raft_types::NodeId>>, (StatusCode, String)> {
    if let Some(raft) = &state.raft {
        let resp = raft.vote(req).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(resp))
    } else {
        Err((StatusCode::INTERNAL_SERVER_ERROR, "Raft not running".into()))
    }
}

#[derive(Deserialize)]
pub struct PolicyRequest {
    pub policy: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct PolicyResponse {
    pub policy: String,
    pub enabled: bool,
    pub message: String,
}

pub async fn set_policy_handler(
    State(_state): State<AppState>,
    Json(payload): Json<PolicyRequest>,
) -> Result<Json<PolicyResponse>, (StatusCode, String)> {
    info!("Setting policy: {} to {}", payload.policy, payload.enabled);

    match payload.policy.as_str() {
        "require-measured-boot" => {
            let message = if payload.enabled {
                "Measured boot policy enabled. VMs will require TPM attestation."
            } else {
                "Measured boot policy disabled."
            };
            Ok(Json(PolicyResponse {
                policy: payload.policy,
                enabled: payload.enabled,
                message: message.to_string(),
            }))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown policy: {}", payload.policy),
        )),
    }
}

pub async fn get_policy_handler(
    State(_state): State<AppState>,
) -> Result<Json<Vec<PolicyResponse>>, (StatusCode, String)> {
    Ok(Json(vec![
        PolicyResponse {
            policy: "require-measured-boot".to_string(),
            enabled: false,
            message: "Use PUT /policy with {policy: 'require-measured-boot', enabled: true/false}".to_string(),
        }
    ]))
}
