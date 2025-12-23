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
    /// Creates a Device Mapper snapshot target.
    /// 
    /// Arguments:
    /// * `name`: Unique name for the mapping (e.g., "ignite-vm-123")
    /// * `base_dev`: Path to the read-only base device (e.g., /dev/loop0)
    /// * `cow_dev`: Path to the read-write COW device (e.g., /dev/loop1)
    /// * `size_sectors`: Size of the volume in 512-byte sectors.
    pub fn create_dm_snapshot(name: &str, base_dev: &str, cow_dev: &str, size_sectors: u64) -> Result<String> {
        info!("Creating Device Mapper snapshot '{}'", name);
        
        // Table format: <start_sector> <length> snapshot <origin> <cow> <persistent> <chunksize>
        // P = Persistent (survives reboot if metadata on disk, but here just means standard snapshot)
        // N = Not persistent (we usually use P or N, but 'P' is standard for generic file-backed snapshots in some contexts, 
        // actually for dm-snapshot 'P' or 'N' refers to metadata validity. 
        // For simple transient VMs, we might want 'N' if we don't care, but let's stick to standard syntax usage.
        // Actually, for `snapshot`, the args are: <origin> <COW device> <p|n> <chunksize>
        
        // 8 sectors = 4KB chunk size (standard page size)
        let table = format!("0 {} snapshot {} {} N 8", size_sectors, base_dev, cow_dev);
        
        // Pipe the table to dmsetup create
        let mut child = Command::new("sudo")
            .arg("dmsetup")
            .arg("create")
            .arg(name)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn dmsetup: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(table.as_bytes())?;
        }

        let status = child.wait()?;

        if !status.success() {
             return Err(anyhow!("dmsetup create failed for {}", name));
        }

        let device_path = format!("/dev/mapper/{}", name);
        info!("Created DM device: {}", device_path);
        
        Ok(device_path)
    }

    /// Removes a Device Mapper device.
    pub fn remove_dm_device(name: &str) -> Result<()> {
        info!("Removing DM device '{}'", name);
        
        let status = Command::new("sudo")
            .arg("dmsetup")
            .arg("remove")
            .arg("--retry") // Retry if busy handy for quick teardowns
            .arg(name)
            .status()
            .map_err(|e| anyhow!("Failed to execute dmsetup remove: {}", e))?;
            
        if !status.success() {
            return Err(anyhow!("dmsetup remove failed for {}", name));
        }
        
        Ok(())
    }
}

