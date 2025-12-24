use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use tracing::{info, error};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use ignite_core::vmm::VmmManager;

#[derive(Clone)]
struct AppState {
    // Map<VmId, Arc<TokioMutex<VmInstance>>>
    vms: Arc<StdMutex<HashMap<String, Arc<TokioMutex<VmInstance>>>>>,
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
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("ignited (Ignite Daemon) starting up...");

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
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
    // Strategy: 
    // - Check if image dir exists (simple caching).
    // - If not, pull and unpack.
    // - Create base.ext4.
    
    // Sanitize image name for path (replace / and : with _)
    let safe_image_name = payload.image.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");
    
    if !base_image_file.exists() {
        info!("Image {} not found locally. Pulling...", payload.image);
        std::fs::create_dir_all(&image_store_path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        let mut oci = OciManager::new();
        let manifest_json = oci.pull_manifest(&payload.image).await.map_err(|e| (StatusCode::BAD_REQUEST, format!("Pull failed: {}", e)))?;
        
        let layers = oci.parse_layers(&manifest_json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        let temp_unpack_dir = tempfile::tempdir().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        for digest in layers {
             let layer_data = oci.pull_layer(&payload.image, &digest).await.map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed layer {}: {}", digest, e)))?;
             LayerManager::unpack_layer(&layer_data, temp_unpack_dir.path()).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Unpack failed: {}", e)))?;
        }
        
        // Create base Ext4
        // Default 2GB for base
        let size_mb = 2048;
        StorageManager::create_empty_file(&base_image_file, size_mb).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::format_ext4(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        StorageManager::populate_image(&base_image_file, temp_unpack_dir.path()).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Populate failed: {}", e)))?;
    } else {
        info!("Image found locally at {:?}", base_image_file);
    }

    // 3. VM Specific Setup
    let vm_id = uuid::Uuid::new_v4().to_string();
    let vm_dir = vms_root.join(&vm_id);
    std::fs::create_dir_all(&vm_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
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
    let boot_args = format!("console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw ip={}::{}:255.255.255.0::eth0:off", vm_ip, "172.16.0.1");
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
        
        vm.vmm.create_snapshot(
            snapshot_path.to_string_lossy().as_ref(), 
            mem_path.to_string_lossy().as_ref()
        ).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        Ok(format!("Snapshot created for VM {}", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

#[derive(Deserialize)]
struct RestoreRequest {
    snapshot_path: String,
    mem_path: String,
    // For MVP, we presume the disk state (COW) is already in a known location or passed here.
    // Ideally, "Teleportation" means standardizing the bundle format.
    // For now, let's assume we are "Teleporting" to the SAME machine but a new VM ID for verification.
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
    let size_mb = 2048;
    StorageManager::create_cow_file(&cow_file, size_mb).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // 4. Setup Storage Stack
    let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop base: {}", e)))?;
    let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop cow: {}", e)))?;
    
    let dm_name = format!("ign-{}", vm_id);
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
    let instance = VmInstance {
        vmm,
        id: vm_id.clone(),
        tap_name: tap_name.clone(),
        dm_name: dm_name.clone(),
        loop_devices: vec![base_loop, cow_loop],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        ip_address: vm_ip.clone(),
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
