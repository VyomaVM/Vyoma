use anyhow::Result;
use ignite_core::storage::StorageManager;
use std::fs;

#[test]
fn test_storage_ops() -> Result<()> {
    // 1. Create a temporary path for our image
    // NamedTempFile deletes itself on drop, but we want to persist it slightly for the test steps, 
    // or just let it exist. We actually want to *create* the file ourself with truncate.
    let dir = tempfile::tempdir()?;
    let image_path = dir.path().join("test_image.ext4");
    
    // 2. Create Empty File (e.g., 50MB)
    StorageManager::create_empty_file(&image_path, 50)?;
    
    let metadata = fs::metadata(&image_path)?;
    assert_eq!(metadata.len(), 50 * 1024 * 1024, "File size should be exactly 50MB");
    
    // 3. Format as ext4
    // This requires mkfs.ext4 to be installed on the system.
    StorageManager::format_ext4(&image_path)?;
    
    // We can't easily verify it is valid ext4 without mounting or `file` command, 
    // but if the command exited with 0, it likely worked.
    
    Ok(())
}

// Separate test for population because it requires SUDO
// Run with: cargo test --test storage_integration -- --ignored
#[test]
#[ignore]
fn test_storage_population() -> Result<()> {
    use ignite_core::storage::StorageManager;
    use std::fs::{self, File};
    use std::io::Write;

    let dir = tempfile::tempdir()?;
    let image_path = dir.path().join("rootfs.ext4");
    
    // Setup image
    StorageManager::create_empty_file(&image_path, 50)?;
    StorageManager::format_ext4(&image_path)?;

    // Setup source content
    let source_dir = dir.path().join("source");
    fs::create_dir(&source_dir)?;
    File::create(source_dir.join("hello.txt"))?.write_all(b"Hello World")?;

    // Attempt population (Will ask for SUDO password if run interactively, or fail)
    StorageManager::populate_image(&image_path, &source_dir)?;

    println!("Population successful (assumed if no panic)");
    Ok(())
}
