//! Pure-Rust, cross-platform HTTP API request and response DTOs shared
//! between the `vyomad` daemon and `vyoma` clients.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors returned while constructing or parsing Vyoma API payloads.
#[derive(Debug, Error)]
pub enum ApiError {
    /// Failed to serialize or deserialize an API JSON payload.
    #[error("API JSON codec error: {0}")]
    Json(#[from] serde_json::Error),

    /// Daemon returned an HTTP error status with an optional message body.
    #[error("Daemon API error ({status}): {message}")]
    HttpStatus { status: u16, message: String },
}

/// Host-to-VM TCP/UDP port mapping specification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortMapping {
    pub host_port: u16,
    pub vm_port: u16,
}

/// Host directory to guest VM mount point mapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VolumeMount {
    pub host_path: String,
    pub vm_path: String,
    pub read_only: bool,
}

/// Live CPU and memory usage metrics for a running VM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmStatsResponse {
    pub vm_id: String,
    pub cpu_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_limit_bytes: u64,
    pub memory_percent: f64,
}

fn default_vcpu() -> u32 {
    1
}

fn default_mem() -> u32 {
    512
}

/// Request payload to create and boot a new micro-VM (`POST /run`).
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Default for RunRequest {
    fn default() -> Self {
        Self {
            image: String::new(),
            vcpu: default_vcpu(),
            mem_size_mib: default_mem(),
            ports: Vec::new(),
            volumes: Vec::new(),
            hostname: None,
            networks: Vec::new(),
            labels: HashMap::new(),
            base_image_path: String::new(),
        }
    }
}

/// Response returned after starting or restoring a VM.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunResponse {
    pub vm_id: String,
    pub status: String,
    #[serde(default)]
    pub ip_address: String,
}

/// Request payload to execute a command inside a VM (`POST /vms/:id/exec`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command: Vec<String>,
}

/// Output captured from executing a command inside a VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Request payload to commit a VM's storage overlay to a reusable image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRequest {
    pub new_image_name: String,
}

/// Query parameters for creating a named snapshot (`POST /vms/:id/snapshot`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotQuery {
    pub name: Option<String>,
}

/// Request payload to branch (clone CoW overlay from) a VM snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRequest {
    pub from: String,
    pub new_name: String,
}

/// Response returned when creating a VM branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResponse {
    pub vm_id: String,
    pub name: String,
    pub parent_vm_id: String,
    pub snapshot_id: String,
    pub status: String,
    pub ip_address: String,
    pub cow_file_path: String,
}

/// Request payload to restore a VM from a TimeMachine snapshot or raw files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestoreRequest {
    #[serde(default)]
    pub vm_id: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub snapshot_path: Option<String>,
    #[serde(default)]
    pub mem_path: Option<String>,
    #[serde(default)]
    pub cow_path: Option<String>,
    #[serde(default)]
    pub original_vm_id: Option<String>,
}

/// Request payload for explicit TimeMachine rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTravelRequest {
    pub vm_id: String,
    pub snapshot_id: String,
}

/// Summary metadata for a managed VM returned by `GET /list` / `GET /vms`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmSummary {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub ip_address: String,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub attestation_status: Option<String>,
    #[serde(default)]
    pub base_image_path: String,
    #[serde(default)]
    pub vcpu: u32,
    #[serde(default)]
    pub mem_size_mib: u32,
}

/// Response payload listing all managed VMs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListResponse {
    pub vms: Vec<VmSummary>,
}

/// Request payload to pull an OCI/VMIF image via the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub image: String,
}

/// Response returned after pulling an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub status: String,
    pub path: String,
}

fn default_disk_id() -> String {
    "rootfs".to_string()
}

/// Request payload to live-resize an attached VM virtio-blk disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeDiskRequest {
    #[serde(default = "default_disk_id")]
    pub disk_id: String,
    pub desired_size: u64,
}

/// Response returned after attempting to resize a VM disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeDiskResponse {
    pub success: bool,
    pub vm_id: String,
    pub disk_id: String,
    pub desired_size: u64,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_request_defaults_deserialize() {
        let req: RunRequest = serde_json::from_str(r#"{"image":"ubuntu:24.04"}"#).unwrap();
        assert_eq!(req.image, "ubuntu:24.04");
        assert_eq!(req.vcpu, 1);
        assert_eq!(req.mem_size_mib, 512);
        assert!(req.ports.is_empty());
        assert!(req.volumes.is_empty());
        assert!(req.networks.is_empty());
    }

    #[test]
    fn resize_disk_request_default_disk_id() {
        let req: ResizeDiskRequest =
            serde_json::from_str(r#"{"desired_size": 10737418240}"#).unwrap();
        assert_eq!(req.disk_id, "rootfs");
        assert_eq!(req.desired_size, 10737418240);
    }

    #[test]
    fn list_response_tolerates_minimal_client_fields() {
        let json = r#"{"vms":[{"id":"vm-1","ip_address":"10.0.0.2","hostname":null}]}"#;
        let parsed: ListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.vms.len(), 1);
        assert_eq!(parsed.vms[0].id, "vm-1");
        assert!(parsed.vms[0].labels.is_empty());
        assert_eq!(parsed.vms[0].vcpu, 0);
    }
}
