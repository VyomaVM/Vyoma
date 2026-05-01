use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmRequest {
    pub image: String,
    pub vcpus: u32,
    pub memory_mb: u64,
    pub name: String,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVmResponse {
    pub vm_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmIdRequest {
    pub vm_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStatusResponse {
    pub vm_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVmsResponse {
    pub vms: Vec<VmInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub id: String,
    pub image: String,
    pub status: String,
    pub ip: String,
    pub vcpus: u32,
    pub memory_mb: u64,
    pub ports: Vec<PortMapping>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u32,
    pub vm: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMapping {
    pub host_path: String,
    pub vm_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub vm_id: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRequest {
    pub vm_id: String,
    pub follow: bool,
    pub tail: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub line: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRequest {
    pub vm_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub name: String,
    pub created_at: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub vm_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateRequest {
    pub vm_id: String,
    pub dest_address: String,
    pub bandwidth_mbps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub round: u32,
    pub pages_transferred: u64,
    pub total_pages: u64,
    pub bytes_transferred: u64,
    pub completed: bool,
    pub error: Option<String>,
}

impl Default for ListVmsResponse {
    fn default() -> Self {
        Self { vms: Vec::new() }
    }
}

impl Default for VmStatusResponse {
    fn default() -> Self {
        Self {
            vm_id: String::new(),
            status: "Unknown".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_vm_request() {
        let req = CreateVmRequest {
            image: "ubuntu:latest".to_string(),
            vcpus: 2,
            memory_mb: 2048,
            name: "test-vm".to_string(),
            ports: vec![PortMapping { host: 8080, vm: 80 }],
            volumes: vec![],
        };
        assert_eq!(req.vcpus, 2);
    }

    #[test]
    fn test_vm_info() {
        let info = VmInfo {
            id: "vm-123".to_string(),
            image: "nginx:latest".to_string(),
            status: "Running".to_string(),
            ip: "172.16.0.2".to_string(),
            vcpus: 4,
            memory_mb: 4096,
            ports: vec![],
            created_at: 1234567890,
        };
        assert_eq!(info.status, "Running");
    }

    #[test]
    fn test_migration_progress() {
        let progress = MigrationProgress {
            round: 5,
            pages_transferred: 10000,
            total_pages: 65536,
            bytes_transferred: 40960000,
            completed: false,
            error: None,
        };
        assert!(progress.error.is_none());
        assert!(!progress.completed);
    }

    #[test]
    fn test_exec_output() {
        let output = ExecOutput {
            stdout: b"Hello".to_vec(),
            stderr: b"".to_vec(),
            exit_code: 0,
        };
        assert_eq!(output.exit_code, 0);
    }

    #[test]
    fn test_snapshot_info() {
        let info = SnapshotInfo {
            snapshot_id: "snap-1".to_string(),
            name: "my-snapshot".to_string(),
            created_at: 1234567890,
            size_bytes: 1024000,
        };
        assert_eq!(info.name, "my-snapshot");
    }

    #[test]
    fn test_port_mapping() {
        let mapping = PortMapping { host: 8080, vm: 80 };
        assert_eq!(mapping.host, 8080);
        assert_eq!(mapping.vm, 80);
    }

    #[test]
    fn test_volume_mapping() {
        let mapping = VolumeMapping {
            host_path: "/data".to_string(),
            vm_path: "/app/data".to_string(),
        };
        assert_eq!(mapping.host_path, "/data");
    }

    #[test]
    fn test_log_request() {
        let req = LogRequest {
            vm_id: "vm-123".to_string(),
            follow: true,
            tail: 100,
        };
        assert!(req.follow);
        assert_eq!(req.tail, 100);
    }

    #[test]
    fn test_restore_request() {
        let req = RestoreRequest {
            vm_id: "vm-123".to_string(),
            snapshot_id: "snap-1".to_string(),
        };
        assert_eq!(req.vm_id, "vm-123");
        assert_eq!(req.snapshot_id, "snap-1");
    }
}
