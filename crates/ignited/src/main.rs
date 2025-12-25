use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::{State, Path},
    http::StatusCode,
    response::sse::{Event, Sse},
};
use tokio_stream::wrappers::BroadcastStream;
// use tokio_stream::StreamExt;
use futures::stream::Stream;
use std::convert::Infallible;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use tracing::{info, error};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use ignite_core::vmm::VmmManager;
use ignite_core::api::{PortMapping, VolumeMount};
use ignite_core::proxy::ProxyManager;
use ignite_core::fs::VirtioFsManager;
use ignite_core::cgroups::CgroupManager;
use tokio::task::JoinHandle;
use std::process::Command;
use axum::body::Body;
use axum::body::Bytes;
use tokio_util::io::StreamReader;
use tokio::io::AsyncReadExt;
use flate2::read::GzDecoder;
use tar::Archive;
use ignite_core::builder::{IgniteFile, Instruction};
// use futures::StreamExt as FuturesStreamExt;
use tokio::fs;

#[derive(Clone)]
struct AppState {
    // Map<VmId, Arc<TokioMutex<VmInstance>>>
    vms: Arc<StdMutex<HashMap<String, Arc<TokioMutex<VmInstance>>>>>,
    cgroups: Arc<CgroupManager>,
}

#[derive(Debug)]
struct VmInstance {
    vmm: VmmManager,
    // Resources to clean up
    id: String,
    tap_name: String,
    dm_name: String,
    loop_devices: Vec<String>,
    cow_file_path: String,
    ip_address: String,
    proxy_tasks: Vec<JoinHandle<()>>,

    fs_managers: Vec<VirtioFsManager>,
    cgroup_path: Option<String>,
}

