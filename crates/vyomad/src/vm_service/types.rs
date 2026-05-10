use std::collections::HashMap;
use std::path::PathBuf;

use vyoma_core::api::{PortMapping, VolumeMount};

#[derive(Debug, Clone)]
pub struct VmRunRequest {
    pub image: String,
    pub vcpu: u32,
    pub mem_size_mib: u32,
    pub ports: Vec<PortMapping>,
    pub volumes: Vec<VolumeMount>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    pub initramfs_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PolicyResult {
    pub passed: bool,
    pub attestation_pending: bool,
}

#[derive(Debug, Clone)]
pub struct VmInstanceSnapshot {
    pub vm_id: String,
    pub ch_socket_path: String,
    pub tap_name: String,
    pub dm_name: String,
    pub loop_devices: Vec<String>,
    pub cow_file_path: String,
    pub ip_address: String,
    pub cgroup_path: Option<String>,
    pub netns_path: Option<String>,
    pub config_ports: Vec<PortMapping>,
    pub config_volumes: Vec<VolumeMount>,
    pub hostname: Option<String>,
    pub labels: HashMap<String, String>,
    pub base_image_path: String,
    pub vcpu: u32,
    pub mem_size_mib: u32,
    pub networks: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_run_response_creation() {
        let response = VmRunResponse {
            vm_id: "test-vm-123".to_string(),
            status: "Running".to_string(),
            ip_address: "172.16.0.5".to_string(),
        };
        assert_eq!(response.vm_id, "test-vm-123");
        assert_eq!(response.status, "Running");
        assert_eq!(response.ip_address, "172.16.0.5");
    }

    #[test]
    fn test_prepared_image_paths() {
        let img = PreparedImage {
            path: PathBuf::from("/home/.ignite/images/alpine_latest/base.ext4"),
            config: vyoma_core::oci::OciImageConfig::default(),
        };
        assert!(img.path.to_string_lossy().contains("alpine"));
    }

    #[test]
    fn test_prepared_storage_loop_devices() {
        let storage = PreparedStorage {
            dm_device_path: "/dev/mapper/ign-test".to_string(),
            loop_devices: vec!["/dev/loop0".to_string(), "/dev/loop1".to_string()],
            cow_file_path: "/tmp/diff.cow".to_string(),
            dm_name: "ign-test".to_string(),
        };
        assert_eq!(storage.loop_devices.len(), 2);
        assert_eq!(storage.dm_name, "ign-test");
    }

    #[test]
    fn test_network_info_with_gateway() {
        let info = NetworkInfo {
            ip: "192.168.1.100".to_string(),
            tap_name: "tap12345678".to_string(),
            gateway: Some("192.168.1.1".to_string()),
            interface_name: "eth0".to_string(),
            network_name: "bridge0".to_string(),
        };
        assert!(info.gateway.is_some());
        assert_eq!(info.interface_name, "eth0");
    }

    #[test]
    fn test_ch_config_boot_args_contains_init() {
        let config = ChConfig {
            kernel_path: "/boot/vmlinuz".to_string(),
            ch_path: "/usr/bin/cloud-hypervisor".to_string(),
            socket_path: "/tmp/ch.sock".to_string(),
            boot_args: "console=ttyS0 init=/sbin/vyoma-init".to_string(),
            rootfs_path: "/dev/mapper/ign-123".to_string(),
            vsock_cid: 99,
            vsock_path: PathBuf::from("/tmp/vsock.sock"),
            initramfs_path: Some("/tmp/initramfs.cpio.gz".to_string()),
        };
        assert!(config.boot_args.contains("init=/sbin/vyoma-init"));
        assert!(config.initramfs_path.is_some());
    }

    #[test]
    fn test_agent_config_with_initramfs() {
        let config = AgentConfig {
            initramfs_path: Some(PathBuf::from("/tmp/initramfs.cpio")),
            cmd: vec!["/bin/sh".to_string()],
            workdir: "/app".to_string(),
            envs: vec!["PATH=/usr/bin".to_string()],
        };
        assert!(config.initramfs_path.is_some());
        assert_eq!(config.workdir, "/app");
    }

    #[test]
    fn test_policy_result_pending() {
        let result = PolicyResult {
            passed: false,
            attestation_pending: true,
        };
        assert!(!result.passed);
        assert!(result.attestation_pending);
    }

    #[test]
    fn test_vm_instance_snapshot_complete() {
        let snapshot = VmInstanceSnapshot {
            vm_id: "vm-snapshot-1".to_string(),
            ch_socket_path: "/tmp/ch.sock".to_string(),
            tap_name: "tap0abc".to_string(),
            dm_name: "ign-123".to_string(),
            loop_devices: vec!["/dev/loop0".to_string()],
            cow_file_path: "/tmp/cow".to_string(),
            ip_address: "172.16.0.5".to_string(),
            cgroup_path: Some("/sys/fs/cgroup".to_string()),
            netns_path: Some("/var/run/netns/vm-123".to_string()),
            config_ports: vec![],
            config_volumes: vec![],
            hostname: Some("test-vm".to_string()),
            labels: std::collections::HashMap::from([("app".to_string(), "test".to_string())]),
            base_image_path: "/home/.ignite/images/alpine".to_string(),
            vcpu: 4,
            mem_size_mib: 2048,
            networks: vec!["default".to_string()],
        };
        assert_eq!(snapshot.vcpu, 4);
        assert_eq!(snapshot.mem_size_mib, 2048);
    }

    #[test]
    fn test_types_serde_serialization() {
        let storage = PreparedStorage {
            dm_device_path: "/dev/mapper/ign-test".to_string(),
            loop_devices: vec!["/dev/loop0".to_string()],
            cow_file_path: "/tmp/diff.cow".to_string(),
            dm_name: "ign-test".to_string(),
        };
        let json = serde_json::to_string(&storage).unwrap();
        let parsed: PreparedStorage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dm_name, "ign-test");
    }

    #[test]
    fn test_vm_run_request_labels() {
        let req = VmRunRequest {
            image: "nginx:latest".to_string(),
            vcpu: 2,
            mem_size_mib: 1024,
            ports: vec![],
            volumes: vec![],
            hostname: None,
            networks: vec![],
            labels: std::collections::HashMap::from([
                ("service".to_string(), "web".to_string()),
                ("env".to_string(), "prod".to_string()),
            ]),
            base_image_path: String::new(),
        };
        assert_eq!(req.labels.get("service"), Some(&"web".to_string()));
        assert_eq!(req.labels.len(), 2);
    }
}