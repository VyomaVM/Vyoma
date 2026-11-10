use anyhow::Result;
use vyoma_core::storage::StorageManager;
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