impl VmInstance {
    pub async fn cleanup(&mut self) {
        info!("Cleaning up VM resources for {}", self.id);
        
        // 1. Kill VMM
        if let Err(e) = self.vmm.kill() {
            error!("Failed to kill VMM: {}", e);
        }

        // 2. Remove Network Interface
        if let Err(e) = NetworkManager::remove_interface(&self.tap_name) {
             error!("Failed to remove TAP {}: {}", self.tap_name, e);
        }
        
        // 3. Remove DM Device
        // This might fail if VMM is still holding it open, but kill() should have released it.
        // Sometimes a small delay helps or retry logic in remove_dm_device
        if let Err(e) = StorageManager::remove_dm_device(&self.dm_name) {
             error!("Failed to remove DM {}: {}", self.dm_name, e);
        }

        // 4. Detach Loop Devices
        for dev in &self.loop_devices {
            if let Err(e) = StorageManager::detach_loop_device(dev) {
                error!("Failed to detach loop {}: {}", dev, e);
            }
        }
        
        // 5. Remove COW file (optional, maybe keep for persistence?)
        // For now, let's keep it to allow debugging, or maybe delete it. 
        // Let's delete it for "ephemeral" feeling.
        if std::path::Path::new(&self.cow_file_path).exists() {
             let _ = std::fs::remove_file(&self.cow_file_path);
        }
        
        // 6. Abort Proxy Tasks
        for task in &self.proxy_tasks {
            task.abort();
        }

        // 7. Remove Cgroup
        if let Some(path) = &self.cgroup_path {
             let cm = CgroupManager::new(); // Re-instantiate or assume we can just use path
             // Ideally we shouldn't re-instantiate, but for cleanup it's stateless enough.
             // Actually, remove_vm_cgroup just takes ID if we pass it, or we use path.
             // Our CgroupManager takes ID.
             if let Err(e) = cm.remove_vm_cgroup(&self.id) {
                 error!("Failed to remove cgroup for {}: {}", self.id, e);
             }
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    // Rootless Checks
    if ignite_core::rootless::RootlessManager::is_root() {
        info!("Running as ROOT (Privileged Mode)");
    } else {
        info!("Running as USER (Rootless Mode)");
        // Verify KVM Access
        if let Err(e) = ignite_core::rootless::RootlessManager::check_kvm_permissions() {
             error!("Rootless Mode Error: {}", e);
             error!("Please add your user to the 'kvm' group: sudo usermod -aG kvm $USER");
             std::process::exit(1);
        }
    }

    info!("ignited (Ignite Daemon) starting up...");

    let cgroups = CgroupManager::new();
    if let Err(e) = cgroups.init() {
        error!("Failed to init cgroups: {}", e);
        // We continue? Or fail? Fail is safer.
        // return;
    }
    let cgroups = Arc::new(cgroups);

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
        cgroups,
    };

    // build our application with a route
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // VM Management
        .route("/run", post(run_vm))
        .route("/stop/:id", post(stop_vm))
        .route("/pause/:id", post(pause_vm))
        .route("/resume/:id", post(resume_vm))
        .route("/ps", get(list_vms))
        .route("/snapshot/:id", post(snapshot_vm))
        .route("/restore", post(restore_vm))
        .route("/logs/:id", get(stream_logs))
        .route("/build", post(build_image))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    info!("Daemon listening on {}", listener.local_addr().unwrap());
    
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
struct RunRequest {
    image: String,
    #[serde(default = "default_vcpu")]
    vcpu: u32,
    #[serde(default = "default_mem")]
    mem_size_mib: u32,
    #[serde(default)]
    ports: Vec<PortMapping>,
    #[serde(default)]
    volumes: Vec<VolumeMount>,
}

fn default_vcpu() -> u32 { 1 }
fn default_mem() -> u32 { 512 }

#[derive(Serialize)]
struct RunResponse {
    vm_id: String,
    status: String,
    ip_address: String,
}

use ignite_core::oci::OciManager;
use ignite_core::layers::LayerManager;
use ignite_core::storage::StorageManager;
use ignite_core::network::NetworkManager;

async fn run_vm(
    State(state): State<AppState>,
    Json(payload): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!("Received request to run image: {}", payload.image);
    
    // 1. Setup Directories
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let ignite_root = home.join(".ignite");
    let images_root = ignite_root.join("images");
    let vms_root = ignite_root.join("vms");
    
    std::fs::create_dir_all(&images_root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    std::fs::create_dir_all(&vms_root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // 2. Pull Image & Prepare Base
    let base_image_file = ensure_image_locally(&payload.image).await?;
    info!("Using base image at {:?}", base_image_file);

    // 3. VM Specific Setup
    let vm_id = uuid::Uuid::new_v4().to_string();
    let vm_dir = vms_root.join(&vm_id);
    std::fs::create_dir_all(&vm_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // Initialize Git Repo for Time Travel
    if let Err(e) = git_init(&vm_dir) {
        error!("Failed to init git repo in {:?}: {}", vm_dir, e);
    }
    
    // COW File
    let cow_file = vm_dir.join("diff.cow");
    let size_mb = 2048; // Same as base for simplicity, or larger?
    StorageManager::create_cow_file(&cow_file, size_mb).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // Loop Devices
    let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop base: {}", e)))?;
    let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop cow: {}", e)))?;
    
    // DM Snapshot
    let dm_name = format!("ign-{}", vm_id);
    // Size in sectors: 2048MB * 1024 * 1024 / 512 = 4,194,304
    let size_sectors = size_mb * 1024 * 1024 / 512;
    let dm_path = StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DM create: {}", e)))?;
    
    // Network
    // Ensure bridge exists
    let bridge_name = "ign0";
    let bridge_cidr = "172.16.0.1/24";
    NetworkManager::setup_bridge(bridge_name, bridge_cidr).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Bridge setup: {}", e)))?;
    NetworkManager::setup_nat("172.16.0.0/24").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("NAT setup: {}", e)))?;
    
    let tap_name = format!("tap{}", &vm_id[0..8]); // shorten for ifname limit
    NetworkManager::setup_tap(&tap_name, bridge_name).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("TAP setup: {}", e)))?;
    
    // Determine VM IP (Hack: random in subnet or increment. For MVP, we need to manage this state.)
    // TODO: Implement proper IPAM.
    // Hack: Generate random byte for last octet unique-ish.
    // This is race-prone but "good enough" for single user manual test.
    let random_octet = rand::random::<u8>(); 
    // Ensure it's not 0, 1 (gateway), or 255
    let safe_octet = std::cmp::max(2, std::cmp::min(254, random_octet));
    let vm_ip = format!("172.16.0.{}", safe_octet);

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
    for mapping in &payload.ports {
        let handle = ProxyManager::start_proxy(mapping.host_port, vm_ip.clone(), mapping.vm_port);
        proxy_tasks.push(handle);
    }

    let boot_args = format!("console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw ip={}::{}:255.255.255.0::eth0:off init=/bin/sh", vm_ip, "172.16.0.1");
    // "ip=<client-ip>:<server-ip>:<gw-ip>:<netmask>:<hostname>:<device>:<autoconf>"
    // Correct kernel format: ip=172.16.0.X::172.16.0.1:255.255.255.0::eth0:off
    
    // 4. Firecracker VMM
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    let mut vmm = VmmManager::new(&socket_path);
    
    // Kernel path: bin/vmlinux (relative to CWD of daemon)
    let kernel_path = "bin/vmlinux";
    if !std::path::Path::new(kernel_path).exists() {
         return Err((StatusCode::INTERNAL_SERVER_ERROR, "Kernel binary (bin/vmlinux) not found".into()));
    }

    if let Err(e) = vmm.start_daemon("bin/firecracker") {
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start FC: {}", e)));
    }
    
    // Config
    if let Err(e) = vmm.set_boot_source(kernel_path, &boot_args).await {
        let _ = vmm.kill();
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Boot source: {}", e)));
    }
    
    if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
         let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add drive: {}", e)));
    }
    
