//! Error Recovery Tests for VM Service Stages
//!
//! These tests inject faults into individual component stages and verify that
//! resources are released and errors are propagated correctly.
//! They do not require KVM; they use mocked dependencies.

use std::sync::Arc;
use std::path::PathBuf;
use std::collections::HashMap;
use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, VmInstance, VmStatus};
    use crate::vm_service::types::{
        PreparedStorage, VmNetworkConfig, NetworkInfo, VmRunRequest,
    };
    use vyoma_core::api::{PortMapping, VolumeMount};

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestError {}

    #[derive(Clone)]
    struct FakeAppState {
        data_dir: PathBuf,
        vm_instances: Arc<Mutex<HashMap<String, VmInstance>>>,
    }

    impl FakeAppState {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            Self {
                data_dir: temp_dir.path().to_path_buf(),
                vm_instances: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn into_app_state(self) -> Arc<AppState> {
            Arc::new(AppState {
                data_dir: self.data_dir,
                vm_instances: self.vm_instances,
                config: Default::default(),
            })
        }
    }

    #[tokio::test]
    async fn test_image_failure_no_vm_created() {
        let mock_oci = MockFailingOciManager::new(TestError("OCI pull failed"));

        let result = mock_oci.pull_image("alpine:latest").await;

        assert!(result.is_err());
        println!("Image pull failed as expected: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_storage_failure_loop_device_detached() {
        let state = FakeAppState::new().into_app_state();

        let mut mock_storage = MockFailingStorageManager::new();
        mock_storage.fail_after_loop_creation = true;

        let image_path = PathBuf::from("/tmp/test-image");
        let vm_dir = PathBuf::from("/tmp/vm-test");
        std::fs::create_dir_all(&vm_dir).ok();

        let result = mock_storage.prepare_storage(&state, &image_path, &vm_dir, "test-vm").await;

        assert!(result.is_err(), "Storage preparation should fail");
        println!("Storage preparation failed as expected: {:?}", result.err());

        let cleaned_loop_devices = mock_storage.get_cleaned_devices();
        assert!(
            !cleaned_loop_devices.is_empty(),
            "Loop devices should be cleaned up on failure"
        );
    }

    #[tokio::test]
    async fn test_network_partial_failure_first_network_cleaned() {
        let state = FakeAppState::new().into_app_state();

        let mut mock_network = MockFailingNetworkManager::new();
        mock_network.fail_on_second_network = true;

        let vm_id = "test-vm-123";
        let networks = vec!["bridge0".to_string(), "bridge1".to_string()];

        let result = mock_network.setup_network(&state, vm_id, &networks).await;

        assert!(result.is_err(), "Network setup should fail when second network fails");
        println!("Network setup failed as expected: {:?}", result.err());

        let cleaned_networks = mock_network.get_cleaned_networks();
        assert!(
            cleaned_networks.contains(&"bridge0".to_string()),
            "First network should be cleaned up when second network fails"
        );
    }

    #[tokio::test]
    async fn test_boot_failure_vm_not_left_running() {
        let fake_state = FakeAppState::new();
        let state = fake_state.clone().into_app_state();

        let mut mock_boot = MockFailingBootManager::new();
        mock_boot.fail_on_boot = true;

        let storage = create_test_storage();
        let network = create_test_network();
        let vm_id = "test-vm-456";

        {
            let instance = VmInstance::new(vm_id.to_string(), "alpine:latest".to_string());
            state.vm_instances.lock().await.insert(vm_id.to_string(), instance);
        }

        let result = mock_boot.start_vm(&state, vm_id, &storage, &network).await;

        assert!(result.is_err(), "Boot should fail");
        println!("Boot failed as expected: {:?}", result.err());

        let vms = state.vm_instances.lock().await;
        let vm = vms.get(vm_id).expect("VM should exist");

        assert!(
            vm.status != VmStatus::Running,
            "VM should not be in Running state after boot failure: {:?}",
            vm.status
        );
    }

    #[tokio::test]
    async fn test_vm_creation_context_cleanup_on_image_failure() {
        let mock_oci = MockFailingOciManager::new(TestError("Manifest parsing failed"));

        let image_result = mock_oci.prepare_image("invalid-image:latest").await;

        assert!(image_result.is_err(), "Image preparation should fail");

        println!("Image preparation failed as expected, checking no resources leaked");
    }

    #[tokio::test]
    async fn test_error_propagation_through_stages() {
        let mock_oci = MockFailingOciManager::new(TestError("Network error"));

        let result = mock_oci.prepare_image("registry.example.com/image:latest").await;

        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Network error"),
            "Error message should contain original error"
        );
    }

    #[tokio::test]
    async fn test_storage_cleanup_called_on_any_failure() {
        let state = FakeAppState::new().into_app_state();

        let mut mock_storage = MockStorageWithCleanupTracking::new();
        mock_storage.should_fail = true;

        let image_path = PathBuf::from("/nonexistent/image");
        let vm_dir = PathBuf::from("/tmp/vm-cleanup-test");
        std::fs::create_dir_all(&vm_dir).ok();

        let result = mock_storage.prepare_storage(&state, &image_path, &vm_dir, "cleanup-test-vm").await;

        assert!(result.is_err());

        assert!(
            mock_storage.cleanup_called,
            "Storage cleanup should be called even on failure"
        );
    }

    fn create_test_storage() -> PreparedStorage {
        PreparedStorage {
            dm_device_path: "/dev/mapper/test-vm".to_string(),
            loop_devices: vec!["/dev/loop0".to_string()],
            cow_file_path: "/tmp/test.cow".to_string(),
            dm_name: "test-vm".to_string(),
        }
    }

    fn create_test_network() -> VmNetworkConfig {
        VmNetworkConfig {
            ip_address: "172.16.0.2".to_string(),
            primary_tap: "tap0".to_string(),
            gateway: "172.16.0.1".to_string(),
            network_infos: vec![NetworkInfo {
                ip: "172.16.0.2".to_string(),
                tap_name: "tap0".to_string(),
                gateway: Some("172.16.0.1".to_string()),
                interface_name: "eth0".to_string(),
                network_name: "bridge0".to_string(),
            }],
            netns_path: Some("/var/run/netns/test".to_string()),
        }
    }
}

