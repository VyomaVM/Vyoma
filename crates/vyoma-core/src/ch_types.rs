use serde::{Deserialize, Serialize};

/// Minimal representation of Cloud Hypervisor API v1 `VmConfig`.
/// Mirrors the JSON structure expected by `/api/v1/vm.create`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<CpusConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<PayloadConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disks: Option<Vec<DiskConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Vec<NetConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs: Option<Vec<FsConfig>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<ConsoleConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub console: Option<ConsoleConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsock: Option<VsockConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<FirmwareConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm: Option<TpmConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sev_snp: Option<SevSnpConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tdx: Option<TdxConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VsockConfig {
    pub cid: u32,
    pub socket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CpusConfig {
    pub boot_vcpus: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_vcpus: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    pub size: u64, // In bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hugepages: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PayloadConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmdline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initramfs: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskConfig {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vhost_user: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>, // format: "ip_addr/mask"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vhost_user: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FsConfig {
    pub tag: String,
    pub socket: String, // Path to virtiofsd vhost-user socket
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_queues: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_size: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsoleConfig {
    pub mode: String, // "Off", "Null", "File", "Tty", "Pty"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshotConfig {
    pub destination_url: String, // "file:///path/to/snapshot"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmRestoreConfig {
    pub source_url: String, // "file:///path/to/snapshot"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiveMigrationData {
    pub receiver_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMigrationData {
    pub destination_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FirmwareConfig {
    pub firmware_path: String,
    #[serde(default)]
    pub secure_boot: bool,
    #[serde(default)]
    pub uefi_vars: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TpmConfig {
    pub socket_path: String,
    pub tpm_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SevSnpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub certificate_path: Option<String>,
    #[serde(default)]
    pub guest_key_root_hash: Option<String>,
    #[serde(default)]
    pub host_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TdxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub measurement_uuid: Option<String>,
}
