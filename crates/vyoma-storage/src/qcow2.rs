use std::path::Path;
use std::process::Command;
use tracing::{info, error};
use serde::{Deserialize, Serialize};

use crate::error::{StorageError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qcow2Info {
    #[serde(rename = "virtual-size")]
    pub virtual_size: u64,
    pub filename: String,
    pub format: String,
    #[serde(rename = "actual-size")]
    pub actual_size: Option<u64>,
    #[serde(rename = "backing-filename")]
    pub backing_filename: Option<String>,
    #[serde(rename = "backing-filename-format")]
    pub backing_filename_format: Option<String>,
}

pub struct Qcow2Manager;

impl Qcow2Manager {
    /// Create a new independent QCOW2 image of specified size in megabytes.
    pub fn create_image(path: &Path, size_mb: u64) -> Result<()> {
        info!("Creating QCOW2 disk image at {:?} ({}MB)", path, size_mb);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }

        let output = Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg("qcow2")
            .arg(path)
            .arg(format!("{}M", size_mb))
            .output()
            .map_err(|e| StorageError::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to create QCOW2 image: {}", stderr);
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("qemu-img create failed: {}", stderr),
            )));
        }

        info!("Successfully created QCOW2 image at {:?}", path);
        Ok(())
    }

    /// Create a copy-on-write overlay image pointing to a backing file.
    /// Cloud Hypervisor v54.0 natively supports QCOW2 backing files for instant VM branching.
    pub fn create_overlay(overlay_path: &Path, backing_file: &Path) -> Result<()> {
        info!(
            "Creating QCOW2 overlay {:?} backed by {:?}",
            overlay_path, backing_file
        );
        if !backing_file.exists() {
            return Err(StorageError::NotFound(format!(
                "Backing file does not exist: {:?}",
                backing_file
            )));
        }
        if let Some(parent) = overlay_path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }

        let backing_path_str = backing_file.to_str().ok_or_else(|| {
            StorageError::Path(format!("Invalid backing file path: {:?}", backing_file))
        })?;

        let output = Command::new("qemu-img")
            .arg("create")
            .arg("-f")
            .arg("qcow2")
            .arg("-b")
            .arg(backing_path_str)
            .arg("-F")
            .arg("qcow2")
            .arg(overlay_path)
            .output()
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to create QCOW2 overlay: {}", stderr);
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("qemu-img create overlay failed: {}", stderr),
            )));
        }

        info!("Successfully created QCOW2 overlay at {:?}", overlay_path);
        Ok(())
    }

    /// Resize an existing QCOW2 image.
    pub fn resize_image(path: &Path, new_size_mb: u64) -> Result<()> {
        info!("Resizing QCOW2 image at {:?} to {}MB", path, new_size_mb);
        if !path.exists() {
            return Err(StorageError::NotFound(format!("File not found: {:?}", path)));
        }

        let output = Command::new("qemu-img")
            .arg("resize")
            .arg(path)
            .arg(format!("{}M", new_size_mb))
            .output()
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to resize QCOW2 image: {}", stderr);
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("qemu-img resize failed: {}", stderr),
            )));
        }

        info!("Successfully resized QCOW2 image at {:?}", path);
        Ok(())
    }

    /// Query metadata and inspect a QCOW2 image.
    pub fn get_image_info(path: &Path) -> Result<Qcow2Info> {
        if !path.exists() {
            return Err(StorageError::NotFound(format!("File not found: {:?}", path)));
        }

        let output = Command::new("qemu-img")
            .arg("info")
            .arg("--output=json")
            .arg(path)
            .output()
            .map_err(StorageError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("qemu-img info failed: {}", stderr),
            )));
        }

        let info: Qcow2Info = serde_json::from_slice(&output.stdout).map_err(|e| {
            StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse qemu-img JSON output: {}", e),
            ))
        })?;

        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_qcow2_lifecycle() {
        let dir = tempdir().unwrap();
        let base_img = dir.path().join("base.qcow2");
        let overlay_img = dir.path().join("overlay.qcow2");

        // 1. Create base 100MB image
        Qcow2Manager::create_image(&base_img, 100).expect("create base image failed");
        assert!(base_img.exists());

        // 2. Query info
        let base_info = Qcow2Manager::get_image_info(&base_img).expect("get base info failed");
        assert_eq!(base_info.format, "qcow2");
        assert_eq!(base_info.virtual_size, 100 * 1024 * 1024);
        assert!(base_info.backing_filename.is_none());

        // 3. Create overlay backed by base
        Qcow2Manager::create_overlay(&overlay_img, &base_img).expect("create overlay failed");
        assert!(overlay_img.exists());

        let overlay_info = Qcow2Manager::get_image_info(&overlay_img).expect("get overlay info failed");
        assert_eq!(overlay_info.format, "qcow2");
        assert!(overlay_info.backing_filename.is_some());

        // 4. Resize overlay
        Qcow2Manager::resize_image(&overlay_img, 200).expect("resize overlay failed");
        let resized_info = Qcow2Manager::get_image_info(&overlay_img).expect("get resized info failed");
        assert_eq!(resized_info.virtual_size, 200 * 1024 * 1024);
    }
}
