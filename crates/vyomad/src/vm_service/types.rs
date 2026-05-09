use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct VmRunRequest {
    pub image: String,
    pub vcpu: u32,
    pub mem_size_mib: u32,
    pub ports: Vec<vyoma_core::api::PortMapping>,
    pub volumes: Vec<vyoma_core::api::VolumeMount>,
    pub hostname: Option<String>,
    pub networks: Vec<String>,
    pub labels: HashMap<String, String>,
    pub base_image_path: String,
}

impl From<crate::api::handlers::RunRequest> for VmRunRequest {
    fn from(req: crate::api::handlers::RunRequest) -> Self {
        Self {
            image: req.image.clone(),
            vcpu: req.vcpu,
            mem_size_mib: req.mem_size_mib,
            ports: req.ports.clone(),
            volumes: req.volumes.clone(),
            hostname: req.hostname.clone(),
            networks: req.networks.clone(),
            labels: req.labels.clone(),
            base_image_path: req.base_image_path.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmRunResponse {
    pub vm_id: String,
    pub status: String,
    pub ip_address: String,
}

#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub path: PathBuf,
    pub config: vyoma_core::oci::OciImageConfig,
}

#[derive(Debug, Clone)]
pub struct PreparedStorage {
    pub dm_device_path: String,
    pub loop_devices: Vec<String>,
    pub cow_file_path: String,
    pub dm_name: String,
}

#[derive(Debug, Clone)]
pub struct VmNetworkConfig {
    pub ip_address: String,
    pub primary_tap: String,
    pub gateway: String,
    pub network_infos: Vec<NetworkInfo>,
    pub netns_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub ip: String,
    pub tap_name: String,
    pub gateway: Option<String>,
    pub interface_name: String,
    pub network_name: String,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub initramfs_path: Option<PathBuf>,
    pub init_script_path: PathBuf,
    pub cmd: Vec<String>,
    pub workdir: String,
    pub envs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChConfig {
    pub kernel_path: String,
    pub ch_path: String,
    pub socket_path: String,
    pub boot_args: String,
    pub rootfs_path: String,
    pub vsock_cid: u32,
    pub vsock_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PolicyResult {
    pub passed: bool,
    pub attestation_pending: bool,
}

pub struct VmService<P = ()> {
    state: crate::state::AppState,
    _phantom: std::marker::PhantomData<P>,
}

impl VmService {
    pub fn new(state: crate::state::AppState) -> Self {
        Self {
            state,
            _phantom: std::marker::PhantomData,
        }
    }
}