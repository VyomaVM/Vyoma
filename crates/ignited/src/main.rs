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
use tracing::{info, error, warn};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use ignite_core::vmm::VmmManager;
use ignite_core::api::{PortMapping, VolumeMount};
use ignite_core::proxy::ProxyManager;
use ignite_core::fs::VirtioFsManager;
use ignite_core::cgroups::CgroupManager;

mod dns;
use tokio::task::JoinHandle;
use std::process::Command;
use axum::body::Body;
use tokio_util::io::StreamReader;
use flate2::read::GzDecoder;
use tar::Archive;
use ignite_core::builder::{IgniteFile, Instruction};
// use futures::StreamExt as FuturesStreamExt;

#[derive(Clone)]
struct AppState {
    vms: Arc<StdMutex<HashMap<String, Arc<TokioMutex<VmInstance>>>>>,
    cgroups: Arc<CgroupManager>,
    cni_manager: Arc<ignite_core::cni::CniManager>,
    rootless: bool,
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
    #[allow(dead_code)]
    slirp: Option<ignite_core::slirp::SlirpManager>,
    cgroup_path: Option<String>,
    netns_path: Option<String>,
    
    // Metadata for Recovery
    config_ports: Vec<PortMapping>,
    config_volumes: Vec<VolumeMount>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VmState {
    id: String,
    tap_name: String,
    dm_name: String,
    loop_devices: Vec<String>,
    cow_file_path: String,
    ip_address: String,
    cgroup_path: Option<String>,
    netns_path: Option<String>,
    ports: Vec<PortMapping>,
    volumes: Vec<VolumeMount>,
}

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
        
        // 0. CNI Cleanup (If applicable)
        // We need the CniManager to call del(). VmInstance doesn't have it.
        // We can pass it in, or we can handle it in the caller.
        // VmInstance::cleanup currently takes &mut self only.
        // Refactoring to take cni_manager? No, let's keep it simple.
        // We'll require the caller to handle CNI DEL if they have the manager,
        // OR we just clean up the Namespace which implicitly kills the veth/interface?
        // CNI DEL is "polite" cleanup.
        //
        // NOTE: Since VmInstance struct doesn't have access to AppState or CniManager,
        // we can't call cni.del() here without changing the method signature.
        // Let's modify the signature.
        
        // 1. Kill VMM
        if let Err(e) = self.vmm.kill() {
            error!("Failed to kill VMM: {}", e);
        }

        // 2. Remove Network Interface / CNI
        if let Some(netns) = &self.netns_path {
             if let Err(e) = cni_manager.del(&self.id, netns, "eth0") {
                  error!("CNI DEL failed: {}", e);
             }
             // Remove netns file
             // "ip netns delete <name>"
             // Name is likely "vm-{id}" derived or we can just unmount/remove the path?
             // "ip netns delete" unmounts /var/run/netns/<name> and removes it.
             let netns_name = format!("vm-{}", self.id);
             let _ = Command::new("ip").args(&["netns", "delete", &netns_name]).output();
        }
        
        if !self.tap_name.is_empty() {
            if let Err(e) = NetworkManager::remove_interface(&self.tap_name) {
                 error!("Failed to remove TAP {}: {}", self.tap_name, e);
            }
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
    
    // CNI Initialization
    let home = dirs::home_dir().expect("No home dir");
    let cni_config_dir = home.join(".ignite").join("cni").join("net.d");
    let cni_plugin_dir = home.join(".ignite").join("cni").join("bin");
    
    std::fs::create_dir_all(&cni_config_dir).unwrap();
    std::fs::create_dir_all(&cni_plugin_dir).unwrap();
    
    // Create default bridge config if empty
    let bridge_conf = cni_config_dir.join("10-ignite-bridge.conf");
    if !bridge_conf.exists() {
        let conf = r#"{
    "cniVersion": "0.4.0",
    "name": "ignite-net",
    "type": "bridge",
    "bridge": "ign0",
    "isGateway": true,
    "ipMasq": true,
    "ipam": {
        "type": "host-local",
        "subnet": "172.16.0.0/24",
        "routes": [
            { "dst": "0.0.0.0/0" }
        ]
    }
}"#;
        std::fs::write(&bridge_conf, conf).unwrap();
        info!("Created default CNI configuration at {:?}", bridge_conf);
    }
    
    let cni_manager = Arc::new(ignite_core::cni::CniManager::new(cni_plugin_dir, cni_config_dir));

    let rootless = !ignite_core::rootless::RootlessManager::is_root();
    
    // Slirp Check
    if rootless {
        if let Err(e) = ignite_core::slirp::SlirpManager::check_available() {
             error!("Rootless Error: {}", e);
             std::process::exit(1);
        }
    }

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
        cgroups,
        cni_manager,
        rootless,
    };

    // Recovery Phase
    initialize_state(&state).await;
    
    // Start Process Monitor (Zombie Reaper)
    start_process_monitor(state.clone()).await;
    
    // Start DNS Server
    dns::start_dns_server(state.clone()).await;

    let shutdown_state = state.clone();

    // build our application with a route
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // VM Management
        .route("/run", post(run_vm))
        .route("/pull", post(pull_image_handler))
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
    


    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_state))
        .await
        .unwrap();
}

