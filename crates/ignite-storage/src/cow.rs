use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::{info, error};
use loopdev::LoopControl;

use crate::error::{StorageError, Result};

pub struct LoopDevice {
    pub path: PathBuf,
    // Store the underlying loopdev device so we can detach it natively
    device: Option<loopdev::LoopDevice>,
}

impl LoopDevice {
    pub fn new(path: PathBuf, device: Option<loopdev::LoopDevice>) -> Self {
        Self { path, device }
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct LoopManager {
    control: LoopControl,
}

impl LoopManager {
    pub fn new() -> Result<Self> {
        info!("Initializing Loop manager");
        let control = LoopControl::open().map_err(|e| StorageError::Io(e))?;
        Ok(Self { control })
    }
    
    pub fn attach(&self, file: &Path) -> Result<LoopDevice> {
        info!("Attaching loop device to {:?}", file);
        
        if !file.exists() {
            return Err(StorageError::NotFound(format!("File not found: {:?}", file)));
        }
        
        let ld = self.control.next_free().map_err(|e| StorageError::Io(e))?;
        
        // Ensure read/write
        ld.with().read_only(false).attach(file).map_err(|e| StorageError::Io(e))?;
        
        let loop_path = ld.path().unwrap_or_else(|| PathBuf::from(""));
        Ok(LoopDevice::new(loop_path, Some(ld)))
    }
    
    pub fn detach(&self, device: &LoopDevice) -> Result<()> {
        info!("Detaching loop device {:?}", device.path);
        
        if let Some(ld) = &device.device {
            ld.detach().map_err(|e| StorageError::Io(e))?;
        } else {
            // Unlikely to happen unless constructed manually without loopdev backend,
            // Fallback: try opening path
            if let Some(path_str) = device.path.to_str() {
                if let Ok(temp_ld) = loopdev::LoopDevice::open(path_str) {
                    temp_ld.detach().map_err(|e| StorageError::Io(e))?;
                }
            }
        }
        
        Ok(())
    }
    
    pub fn create_cow_file(path: &Path, size_mb: u64) -> Result<()> {
        info!("Creating COW file: {:?} ({} MB)", path, size_mb);
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let file = File::create(path)?;
        file.set_len(size_mb * 1024 * 1024)?;
        
        info!("COW file created successfully");
        Ok(())
    }
    
    pub fn get_size(path: &Path) -> Result<u64> {
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }
    
    pub fn is_attached(&self, device: &LoopDevice) -> Result<bool> {
        Ok(device.path.exists()) // Proper check might involve interrogating LoopControl natively
    }
    
    pub fn list_devices(&self) -> Result<Vec<LoopDevice>> {
        info!("Listing loop devices");
        let mut devices = Vec::new();
        // Since list_devices isn't trivially exposed or needed safely right now, we keep it minimum
        Ok(devices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_create_cow_file() {
        let temp_dir = TempDir::new().unwrap();
        let cow_path = temp_dir.path().join("test.cow");
        
        LoopManager::create_cow_file(&cow_path, 100).unwrap();
        
        assert!(cow_path.exists());
        assert!(LoopManager::get_size(&cow_path).unwrap() > 0);
    }
}
