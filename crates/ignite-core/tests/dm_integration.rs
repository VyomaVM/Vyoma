use anyhow::Result;

#[test]
#[ignore]
fn test_dm_snapshot_lifecycle() -> Result<()> {
    // This test simulates the full lifecycle:
    // 1. Create Base Image (100MB)
    // 2. Loopback mount Base (Loop0)
    // 3. Create Cow File (100MB sparse)
    // 4. Loopback mount Cow (Loop1)
    // 5. Create DM Snapshot (Mapper) -> Write to it
    // 6. Cleanup
    
    use ignite_core::storage::StorageManager;
    use std::fs;

    let dir = tempfile::tempdir()?;
    let base_path = dir.path().join("base.ext4");
    let cow_path = dir.path().join("cow.img");
    let dm_name = "ignite-test-snapshot";

    // 1. Base
    StorageManager::create_empty_file(&base_path, 100)?;
    StorageManager::format_ext4(&base_path)?;
    let base_loop = StorageManager::setup_loop_device(&base_path)?;
    println!("Base attached to {}", base_loop);

    // 2. Cow
    // For DM snapshot, cow device must be block device too.
    StorageManager::create_cow_file(&cow_path, 100)?;
    let cow_loop = StorageManager::setup_loop_device(&cow_path)?;
    println!("Cow attached to {}", cow_loop);

    // 3. DM
    // Size in sectors. 1MB = 2048 sectors (512b). 100MB = 204800.
    let size_sectors = 100 * 1024 * 1024 / 512;
    let mapped_dev = StorageManager::create_dm_snapshot(dm_name, &base_loop, &cow_loop, size_sectors)?;
    println!("Mapped device created: {}", mapped_dev);

    // 4. Verify existence
    let exists = fs::metadata(&mapped_dev).is_ok();
    assert!(exists);

    // 5. Cleanup
    StorageManager::remove_dm_device(dm_name)?;
    StorageManager::detach_loop_device(&cow_loop)?;
    StorageManager::detach_loop_device(&base_loop)?;

    Ok(())
}