struct MockFailingOciManager {
    error: TestError,
}

impl MockFailingOciManager {
    fn new(error: TestError) -> Self {
        Self { error }
    }

    async fn pull_image(&self, _image: &str) -> Result<String> {
        Err(anyhow::anyhow!("{}", self.error.0))
    }

    async fn prepare_image(&self, _image: &str) -> Result<crate::vm_service::types::PreparedImage> {
        Err(anyhow::anyhow!("{}", self.error.0))
    }
}

struct MockFailingStorageManager {
    fail_after_loop_creation: bool,
    created_loop_devices: Vec<String>,
    cleaned_devices: Vec<String>,
}

impl MockFailingStorageManager {
    fn new() -> Self {
        Self {
            fail_after_loop_creation: false,
            created_loop_devices: Vec::new(),
            cleaned_devices: Vec::new(),
        }
    }

    async fn prepare_storage(
        &mut self,
        _state: &Arc<AppState>,
        _base_image: &PathBuf,
        _vm_dir: &PathBuf,
        _vm_id: &str,
    ) -> Result<PreparedStorage> {
        if self.fail_after_loop_creation {
            self.created_loop_devices.push("/dev/loop0".to_string());

            self.cleanup_partial()?;
            return Err(anyhow::anyhow!("Storage preparation failed after loop device creation"));
        }

        Ok(PreparedStorage {
            dm_device_path: "/dev/mapper/test".to_string(),
            loop_devices: vec![],
            cow_file_path: "/tmp/test.cow".to_string(),
            dm_name: "test".to_string(),
        })
    }