async fn shutdown_signal(state: AppState) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
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

    // Collect IDs first to avoid holding lock while cleaning up (cleanup takes time)
    let ids: Vec<String> = {
        let map = state.vms.lock().unwrap();
        map.keys().cloned().collect()
    };

    if ids.is_empty() {
        info!("No active VMs to clean up.");
    } else {
        info!("Cleaning up {} active VMs...", ids.len());
        for id in ids {
            let vm_arc = {
                let mut map = state.vms.lock().unwrap();
                map.remove(&id)
            };
            
            if let Some(vm_mutex) = vm_arc {
                info!("Stopping VM: {}", id);
                let mut vm = vm_mutex.lock().await;
                // VmInstance::cleanup handles the logic
                vm.cleanup(&state.cni_manager).await;
            }
        }
    }
    
    info!("Graceful shutdown complete. Bye!");
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

    // Helper to parse CNI IP
    fn parse_cni_ip(res: &serde_json::Value) -> anyhow::Result<String> {
        let ips = res.get("ips").and_then(|v| v.as_array()).ok_or(anyhow::anyhow!("No IPs in CNI result"))?;
        let first = ips.first().ok_or(anyhow::anyhow!("Empty IP list"))?;
        let addr = first.get("address").and_then(|v| v.as_str()).ok_or(anyhow::anyhow!("No address field"))?;
        // addr is CIDR (e.g. 172.16.0.5/24). Split it.
        let ip = addr.split('/').next().unwrap_or(addr);
        Ok(ip.to_string())
    }

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
    
    // Storage Setup
    let (dm_path, loop_devices, cow_file_str) = if state.rootless {
        // Rootless: Simple Copy (No DM/Loop)
        let vm_disk = vm_dir.join("disk.ext4");
        info!("Rootless: Copying base image to {:?}", vm_disk);
        std::fs::copy(&base_image_file, &vm_disk).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Rootless copy: {}", e)))?;
        
        // We use the disk file as the "dm_path" (Firecracker treats it as root block device)
        (vm_disk.to_string_lossy().to_string(), Vec::new(), vm_disk.to_string_lossy().to_string())
    } else {
        // Privileged: DM Snapshot (COW)
        let cow_file = vm_dir.join("diff.cow");
        let size_mb = 2048; 
        StorageManager::create_cow_file(&cow_file, size_mb).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        let base_loop = StorageManager::setup_loop_device(&base_image_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop base: {}", e)))?;
        let cow_loop = StorageManager::setup_loop_device(&cow_file).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Loop cow: {}", e)))?;
        
        let dm_name = format!("ign-{}", vm_id);
        let size_sectors = size_mb * 1024 * 1024 / 512;
        let dm_path = StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DM create: {}", e)))?;
        
        (dm_path, vec![base_loop, cow_loop], cow_file.to_string_lossy().to_string())
    };
    
    // Legacy bindings for compilation compatibility if needed
    let dm_name = if state.rootless { "rootless".to_string() } else { format!("ign-{}", vm_id) };
    
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
        std::fs::create_dir_all("/var/run/netns").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        // Create netns: ip netns add <name> (Requires root, which we have)
        let _ = Command::new("ip").args(&["netns", "add", &netns_name]).output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create netns: {}", e)))?;
            
        // 2. Call CNI ADD
        let ifname = "eth0";
        let cni_result = state.cni_manager.add(&vm_id, &netns_path, ifname)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("CNI ADD failed: {}", e)))?;
            
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
            let handle = ProxyManager::start_proxy(mapping.host_port, vm_ip.clone(), mapping.vm_port);
            proxy_tasks.push(handle);
        }
    } else {
        info!("Rootless: Skipping ProxyManager. Ports are mapped by slirp4netns.");
    }

    let boot_args = format!("console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw ip={}::{}:255.255.255.0:{}:eth0:off:{} init=/bin/sh", 
        vm_ip, "172.16.0.1", vm_id, "172.16.0.1");
    // "ip=<client-ip>:<server-ip>:<gw-ip>:<netmask>:<hostname>:<device>:<autoconf>:<dns0-ip>"
    // Correct kernel format: ip=172.16.0.X::172.16.0.1:255.255.255.0:myvm:eth0:off:172.16.0.1
    
    // 4. Firecracker VMM
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    let mut vmm = VmmManager::new(&socket_path);
    
    // Kernel path: bin/vmlinux (relative to CWD of daemon)
    let kernel_path = "bin/vmlinux";
    if !std::path::Path::new(kernel_path).exists() {
         return Err((StatusCode::INTERNAL_SERVER_ERROR, "Kernel binary (bin/vmlinux) not found".into()));
    }

    let mut slirp_mgr = None;

    if state.rootless {
        // Rootless Start
        info!("Spawning Firecracker in Rootless Mode (unshare -r -n)...");
        if let Err(e) = vmm.start_daemon("bin/firecracker", None, true) {
             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start FC (Rootless): {}", e)));
        }
        
        // Get PID for Slirp
        let pid = vmm.get_pid().ok_or((StatusCode::INTERNAL_SERVER_ERROR, "FC PID not found".into()))?;
        
        // Spawn Slirp
        let socket_path = vm_dir.join("slirp.sock");
        let mut slirp = ignite_core::slirp::SlirpManager::new(socket_path.to_string_lossy().as_ref());
        if let Err(e) = slirp.spawn(pid, "tap0", &payload.ports) {
             let _ = vmm.kill();
             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start slirp: {}", e)));
        }
        slirp_mgr = Some(slirp);
        
        // Config FC
        if let Err(e) = vmm.set_boot_source(kernel_path, &boot_args).await {
            let _ = vmm.kill();
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Boot source: {}", e)));
        }
        
        if let Err(e) = vmm.add_drive("rootfs", &dm_path, true).await {
             let _ = vmm.kill();
             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add drive: {}", e)));
        }
        
        // Network: Interface inside NetNS is "tap0" created by Slirp
        if let Err(e) = vmm.add_network_interface("eth0", "tap0", None).await {
             let _ = vmm.kill();
             return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Add net (rootless): {}", e)));
        }
        
    } else {
        // Root/CNI Start (Hybrid Mode)
        if let Err(e) = vmm.start_daemon("bin/firecracker", None, false) {
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
        tap_name: if state.rootless { String::new() } else { tap_name.clone() },
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
    };
    
    instance.save_state().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save state: {}", e)))?;

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
        vm.cleanup(&state.cni_manager).await;
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
    #[allow(dead_code)]
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
    
    if let Err(e) = vmm.start_daemon("bin/firecracker", None, state.rootless) {
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

#[derive(Serialize)]
struct PullResponse {
    status: String,
    path: String,
}

#[derive(Deserialize)]
struct PullRequest {
    image: String,
}

async fn pull_image_handler(
    Json(payload): Json<PullRequest>,
) ->  Result<Json<PullResponse>, (StatusCode, String)> {
    info!("Handling Pull request for {}", payload.image);
    
    let path = ensure_image_locally(&payload.image).await?;
    
    Ok(Json(PullResponse {
        status: "Image pulled successfully".to_string(),
        path: path.to_string_lossy().to_string(),
    }))
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

async fn initialize_state(state: &AppState) {
    info!("Recovery: Scanning for existing VMs...");
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return,
    };
    let vms_dir = home.join(".ignite").join("vms");
    if !vms_dir.exists() { return; }

    let entries = match std::fs::read_dir(vms_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if !entry.path().is_dir() { continue; }
        
        // Check for state.json
        let state_path = entry.path().join("state.json");
        if !state_path.exists() { continue; } // Not a managed VM or incomplete
        
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
                 let t = ProxyManager::start_proxy(p.host_port, vm_state.ip_address.clone(), p.vm_port);
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
             };
             
             state.vms.lock().unwrap().insert(vm_state.id, Arc::new(TokioMutex::new(instance)));
             
        } else {
             info!("Recovery: VM {} found but DEAD. cleaning up artifacts...", vm_state.id);
             // Cleanup dead VM
             let mut instance = VmInstance {
                 vmm,
                 id: vm_state.id.clone(),
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

async fn start_process_monitor(state: AppState) {
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
                            let mut msg = format!("Monitor: VM {} Firecracker process EXITED (Reaped): {}", id, status);
                            
                            // Check for OOM
                            if let Ok(count) = state.cgroups.get_oom_kill_count(&id) {
                                if count > 0 {
                                    msg.push_str(&format!(" [WARNING: OOM Kill Detected: {}]", count));
                                }
                            }
                            error!("{}", msg);
                        },
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
                                 },
                                 Ok(None) => None,
                             }
                        };
                        
                        if let Some(status) = exit_status {
                             warn!("Monitor: VM {} VirtioFS (vol{}) EXITED with {}. Restarting...", id, idx, status);
                             
                             // Get Config (Immutable borrow OK now)
                             let host_path = vm.config_volumes.get(idx).map(|v| v.host_path.clone());
                             
                             if let Some(path) = host_path {
                                  let fs = &mut vm.fs_managers[idx]; // Re-borrow mutably
                                  if let Err(e) = fs.start(&path) {
                                       error!("Monitor: FAILED to auto-restart VirtioFS for VM {}: {}", id, e);
                                  } else {
                                       info!("Monitor: RESTARTED VirtioFS for VM {} (vol{})", id, idx);
                                  }
                             } else {
                                  error!("Monitor: Cannot restart VirtioFS (vol{}), config not found.", idx);
                             }
                        }
                    }
                }
            }
        }
    });
}
