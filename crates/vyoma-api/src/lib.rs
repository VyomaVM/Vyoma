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
    #[serde(default)]
    pub kernel: Option<String>,
    #[serde(default)]
    pub kernel_cmdline: Option<Vec<String>>,
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
            kernel: None,
            kernel_cmdline: None,
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
    #[serde(default)]
    pub artifact_type: Option<String>,
}

/// Response returned after pulling an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub status: String,
    pub path: String,
}

/// VMIF artifact classification for the daemon API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmifArtifactTypeApi {
    Software,
    Kernel,
    Composite,
    Appliance,
}

impl Default for VmifArtifactTypeApi {
    fn default() -> Self {
        Self::Appliance
    }
}

/// App Store certification status for the daemon API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatusApi {
    Certified,
    Community,
    Flagged,
    PendingReview,
}

impl Default for VerificationStatusApi {
    fn default() -> Self {
        Self::PendingReview
    }
}

/// Verification report response DTO.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReportResponse {
    pub artifact_digest: String,
    pub status: VerificationStatusApi,
    pub issuer: String,
    pub signed_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub static_scan_passed: bool,
    #[serde(default)]
    pub cve_critical_count: u32,
    #[serde(default)]
    pub detonation_passed: bool,
    #[serde(default)]
    pub golden_pcrs: HashMap<u32, String>,
    #[serde(default)]
    pub verity_root_hash: Option<String>,
}

/// Request to verify an image against local trust store or hub authority (`POST /images/verify`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyImageRequest {
    pub image_ref: String,
    #[serde(default)]
    pub enforce_certified: bool,
}

/// Response from verifying an image (`POST /images/verify`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyImageResponse {
    pub image_ref: String,
    pub status: VerificationStatusApi,
    pub is_trusted: bool,
    pub message: String,
    #[serde(default)]
    pub report: Option<VerificationReportResponse>,
}

/// Inspection metadata for a VMIF v2.0 image (`GET /images/:id/inspect`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmifInspectResponse {
    pub schema_version: u32,
    pub artifact_type: VmifArtifactTypeApi,
    pub media_type: String,
    pub arch: String,
    #[serde(default)]
    pub software_name: Option<String>,
    #[serde(default)]
    pub kernel_ref: Option<String>,
    #[serde(default)]
    pub verity_root_hash: Option<String>,
    #[serde(default)]
    pub verification_status: Option<VerificationStatusApi>,
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

    #[test]
    fn run_request_with_decoupled_kernel() {
        let json = r#"{
            "image": "vyomahub.io/library/postgres:16",
            "kernel": "vyomahub.io/kernels/db:6.16",
            "kernel_cmdline": ["transparent_hugepage=always", "elevator=deadline"]
        }"#;
        let req: RunRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "vyomahub.io/library/postgres:16");
        assert_eq!(req.kernel, Some("vyomahub.io/kernels/db:6.16".to_string()));
        assert_eq!(req.kernel_cmdline, Some(vec!["transparent_hugepage=always".to_string(), "elevator=deadline".to_string()]));
        assert_eq!(req.vcpu, 1);
        assert_eq!(req.mem_size_mib, 512);
    }

    #[test]
    fn pull_request_with_artifact_type() {
        let json = r#"{"image": "vyoma/kernel-db:6.16", "artifact_type": "kernel"}"#;
        let req: PullRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "vyoma/kernel-db:6.16");
        assert_eq!(req.artifact_type, Some("kernel".to_string()));
    }

    #[test]
    fn verification_report_roundtrip() {
        let mut pcrs = HashMap::new();
        pcrs.insert(0, "a1b2c3d4e5f6...".to_string());
        let report = VerificationReportResponse {
            artifact_digest: "sha256:4a8b79e1...".to_string(),
            status: VerificationStatusApi::Certified,
            issuer: "Vyoma Notary CA".to_string(),
            signed_at: "2026-11-21T12:00:00Z".to_string(),
            expires_at: Some("2027-11-21T12:00:00Z".to_string()),
            static_scan_passed: true,
            cve_critical_count: 0,
            detonation_passed: true,
            golden_pcrs: pcrs,
            verity_root_hash: Some("3f2504e0...".to_string()),
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: VerificationReportResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, VerificationStatusApi::Certified);
        assert_eq!(parsed.cve_critical_count, 0);
    }

    #[test]
    fn vmif_inspect_response_roundtrip() {
        let inspect = VmifInspectResponse {
            schema_version: 2,
            artifact_type: VmifArtifactTypeApi::Software,
            media_type: "application/vnd.vyoma.image.software.v2+json".to_string(),
            arch: "x86_64".to_string(),
            software_name: Some("vyoma/postgres".to_string()),
            kernel_ref: Some("vyomahub.io/kernels/db:6.16".to_string()),
            verity_root_hash: Some("3f2504e0...".to_string()),
            verification_status: Some(VerificationStatusApi::Certified),
        };

        let json = serde_json::to_string(&inspect).unwrap();
        let parsed: VmifInspectResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.artifact_type, VmifArtifactTypeApi::Software);
        assert_eq!(parsed.verification_status, Some(VerificationStatusApi::Certified));
    }
}