    // Add network
    // guest MAC: random
    if let Err(e) = vmm.add_network_interface("eth0", &tap_name, None).await {
         let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add net: {}", e)));
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
             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start virtiofsd for {}: {}", vol.host_path, e)));
        }
        
        // Add to FC
        if let Err(e) = vmm.add_file_system(&tag, socket_path.to_string_lossy().as_ref(), &tag).await {
              let _ = vmm.kill();
              let _ = fs_mgr.kill(); 
              return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add fs: {}", e)));
        }
        
        fs_managers.push(fs_mgr);
    }
    
    if let Err(e) = vmm.set_machine_config(payload.vcpu, payload.mem_size_mib).await {
          let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Machine config: {}", e)));
    }
    
    if let Err(e) = vmm.start_instance().await {
         let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Start instance: {}", e)));
    }
    
    // Success - Store State

    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        tap_name: tap_name.clone(),
        dm_name: dm_name.clone(),
        loop_devices: vec![base_loop, cow_loop],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        ip_address: vm_ip.clone(),
        proxy_tasks,
        fs_managers,
        cgroup_path,
    };
    
    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(instance)));
    }

    Ok(Json(RunResponse {
        vm_id,
        status: "Running".to_string(),
        ip_address: vm_ip,
    }))
}

async fn stop_vm(
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
        vm.cleanup().await;
        Ok(format!("VM {} stopped and cleaned up", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

async fn pause_vm(
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
        vm.vmm.pause_instance().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} paused", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

async fn resume_vm(
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
        vm.vmm.resume_instance().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} resumed", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

async fn snapshot_vm(
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
        let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
        let vm_dir = home.join(".ignite").join("vms").join(&id);
        let snapshot_path = vm_dir.join("snapshot.snap");
        let mem_path = vm_dir.join("memory.mem");
        
        // 1. Pause VM (Required for snapshot)
        vm.vmm.pause_instance().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to pause: {}", e)))?;
        
        // 2. Create Snapshot
        let snap_result = vm.vmm.create_snapshot(
            snapshot_path.to_string_lossy().as_ref(), 
            mem_path.to_string_lossy().as_ref()
        ).await;
        
        // 3. Resume VM (Always resume, even if snapshot failed)
        if let Err(e) = vm.vmm.resume_instance().await {
            error!("Failed to resume VM {} after snapshot: {}", id, e);
        }
        
        snap_result.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Snapshot failed: {}", e)))?;
        
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
struct RestoreRequest {
    snapshot_path: String,
    mem_path: String,
    cow_path: String, // Path to the existing COW file to restore from
    // For MVP, we presume the disk state (COW) is already in a known location or passed here.
    original_vm_id: String, // To find the original disk resources
}

async fn restore_vm(
    State(state): State<AppState>,
    Json(payload): Json<RestoreRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
     info!("Request to restore VM from snapshot: {}", payload.snapshot_path);
     
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
    std::fs::create_dir_all(&vm_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // 3. New COW File
    let cow_file = vm_dir.join("diff.cow");
    
    // Copy existing COW state directly
    info!("Restoring disk state from {}", payload.cow_path);
    std::fs::copy(&payload.cow_path, &cow_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to copy COW file: {}", e)))?;
    
    // We don't create empty cow file anymore
    // StorageManager::create_cow_file(&cow_file, size_mb)...
    
    // 4. Setup Storage Stack
    let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop base: {}", e)))?;
    let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop cow: {}", e)))?;
    
    let dm_name = format!("ign-{}", vm_id);
    let size_mb = 2048; // Must match original
    let size_sectors = size_mb * 1024 * 1024 / 512;
    let dm_path = StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DM create: {}", e)))?;
    
    // 5. Setup Network
    // Ensure bridge exists (idempotent-ish)
    let bridge_name = "ign0";
    let bridge_cidr = "172.16.0.1/24";
    // We skip bridge setup here assuming it's up from previous run, or we should just call it safe.
    // NetworkManager::setup_bridge(bridge_name, bridge_cidr)... 
    
    let tap_name = format!("tap{}", &vm_id[0..8]);
    NetworkManager::setup_tap(&tap_name, bridge_name).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("TAP setup: {}", e)))?;
    
    let random_octet = rand::random::<u8>(); 
    let safe_octet = std::cmp::max(2, std::cmp::min(254, random_octet));
    let vm_ip = format!("172.16.0.{}", safe_octet);
    
    // 6. Firecracker VMM
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    let mut vmm = VmmManager::new(&socket_path);
    
    if let Err(e) = vmm.start_daemon("bin/firecracker") {
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start FC: {}", e)));
    }
    
    // 7. Load Snapshot INSTEAD of Boot Source
    if let Err(e) = vmm.load_snapshot(&payload.snapshot_path, &payload.mem_path).await {
        let _ = vmm.kill();
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Load snapshot: {}", e)));
    }
    
    // 8. Add Drives (Must match configuration of snapped VM usually, but we are attaching NEW cow)
    // Firecracker snapshot restoration often involves re-attaching block devices.
    // The device ID "rootfs" must match.
    if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
         let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add drive: {}", e)));
    }
     
    // 9. Add Network
    if let Err(e) = vmm.add_network_interface("eth0", &tap_name, None).await {
         let _ = vmm.kill();
         return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add net: {}", e)));
    }
    
    // 10. Resume
    if let Err(e) = vmm.resume_instance().await {
          let _ = vmm.kill();
          return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Resume instance: {}", e)));
    }

    // Success - Store State
    let proxy_tasks = Vec::new(); // No ports restored for now
    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        tap_name: tap_name.clone(),
        dm_name: dm_name.clone(),
        loop_devices: vec![base_loop, cow_loop],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        ip_address: vm_ip.clone(),
        proxy_tasks,

        fs_managers: Vec::new(),
        cgroup_path: None, // Restored VMs need cgroups too? Yes. 
        // For MVP, simplistic restore skips cgroup enforcement or needs logic duplication.
        // TODO: Isolate create_resources logic.
    };
    
    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(instance)));
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
}

