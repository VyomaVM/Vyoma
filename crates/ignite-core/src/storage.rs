use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

pub struct StorageManager;

impl StorageManager {
    /// Creates an empty sparse file of the given size (in MB).
    pub fn create_empty_file(path: &Path, size_mb: u64) -> Result<()> {
        info!("Creating {}MB file at {:?}", size_mb, path);
        let size_arg = format!("{}M", size_mb);
        
        let status = Command::new("truncate")
            .arg("-s")
            .arg(size_arg)
            .arg(path)
            .status()
            .map_err(|e| anyhow!("Failed to execute truncate: {}", e))?;

        if !status.success() {
            return Err(anyhow!("truncate failed with status: {}", status));
        }

        Ok(())
    }

    /// Formats the given file as ext4.
    pub fn format_ext4(path: &Path) -> Result<()> {
        info!("Formatting {:?} as ext4", path);
        
        // Use mkfs.ext4 on the file directly.
        // -F forces operation on a file (not a block device).
        let status = Command::new("mkfs.ext4")
            .arg("-F") 
            .arg(path)
            .status()
            .map_err(|e| anyhow!("Failed to execute mkfs.ext4: {}", e))?;

        if !status.success() {
            return Err(anyhow!("mkfs.ext4 failed with status: {}", status));
        }
        
        Ok(())
    }

    /// Mounts the image file, copies contents from source_dir, and unmounts.
    /// WARNING: This requires sudo/root access.
    pub fn populate_image(image_path: &Path, source_dir: &Path) -> Result<()> {
        let mount_point = tempfile::tempdir()?;
        let mount_path = mount_point.path();
        
        info!("Mounting {:?} to {:?}", image_path, mount_path);
        
        // 1. Mount (requires sudo)
        let status = Command::new("sudo")
            .arg("mount")
            .arg("-o")
            .arg("loop")
            .arg(image_path)
            .arg(mount_path)
            .status()
            .map_err(|e| anyhow!("Failed to execute sudo mount: {}", e))?;

        if !status.success() {
             return Err(anyhow!("Mount failed. Do you have sudo permissions? Status: {}", status));
        }

        // 2. Copy files (rsync is good for preserving partial attributes, or just cp -a)
        info!("Copying files from {:?} to {:?}", source_dir, mount_path);
        // source_dir/. ensures we copy contents, not the dir itself
        let src_pattern = format!("{}/.", source_dir.to_string_lossy());
        
        let status = Command::new("sudo")
            .arg("cp")
            .arg("-a")
            .arg(&src_pattern)
            .arg(mount_path)
            .status()?;
            
        if !status.success() {
             // Try to cleanup mount before returning error
             let _ = Command::new("sudo").arg("umount").arg(mount_path).status();
             return Err(anyhow!("Failed to copy files"));
        }

        // 3. Unmount
        info!("Unmounting...");
        let status = Command::new("sudo")
            .arg("umount")
            .arg(mount_path)
            .status()?;

        if !status.success() {
            warn!("Failed to unmount {:?}. You may need to do it manually.", mount_path);
        }

        Ok(())
    }
}
