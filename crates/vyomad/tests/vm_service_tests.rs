use std::path::PathBuf;
use vyomad::vm_service::types::*;

#[test]
fn test_oci_config_default() {
    let config = vyoma_core::oci::OciImageConfig::default();
    assert_eq!(config.cmd, None);
    assert_eq!(config.env, None);
    assert_eq!(config.working_dir, None);
}

#[test]
fn test_network_info_serialization() {
    let info = NetworkInfo {
        ip: "192.168.1.100".to_string(),
        tap_name: "tap12345678".to_string(),
        gateway: Some("192.168.1.1".to_string()),
        interface_name: "eth0".to_string(),
        network_name: "bridge0".to_string(),
    };

    let json = serde_json::to_string(&info).unwrap();
    let parsed: NetworkInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.ip, "192.168.1.100");
    assert_eq!(parsed.network_name, "bridge0");
}

#[test]
fn test_vm_run_response_serialization() {
    let resp = VmRunResponse {
        vm_id: "abc-123".to_string(),
        status: "Running".to_string(),
        ip_address: "10.0.2.15".to_string(),
    };

    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("abc-123"));
    assert!(json.contains("Running"));
}

#[test]
fn test_prepared_storage_serialization() {
    let storage = PreparedStorage {
        dm_device_path: "/dev/mapper/ign-test".to_string(),
        loop_devices: vec!["/dev/loop0".to_string()],
        cow_file_path: "/tmp/diff.cow".to_string(),
        dm_name: "ign-test".to_string(),
    };

    let json = serde_json::to_string(&storage).unwrap();
    let parsed: PreparedStorage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.dm_name, "ign-test");
    assert_eq!(parsed.loop_devices.len(), 1);
}

#[test]
fn test_vm_state_round_trip() {
    let state = vyomad::state::VmState {
        id: "vm-123".to_string(),
        tap_name: "tap0abc".to_string(),
        dm_name: "ign-123".to_string(),
        loop_devices: vec!["/dev/loop0".to_string()],
        cow_file_path: "/tmp/cow".to_string(),
        ip_address: "172.16.0.5".to_string(),
        cgroup_path: Some("/sys/fs/cgroup".to_string()),
        netns_path: Some("/var/run/netns/vm-123".to_string()),
        ports: vec![],
        volumes: vec![],
        hostname: Some("test".to_string()),
        labels: std::collections::HashMap::new(),
        base_image_path: "/home/.ignite/images/alpine".to_string(),
        vcpu: 2,
        mem_size_mib: 1024,
        networks: vec!["default".to_string()],
    };

    let json = serde_json::to_string(&state).unwrap();
    let parsed: vyomad::state::VmState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "vm-123");
    assert_eq!(parsed.vcpu, 2);
}

#[test]
fn test_policy_result() {
    let result = PolicyResult {
        passed: true,
        attestation_pending: false,
    };
    assert!(result.passed);
    assert!(!result.attestation_pending);
}

#[test]
fn test_ch_config_boot_args_format() {
    let ch_config = ChConfig {
        kernel_path: "/boot/vmlinuz".to_string(),
        ch_path: "/usr/bin/cloud-hypervisor".to_string(),
        socket_path: "/tmp/ch.sock".to_string(),
        boot_args: format!(
            "console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw ip=172.16.0.5::172.16.0.1:255.255.255.0:test:eth0:off:172.16.0.1 init=/sbin/vyoma-init"
        ),
        rootfs_path: "/dev/mapper/ign-123".to_string(),
        vsock_cid: 99,
        vsock_path: PathBuf::from("/tmp/vsock.sock"),
    };

    assert!(ch_config.boot_args.starts_with("console=ttyS0"));
    assert!(ch_config.boot_args.contains("ip=172.16.0.5"));
    assert_eq!(ch_config.vsock_cid, 99);
}

#[test]
fn test_agent_config_initramfs_path() {
    let config = AgentConfig {
        initramfs_path: Some(PathBuf::from("/tmp/initramfs.cpio")),
        init_script_path: PathBuf::from("/tmp/vyoma-init.sh"),
        cmd: vec!["/bin/bash".to_string(), "-c".to_string(), "exec /bin/sh".to_string()],
        workdir: "/workspace".to_string(),
        envs: vec!["DEBUG=1".to_string(), "LOG_LEVEL=info".to_string()],
    };

    assert!(config.initramfs_path.is_some());
    assert_eq!(config.workdir, "/workspace");
    assert_eq!(config.cmd.len(), 3);
}

#[test]
fn test_wal_entry_creation() {
    let entry = vyomad::state::wal::WalEntry::vm_create("vm-abc".to_string());
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("vm_create") || json.contains("VmCreate"));
}

