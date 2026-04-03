use anyhow::Result;
use ignite_core::vmm::VmmManager;
use ignite_core::storage::StorageManager;
use tempfile::tempdir;

#[tokio::test]
#[ignore]
async fn test_firecracker_lifecycle() -> Result<()> {
    // This requires:
    // 1. KVM access (might fail in CI/WSL without nested virt)
    // 2. A valid Kernel file
    // 3. A valid Rootfs
    // We will just test the process spawning and API configuration, BUT fail before Start if no KVM.
    
    // Check if KVM exists
    if !std::path::Path::new("/dev/kvm").exists() {
        println!("Skipping test: /dev/kvm not found");
        return Ok(());
    }

    let dir = tempdir()?;
    let socket_path = dir.path().join("firecracker.socket");
    let socket_str = socket_path.to_str().unwrap();
    
    // Assume firecracker is in path or ./bin/firecracker
    let fc_path = "bin/firecracker";
    if !std::path::Path::new(fc_path).exists() {
         println!("Skipping test: firecracker binary not found at {}", fc_path);
         return Ok(());
    }

    let mut vmm = VmmManager::new(socket_str);
    vmm.start_daemon(fc_path, None, false)?;

    // Create dummy kernel and rootfs just to pass API validation (Firecracker checks file existence)
    let kernel_path = dir.path().join("vmlinux");
    let rootfs_path = dir.path().join("rootfs.ext4");
    StorageManager::create_empty_file(&kernel_path, 1)?;
    StorageManager::create_empty_file(&rootfs_path, 10)?;
    
    println!("Configuring Boot Source...");
    vmm.set_boot_source(kernel_path.to_str().unwrap(), "console=ttyS0 reboot=k panic=1 pci=off").await?;
    
    println!("Configuring Drive...");
    vmm.add_drive("rootfs", rootfs_path.to_str().unwrap(), true).await?;
    
    println!("Configuring Machine...");
    vmm.set_machine_config(1, 128).await?;
    
    // We DON'T call start_instance() because the kernel file is empty/garbage and it would crash/fail immediately.
    // Use a real kernel for full integration test.
    
    println!("Configuration successful via API!");
    
    Ok(())
}
