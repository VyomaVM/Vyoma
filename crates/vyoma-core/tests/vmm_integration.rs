use anyhow::Result;
use vyoma_core::vmm::VmmManager;
use vyoma_core::storage::StorageManager;
use tempfile::tempdir;

#[tokio::test]
#[ignore]
async fn test_ch_lifecycle() -> Result<()> {
    // This requires:
    // 1. KVM access (might fail in CI/WSL without nested virt)
    // 2. A valid Kernel file
    // 3. A valid Rootfs
    
    // Check if KVM exists
    if !std::path::Path::new("/dev/kvm").exists() {
        println!("Skipping test: /dev/kvm not found");
        return Ok(());
    }

    let dir = tempdir()?;
    let socket_path = dir.path().join("cloud-hypervisor.socket");
    let socket_str = socket_path.to_str().unwrap();
    
    // Assume cloud-hypervisor is in path or ./bin/cloud-hypervisor
    let ch_path = "bin/cloud-hypervisor";
    if !std::path::Path::new(ch_path).exists() {
         println!("Skipping test: cloud-hypervisor binary not found at {}", ch_path);
         return Ok(());
    }

    let mut vmm = VmmManager::new(socket_str);
    vmm.start_daemon(ch_path, None, false)?;

    // Create dummy kernel and rootfs just to pass API validation
    let kernel_path = dir.path().join("vmlinux");
    let rootfs_path = dir.path().join("rootfs.ext4");
    StorageManager::create_empty_file(&kernel_path, 1)?;
    StorageManager::create_empty_file(&rootfs_path, 10)?;
    
    println!("Testing check_alive API endpoint...");
    let alive = vmm.check_alive().await;
    assert!(alive, "Cloud Hypervisor ping API should return success");
    
    println!("Configuring Boot Source...");
    vmm.set_boot_source(kernel_path.to_str().unwrap(), "console=ttyS0 reboot=k panic=1 pci=off", None).await?;
    
    println!("Configuring Drive...");
    vmm.add_drive("rootfs", rootfs_path.to_str().unwrap(), true).await?;
    
    println!("Configuring Machine...");
    vmm.set_machine_config(1, 128).await?;
    
    // We DON'T call start_instance() because the kernel file is empty/garbage and it would crash/fail immediately.
    // Use a real kernel for full integration test.
    
    println!("Configuration builder successful!");
    
    Ok(())
}