#[derive(Serialize)]
struct ListResponse {
    vms: Vec<VmSummary>,
}

async fn list_vms(State(state): State<AppState>) -> Json<ListResponse> {
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
        });
    }
    
    Json(ListResponse { vms: summaries })
}

// Git Helper Functions
fn git_init(path: &std::path::Path) -> std::io::Result<()> {
    Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()?;
        
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

async fn stream_logs(
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
        let stream = BroadcastStream::new(rx)
            .filter_map(|try_msg| {
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




async fn build_image(
    State(_state): State<AppState>,
    body: Body,
) -> Result<String, (StatusCode, String)> {
    info!("Received build request");

    // 1. Stream body to a temp file (tar.gz)
    let temp_dir = tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tar_path = temp_dir.path().join("context.tar.gz");
    
    {
        // Convert Body stream to AsyncRead
        use futures::StreamExt;
        let stream = body.into_data_stream().map(|b| b.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
        let mut reader = StreamReader::new(stream);
        let mut file = tokio::fs::File::create(&tar_path).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tokio::io::copy(&mut reader, &mut file).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // 2. Unpack
    let tar_file = std::fs::File::open(&tar_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let decoder = GzDecoder::new(tar_file);
    let mut archive = Archive::new(decoder);
    let context_dir = temp_dir.path().join("context");
    std::fs::create_dir(&context_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    archive.unpack(&context_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Unpack failed: {}", e)))?;

    // 3. Parse Ignitefile
    let ignitefile_path = context_dir.join("Ignitefile");
    if !ignitefile_path.exists() {
        return Err((StatusCode::BAD_REQUEST, "Ignitefile not found in build context".to_string()));
    }
    
    let ignite_file = IgniteFile::parse(&ignitefile_path).map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse error: {}", e)))?;
    
    // 4. Build Process
    // We need to track the "current image base".
    // 1. FROM -> Pull image, setup as current base.
    //    We need to work on a COPY of the base, not the shared base.
    //    So we create a new "build artifact" (a raw ext4 file).
    
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let build_root = home.join(".ignite").join("builds");
    std::fs::create_dir_all(&build_root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let build_id = uuid::Uuid::new_v4().to_string();
    let current_image_path = build_root.join(format!("{}.ext4", build_id));
    
    // Track if we have a base
    let mut has_base = false;
    
    for instr in ignite_file.instructions {
        match instr {
            Instruction::From(image) => {
                info!("Building FROM {}", image);
                
                let base_cache = ensure_image_locally(&image).await?;
                
                // Copy base cache to current_image_path
                std::fs::copy(&base_cache, &current_image_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to copy base: {}", e)))?;
                has_base = true;
            }
            Instruction::Run(cmd) => {
                if !has_base { return Err((StatusCode::BAD_REQUEST, "RUN before FROM".to_string())); }
                info!("RUN: {}", cmd);
                
                // Mount image
                let loop_device = StorageManager::setup_loop_device(&current_image_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                // Mount the loop device to a temp dir
                let mount_point = tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_status = Command::new("sudo")
                    .args(&["mount", &loop_device, mount_point.path().to_str().unwrap()])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    
                if !mount_status.success() {
                     let _ = StorageManager::detach_loop_device(&loop_device);
                     return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to mount for RUN".to_string()));
                }
                
                // Prepare resolv.conf for networking in chroot
                // COPY host resolv.conf to chroot
                let _ = std::fs::copy("/etc/resolv.conf", mount_point.path().join("etc/resolv.conf"));
                
                // Execute chroot
                // cmd string might need splitting or sh -c
                let chroot_status = Command::new("sudo")
                    .args(&["chroot", mount_point.path().to_str().unwrap(), "/bin/sh", "-c", &cmd])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                // Unmount
                 let umount_status = Command::new("sudo")
                    .args(&["umount", mount_point.path().to_str().unwrap()])
                    .status()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    
                 let _ = StorageManager::detach_loop_device(&loop_device);
                 
                 if !chroot_status.success() {
                     return Err((StatusCode::BAD_REQUEST, format!("RUN command failed: {}", cmd)));
                 }
            }
            Instruction::Copy { src, dest } => {
                if !has_base { return Err((StatusCode::BAD_REQUEST, "COPY before FROM".to_string())); }
                 info!("COPY {} -> {}", src, dest);
                 
                // Mount
                let loop_device = StorageManager::setup_loop_device(&current_image_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_point = tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let mount_status = Command::new("sudo")
                    .args(&["mount", &loop_device, mount_point.path().to_str().unwrap()])
                    .status()
                     .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                
                 if !mount_status.success() {
                     let _ = StorageManager::detach_loop_device(&loop_device);
                     return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to mount for COPY".to_string()));
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
                          let _ = Command::new("sudo").args(&["umount", mount_point.path().to_str().unwrap()]).status();
                           let _ = StorageManager::detach_loop_device(&loop_device);
                         return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to copy dir".to_string()));
                    }
                } else {
                     match std::fs::copy(&src_path, &dest_path) {
                         Ok(_) => {},
                         Err(e) => {
                             // clean up
                             let _ = Command::new("sudo").args(&["umount", mount_point.path().to_str().unwrap()]).status();
                             let _ = StorageManager::detach_loop_device(&loop_device);
                             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed copy file: {}", e)));
                         }
                     }
                }

                 // Unmount
                 let _ = Command::new("sudo").args(&["umount", mount_point.path().to_str().unwrap()]).status();
                 let _ = StorageManager::detach_loop_device(&loop_device);
            }
        }
    }
    
    // Done. The `current_image_path` is the result.
    // Move it to images dir? Or return ID?
    // For now return path.
    Ok(format!("Build successful. Image at: {:?}", current_image_path))
}


async fn ensure_image_locally(image_name: &str) -> Result<std::path::PathBuf, (StatusCode, String)> {
    let home = dirs::home_dir().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;
    let images_root = home.join(".ignite").join("images");
    std::fs::create_dir_all(&images_root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let safe_image_name = image_name.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");

    if !base_image_file.exists() {
        info!("Image {} not found locally. Pulling...", image_name);
        std::fs::create_dir_all(&image_store_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut oci = OciManager::new();
        let manifest_json = oci.pull_manifest(image_name).await.map_err(|e| (StatusCode::BAD_REQUEST, format!("Pull failed: {}", e)))?;

        let layers = oci.parse_layers(&manifest_json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let temp_unpack_dir = tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        for digest in layers {
             let layer_data = oci.pull_layer(image_name, &digest).await.map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed layer {}: {}", digest, e)))?;
             LayerManager::unpack_layer(&layer_data, temp_unpack_dir.path()).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Unpack failed: {}", e)))?;
        }

        let size_mb = 2048;
        StorageManager::create_empty_file(&base_image_file, size_mb).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::format_ext4(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::populate_image(&base_image_file, temp_unpack_dir.path()).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Populate failed: {}", e)))?;
    } else {
        info!("Image found locally at {:?}", base_image_file);
    }
    
    Ok(base_image_file)
}
