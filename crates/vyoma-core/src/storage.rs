use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

pub struct StorageManager;

impl StorageManager {
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

    pub fn format_ext4(path: &Path) -> Result<()> {
        info!("Formatting {:?} as ext4", path);
        
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

    pub fn populate_image(image_path: &Path, source_dir: &Path) -> Result<()> {
        let mount_point = tempfile::tempdir()?;
        let mount_path = mount_point.path();
        
        info!("Mounting {:?} to {:?}", image_path, mount_path);
        
        let status = Command::new("mount")
            .arg("-o")
            .arg("loop")
            .arg(image_path)
            .arg(mount_path)
            .status()
            .map_err(|e| anyhow!("Failed to execute mount: {}", e))?;

        if !status.success() {
             return Err(anyhow!("Mount failed. Status: {}", status));
        }

        info!("Copying files from {:?} to {:?}", source_dir, mount_path);
        let src_pattern = format!("{}/.", source_dir.to_string_lossy());
        
        let status = Command::new("cp")
            .arg("-a")
            .arg(&src_pattern)
            .arg(mount_path)
            .status()?;
            
        if !status.success() {
             let _ = Command::new("umount").arg(mount_path).status();
             return Err(anyhow!("Failed to copy files"));
        }

        info!("Unmounting...");
        let status = Command::new("umount")
            .arg(mount_path)
            .status()?;

        if !status.success() {
            warn!("Failed to unmount {:?}. You may need to do it manually.", mount_path);
        }

Ok(())
    }

    pub fn setup_loop_device(path: &Path) -> Result<String> {
        info!("Delegating to vyoma-storage LoopManager for {:?}", path);
        
        let lm = vyoma_storage::LoopManager::new()?;
        let device = lm.attach(path)?;
        
        let device_path = device.path().to_string_lossy().to_string();
        info!("Attached {:?} to {}", path, device_path);
        Ok(device_path)
    }

    pub fn detach_loop_device(device_path: &str) -> Result<()> {
        info!("Delegating to vyoma-storage for loop device detach: {}", device_path);
        
        let lm = vyoma_storage::LoopManager::new()?;
        let device = vyoma_storage::LoopDevice::new(
            std::path::PathBuf::from(device_path),
            None,
        );
        lm.detach(&device)?;
        
        info!("Detached loop device {}", device_path);
        Ok(())
    }

    pub fn create_dm_snapshot(name: &str, base_dev: &str, cow_dev: &str, _size_sectors: u64) -> Result<String> {
        use vyoma_storage::DmManager;

        let dm_manager = DmManager::new()?;
        let base_path = std::path::Path::new(base_dev);
        let cow_path = std::path::Path::new(cow_dev);
        let dm_device = dm_manager.create_snapshot(&name, base_path, cow_path)?;
        Ok(dm_device.path().to_string_lossy().to_string())
    }

    pub fn remove_dm_device(name: &str) -> Result<()> {
        use vyoma_storage::DmManager;

        let dm_manager = DmManager::new()?;
        dm_manager.remove_snapshot(name)?;
        Ok(())
    }

    pub fn commit_snapshot(src_device: &Path, dst_file: &Path) -> Result<()> {
        info!("Committing block device {:?} to file {:?}", src_device, dst_file);
        
        if !src_device.exists() {
            return Err(anyhow!("Source device does not exist: {:?}", src_device));
        }

        let mut src_f = std::fs::File::open(src_device).map_err(|e| anyhow!("Failed to open src: {}", e))?;
        let mut dst_f = std::fs::File::create(dst_file).map_err(|e| anyhow!("Failed to create dst: {}", e))?;
        std::io::copy(&mut src_f, &mut dst_f).map_err(|e| anyhow!("Failed to copy blocks: {}", e))?;
        
        Ok(())
    }
}