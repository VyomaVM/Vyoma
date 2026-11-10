use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use tracing::info;

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

    pub fn commit_snapshot(src_device: &Path, dst_file: &Path) -> Result<()> {
        info!("Committing disk image {:?} to file {:?}", src_device, dst_file);

        if !src_device.exists() {
            return Err(anyhow!("Source image does not exist: {:?}", src_device));
        }

        let mut src_f = std::fs::File::open(src_device).map_err(|e| anyhow!("Failed to open src: {}", e))?;
        let mut dst_f = std::fs::File::create(dst_file).map_err(|e| anyhow!("Failed to create dst: {}", e))?;
        std::io::copy(&mut src_f, &mut dst_f).map_err(|e| anyhow!("Failed to copy blocks: {}", e))?;

        Ok(())
    }
}