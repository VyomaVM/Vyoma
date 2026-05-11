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
        if !crate::rootless::RootlessManager::is_root() {
            return Self::populate_image_rootless(image_path, source_dir);
        }
        
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

    pub fn populate_image_rootless(image_path: &Path, source_dir: &Path) -> Result<()> {
         info!("Populating {:?} using debugfs (rootless)", image_path);
        
        let mut script = String::new();
        let mut stack = vec![source_dir.to_path_buf()];
        
        while let Some(current_path) = stack.pop() {
            for entry in std::fs::read_dir(&current_path)? {
                 let entry = entry?;
                 let path = entry.path();
                 let rel_path = path.strip_prefix(source_dir)?.to_string_lossy().into_owned();
                 let vm_path = format!("/{}", rel_path);
                 
                 let file_type = entry.file_type()?;
                 if file_type.is_dir() {
                     script.push_str(&format!("mkdir \"{}\"\n", vm_path));
                     stack.push(path);
                 } else if file_type.is_file() {
                     script.push_str(&format!("write \"{}\" \"{}\"\n", path.to_string_lossy(), vm_path));
                 } else if file_type.is_symlink() {
                      let target = std::fs::read_link(&path)?;
                      script.push_str(&format!("symlink \"{}\" \"{}\"\n", vm_path, target.to_string_lossy()));
                 }
             }
        }
        
        use std::io::Write;
        let mut temp_script = tempfile::Builder::new().suffix(".debugfs").tempfile()?;
        write!(temp_script, "{}", script)?;
        
        let script_path = temp_script.path().to_owned();
        
        let status = Command::new("debugfs")
            .arg("-w")
            .arg("-f")
            .arg(&script_path)
            .arg(image_path)
            .stdout(std::process::Stdio::null())
            .status()
            .map_err(|e| anyhow!("Failed to execute debugfs: {}", e))?;

        if !status.success() {
             return Err(anyhow!("debugfs failed with status: {}", status));
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

    pub fn create_dm_snapshot(name: &str, base_dev: &str, cow_dev: &str, size_sectors: u64) -> Result<String> {
        // TODO(technical-debt): Migrate to devicemapper crate Rust API instead of dmsetup CLI
        info!("Creating Device Mapper snapshot '{}'", name);
        
        let table = format!("0 {} snapshot {} {} N 8", size_sectors, base_dev, cow_dev);
        
        let mut child = Command::new("dmsetup")
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

    pub fn remove_dm_device(name: &str) -> Result<()> {
        // TODO(technical-debt): Migrate to devicemapper crate Rust API instead of dmsetup CLI
        info!("Removing DM device '{}'", name);
        
        let status = Command::new("dmsetup")
            .arg("remove")
            .arg("--retry")
            .arg(name)
            .status()
            .map_err(|e| anyhow!("Failed to execute dmsetup remove: {}", e))?;
            
        if !status.success() {
            return Err(anyhow!("dmsetup remove failed for {}", name));
        }
        
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