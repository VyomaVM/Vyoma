use std::path::Path;
use std::process::Command;
use tracing::{info, error};

use crate::error::{StorageError, Result};

pub struct Ext4Manager;

impl Ext4Manager {
    /// Format a device or sparse file with ext4 filesystem
    pub fn format(path: &Path) -> Result<()> {
        info!("Formatting {:?} as ext4", path);
        if !path.exists() {
            return Err(StorageError::NotFound(format!("Path not found for ext4 formatting: {:?}", path)));
        }

        // We use mkfs.ext4 out of necessity, as there is currently no production-ready
        // pure-Rust standard library to author ext4 filesystems directly.
        // We use Command explicitly with fixed args to prevent injection.
        let output = Command::new("mkfs.ext4")
            .arg("-F") // Force (needed for formatting a file instead of a block device without prompting)
            .arg("-b")
            .arg("4096") // Standard 4k block size
            .arg(path)
            .output()
            .map_err(|e| StorageError::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("mkfs.ext4 failed: {}", stderr);
            return Err(StorageError::Other(format!("Failed to format ext4: {}", stderr)));
        }

        info!("Successfully formatted ext4 filesystem");
        Ok(())
    }
}