#[test]
fn test_vm_instance_snapshot_size() {
    let snapshot = VmInstanceSnapshot {
        vm_id: "test-vm".to_string(),
        ch_socket_path: "/tmp/ch.sock".to_string(),
        tap_name: "tap0".to_string(),
        dm_name: "ign-test".to_string(),
        loop_devices: vec![],
        cow_file_path: "/tmp/cow".to_string(),
        ip_address: "10.0.2.15".to_string(),
        cgroup_path: None,
        netns_path: None,
        config_ports: vec![],
        config_volumes: vec![],
        hostname: None,
        labels: std::collections::HashMap::new(),
        base_image_path: String::new(),
        vcpu: 1,
        mem_size_mib: 512,
        networks: vec![],
    };

    assert_eq!(snapshot.vm_id, "test-vm");
    assert_eq!(snapshot.vcpu, 1);
}

#[test]
fn test_types_clone() {
    let storage = PreparedStorage {
        dm_device_path: "/dev/mapper/test".to_string(),
        loop_devices: vec!["/dev/loop0".to_string()],
        cow_file_path: "/tmp/cow".to_string(),
        dm_name: "ign-test".to_string(),
    };

    let storage_clone = storage.clone();
    assert_eq!(storage.dm_name, storage_clone.dm_name);
}

#[test]
fn test_vm_run_request_with_ports() {
    let req = vyomad::api::handlers::RunRequest {
        image: "nginx:latest".to_string(),
        vcpu: 4,
        mem_size_mib: 2048,
        ports: vec![
            vyoma_core::api::PortMapping { host_port: 8080, vm_port: 80 },
            vyoma_core::api::PortMapping { host_port: 8443, vm_port: 443 },
        ],
        volumes: vec![
            vyoma_core::api::VolumeMount { host_path: "/data".to_string(), vm_path: "/mnt".to_string() },
        ],
        hostname: Some("nginx-server".to_string()),
        networks: vec!["bridge0".to_string(), "bridge1".to_string()],
        labels: std::collections::HashMap::from([
            ("service".to_string(), "nginx".to_string()),
            ("version".to_string(), "latest".to_string()),
        ]),
        base_image_path: String::new(),
    };

    let vm_req: VmRunRequest = req.into();
    assert_eq!(vm_req.ports.len(), 2);
    assert_eq!(vm_req.volumes.len(), 1);
    assert_eq!(vm_req.networks.len(), 2);
    assert_eq!(vm_req.labels.get("service"), Some(&"nginx".to_string()));
}

#[test]
fn test_prepared_storage_fields() {
    let storage = PreparedStorage {
        dm_device_path: "/dev/mapper/test".to_string(),
        loop_devices: vec!["/dev/loop0".to_string(), "/dev/loop1".to_string()],
        cow_file_path: "/tmp/cow.raw".to_string(),
        dm_name: "ign-test".to_string(),
    };

    assert_eq!(storage.dm_device_path, "/dev/mapper/test");
    assert_eq!(storage.loop_devices.len(), 2);
    assert_eq!(storage.dm_name, "ign-test");
}

#[test]
fn test_vm_network_config() {
    let config = VmNetworkConfig {
        ip_address: "172.16.0.5".to_string(),
        primary_tap: "tap0abc".to_string(),
        gateway: "172.16.0.1".to_string(),
        network_infos: vec![
            NetworkInfo {
                ip: "172.16.0.5".to_string(),
                tap_name: "tap0abc".to_string(),
                gateway: Some("172.16.0.1".to_string()),
                interface_name: "eth0".to_string(),
                network_name: "default".to_string(),
            },
        ],
        netns_path: Some("/var/run/netns/vm-123".to_string()),
    };

    assert_eq!(config.ip_address, "172.16.0.5");
    assert!(config.netns_path.is_some());
    assert_eq!(config.network_infos.len(), 1);
}

#[test]
fn test_ch_config_validation() {
    let config = ChConfig {
        kernel_path: "/var/lib/ignite/bin/vmlinux".to_string(),
        ch_path: "/var/lib/ignite/bin/cloud-hypervisor".to_string(),
        socket_path: "/tmp/vm.sock".to_string(),
        boot_args: "console=ttyS0".to_string(),
        rootfs_path: "/dev/mapper/test".to_string(),
        vsock_cid: 42,
        vsock_path: PathBuf::from("/tmp/vsock.sock"),
    };

    assert_eq!(config.vsock_cid, 42);
    assert!(config.boot_args.contains("console=ttyS0"));
}

#[test]
fn test_prepared_image() {
    let img = PreparedImage {
        path: PathBuf::from("/home/user/.ignite/images/alpine_latest/base.ext4"),
        config: vyoma_core::oci::OciImageConfig::default(),
    };

    assert!(img.path.to_string_lossy().contains("alpine"));
}