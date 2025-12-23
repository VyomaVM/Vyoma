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

    /// Creates a sparse COW file.
    /// This is functionally identical to create_empty_file, but ensures semantic clarity.
    pub fn create_cow_file(path: &Path, size_mb: u64) -> Result<()> {
        info!("Creating COW file (sparse) of {}MB at {:?}", size_mb, path);
        Self::create_empty_file(path, size_mb)
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
    /// Attaches the file to a loop device. Returns the loop device path (e.g., /dev/loop0).
    /// Requires sudo.
    pub fn setup_loop_device(path: &Path) -> Result<String> {
        info!("Setting up loop device for {:?}", path);
        // losetup --find --show <path>
        let output = Command::new("sudo")
            .arg("losetup")
            .arg("--find")
            .arg("--show")
            .arg(path)
            .output()
            .map_err(|e| anyhow!("Failed to execute losetup: {}", e))?;

        if !output.status.success() {
            return Err(anyhow!("losetup failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let device = String::from_utf8(output.stdout)?.trim().to_string();
        info!("Attached {:?} to {}", path, device);
        Ok(device)
    }

    /// Detaches the loop device.
    /// Requires sudo.
    pub fn detach_loop_device(device_path: &str) -> Result<()> {
        info!("Detaching loop device {}", device_path);
        let status = Command::new("sudo")
            .arg("losetup")
            .arg("-d")
            .arg(device_path)
            .status()
            .map_err(|e| anyhow!("Failed to execute losetup -d: {}", e))?;

        if !status.success() {
             return Err(anyhow!("losetup -d failed with status: {}", status));
        }
        Ok(())
    }
}
