use std::path::PathBuf;
use crate::vm_service::types::*;

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_prepared_storage_serialization() {
        let storage = PreparedStorage {
            dm_device_path: "/dev/mapper/ign-test".to_string(),
            loop_devices: vec!["/dev/loop0".to_string(), "/dev/loop1".to_string()],
            cow_file_path: "/tmp/diff.cow".to_string(),
            dm_name: "ign-test".to_string(),
        };
        
        let json = serde_json::to_string(&storage).unwrap();
        let parsed: PreparedStorage = serde_json::from_str(&json).unwrap();
        
        assert_eq!(parsed.dm_name, "ign-test");
        assert_eq!(parsed.loop_devices.len(), 2);
    }

    #[test]
    fn test_vm_network_config_complete() {
        let config = VmNetworkConfig {
            ip_address: "172.16.0.5".to_string(),
            primary_tap: "tap12345678".to_string(),
            gateway: "172.16.0.1".to_string(),
            network_infos: vec![
                NetworkInfo {
                    ip: "172.16.0.5".to_string(),
                    tap_name: "tap12345678".to_string(),
                    gateway: Some("172.16.0.1".to_string()),
                    interface_name: "eth0".to_string(),
                    network_name: "bridge0".to_string(),
                },
                NetworkInfo {
                    ip: "172.16.0.6".to_string(),
                    tap_name: "tap12345678-1".to_string(),
                    gateway: Some("172.16.0.1".to_string()),
                    interface_name: "eth1".to_string(),
                    network_name: "bridge1".to_string(),
                }
            ],
            netns_path: Some("/var/run/netns/vm-test".to_string()),
        };
        
        assert_eq!(config.ip_address, "172.16.0.5");
        assert_eq!(config.network_infos.len(), 2);
        assert!(config.netns_path.is_some());
    }

    #[test]
    fn test_ch_config_with_boot_args() {
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
        assert_eq!(config.vsock_cid, 99);
    }

    #[test]
    fn test_vm_run_request_with_labels() {
        let req = VmRunRequest {
            image: "nginx:latest".to_string(),
            vcpu: 4,
            mem_size_mib: 8192,
            ports: vec![],
            volumes: vec![],
            hostname: Some("web-server".to_string()),
            networks: vec!["default".to_string()],
            labels: std::collections::HashMap::from([
                ("service".to_string(), "web".to_string()),
                ("env".to_string(), "prod".to_string()),
            ]),
            base_image_path: "/home/.ignite/images/nginx".to_string(),
        };
        
        assert_eq!(req.vcpu, 4);
        assert_eq!(req.labels.get("service"), Some(&"web".to_string()));
        assert_eq!(req.labels.len(), 2);
    }

    #[test]
    fn test_policy_result_pending_attestation() {
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
        assert!(snapshot.cgroup_path.is_some());
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
    fn test_network_info_gateway_optional() {
        let info = NetworkInfo {
            ip: "10.0.2.15".to_string(),
            tap_name: "tap0".to_string(),
            gateway: None,
            interface_name: "eth0".to_string(),
            network_name: "slirp".to_string(),
        };
        
        assert!(info.gateway.is_none());
        assert_eq!(info.network_name, "slirp");
    }

    #[test]
    fn test_snapshot_result_struct() {
        use crate::vm_service::state::SnapshotResult;
        
        let result = SnapshotResult {
            id: "snap-abc123".to_string(),
            path: PathBuf::from("/home/.ignite/vms/test/snapshots/snap-abc123/snapshot.bin"),
        };
        
        assert_eq!(result.id, "snap-abc123");
        assert!(result.path.to_string_lossy().contains("snapshot.bin"));
    }

    #[test]
    fn test_vm_run_response_serialization() {
        let response = VmRunResponse {
            vm_id: "test-vm-456".to_string(),
            status: "Running".to_string(),
            ip_address: "192.168.1.50".to_string(),
        };
        
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("test-vm-456"));
        assert!(json.contains("Running"));
        
        let parsed: VmRunResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.vm_id, "test-vm-456");
    }

    #[test]
    fn test_types_clone() {
        let config = VmNetworkConfig {
            ip_address: "10.0.0.1".to_string(),
            primary_tap: "tap0".to_string(),
            gateway: "10.0.0.254".to_string(),
            network_infos: vec![],
            netns_path: None,
        };
        
        let cloned = config.clone();
        assert_eq!(cloned.ip_address, config.ip_address);
    }
}