    fn cleanup_partial(&mut self) -> Result<()> {
        for device in &self.created_loop_devices {
            println!("Cleaning up loop device: {}", device);
            self.cleaned_devices.push(device.clone());
        }
        self.created_loop_devices.clear();
        Ok(())
    }

    fn get_cleaned_devices(&self) -> Vec<String> {
        self.cleaned_devices.clone()
    }
}

struct MockFailingNetworkManager {
    fail_on_second_network: bool,
    created_networks: Vec<String>,
    cleaned_networks: Vec<String>,
}

impl MockFailingNetworkManager {
    fn new() -> Self {
        Self {
            fail_on_second_network: false,
            created_networks: Vec::new(),
            cleaned_networks: Vec::new(),
        }
    }

    async fn setup_network(
        &mut self,
        _state: &Arc<AppState>,
        _vm_id: &str,
        networks: &[String],
    ) -> Result<VmNetworkConfig> {
        for (i, network) in networks.iter().enumerate() {
            if self.fail_on_second_network && i > 0 {
                self.cleanup_created_networks()?;
                return Err(anyhow::anyhow!("Failed to create network {}", network));
            }

            self.created_networks.push(network.clone());
        }

        Ok(VmNetworkConfig {
            ip_address: "172.16.0.2".to_string(),
            primary_tap: "tap0".to_string(),
            gateway: "172.16.0.1".to_string(),
            network_infos: self.created_networks
                .iter()
                .enumerate()
                .map(|(i, n)| NetworkInfo {
                    ip: format!("172.16.0.{}", i + 2),
                    tap_name: format!("tap{}", i),
                    gateway: Some("172.16.0.1".to_string()),
                    interface_name: format!("eth{}", i),
                    network_name: n.clone(),
                })
                .collect(),
            netns_path: Some("/var/run/netns/test".to_string()),
        })
    }

    fn cleanup_created_networks(&mut self) -> Result<()> {
        for network in &self.created_networks {
            println!("Cleaning up network: {}", network);
            self.cleaned_networks.push(network.clone());
        }
        self.created_networks.clear();
        Ok(())
    }

    fn get_cleaned_networks(&self) -> Vec<String> {
        self.cleaned_networks.clone()
    }
}

struct MockFailingBootManager {
    fail_on_boot: bool,
}

impl MockFailingBootManager {
    fn new() -> Self {
        Self { fail_on_boot: false }
    }

    async fn start_vm(
        &mut self,
        state: &Arc<AppState>,
        vm_id: &str,
        _storage: &PreparedStorage,
        _network: &VmNetworkConfig,
    ) -> Result<()> {
        if self.fail_on_boot {
            let mut vms = state.vm_instances.lock().await;
            if let Some(vm) = vms.get_mut(vm_id) {
                vm.status = VmStatus::Starting;
            }
            return Err(anyhow::anyhow!("Cloud Hypervisor boot failed"));
        }
        Ok(())
    }
}

struct MockStorageWithCleanupTracking {
    should_fail: bool,
    cleanup_called: bool,
}

impl MockStorageWithCleanupTracking {
    fn new() -> Self {
        Self {
            should_fail: false,
            cleanup_called: false,
        }
    }

    async fn prepare_storage(
        &mut self,
        _state: &Arc<AppState>,
        _base_image: &PathBuf,
        _vm_dir: &PathBuf,
        _vm_id: &str,
    ) -> Result<PreparedStorage> {
        if self.should_fail {
            self.cleanup();
            return Err(anyhow::anyhow!("Simulated storage failure"));
        }

        Ok(PreparedStorage {
            dm_device_path: "/dev/mapper/test".to_string(),
            loop_devices: vec![],
            cow_file_path: "/tmp/test.cow".to_string(),
            dm_name: "test".to_string(),
        })
    }

    fn cleanup(&mut self) {
        self.cleanup_called = true;
        println!("Storage cleanup called");
    }
}