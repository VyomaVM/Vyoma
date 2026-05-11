use std::path::{Path, PathBuf};
use tracing::{info, error};

// Add devicemapper imports
use devicemapper::{DM, DmName, DmOptions, DevId, Sectors, Device};
use std::process::Command;
use std::fs;

use crate::error::{StorageError, Result};

#[derive(Debug, Clone)]
pub struct DmDevice {
    pub name: String,
    pub path: PathBuf,
}

impl DmDevice {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct DmManager {
    dm: DM,
}

impl DmManager {
    pub fn new() -> Result<Self> {
        info!("Initializing Device Mapper manager");
        let dm = DM::new().map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(Self { dm })
    }
    
    fn device_size_sectors(path: &Path) -> Result<u64> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).map_err(StorageError::Io)?;
        let bytes = meta.size();
        Ok(bytes / 512)
    }

    pub fn create_snapshot(
        &self,
        name: &str,
        base_dev: &Path,
        cow_dev: &Path,
    ) -> Result<DmDevice> {
        info!("Creating snapshot {}: base={:?}, cow={:?}", name, base_dev, cow_dev);
        
        if !base_dev.exists() {
            return Err(StorageError::NotFound(format!("Base device not found: {:?}", base_dev)));
        }
        if !cow_dev.exists() {
            return Err(StorageError::NotFound(format!("COW device not found: {:?}", cow_dev)));
        }
        
        let origin_name_str = format!("{}-origin", name);
        let sectors = Self::device_size_sectors(base_dev)?;

        // 1. Create origin linear device via dmsetup (as LinearDev & SnapshotDev traits vary wildly between compiler versions)
        // TODO(technical-debt): Migrate to devicemapper crate Rust API instead of CLI
        // We use safe system Command building to ensure strict compliance without trait mismatch.
        // Origin target wraps the read-only base
        let table_origin = format!("0 {} linear {} 0", sectors, base_dev.display());
        let mut dm_origin = Command::new("dmsetup");
        dm_origin.arg("create").arg(&origin_name_str).arg("--table").arg(&table_origin);
        
        let origin_out = dm_origin.output().map_err(StorageError::Io)?;
        if !origin_out.status.success() {
            return Err(StorageError::Other(format!("Failed to create origin dev via dmsetup: {:?}", String::from_utf8_lossy(&origin_out.stderr))));
        }
            
        // 2. Create snapshot device
        let origin_dev_path = format!("/dev/mapper/{}", origin_name_str);
        let table_snap = format!("0 {} snapshot {} {} P 8", sectors, origin_dev_path, cow_dev.display());
        let mut dm_snap = Command::new("dmsetup");
        dm_snap.arg("create").arg(name).arg("--table").arg(&table_snap);
        
        let snap_out = dm_snap.output().map_err(StorageError::Io)?;
        if !snap_out.status.success() {
            // cleanup origin
            let _ = Command::new("dmsetup").arg("remove").arg(&origin_name_str).output();
            return Err(StorageError::Other(format!("Failed to create snapshot dev via dmsetup: {:?}", String::from_utf8_lossy(&snap_out.stderr))));
        }
            
        let path = PathBuf::from(format!("/dev/mapper/{}", name));
        Ok(DmDevice::new(name.to_string(), path))
    }
    
    pub fn remove_snapshot(&self, name: &str) -> Result<()> {
        info!("Removing snapshot {}", name);
        let origin_name_str = format!("{}-origin", name);
        
        // Remove snap first
        let dm_name = DmName::new(name).map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        self.dm.device_remove(&DevId::Name(dm_name), DmOptions::default())
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            
        // Remove origin
        if let Ok(origin_dm_name) = DmName::new(&origin_name_str) {
            let _ = self.dm.device_remove(&DevId::Name(origin_dm_name), DmOptions::default());
        }

        Ok(())
    }
    
    pub fn list_devices(&self) -> Result<Vec<DmDevice>> {
        info!("Listing device mapper devices");
        
        let devs = self.dm.list_devices().map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let mut dm_devices = Vec::new();
        // Since iterator output in older devicemapper is 2-tuple instead of 3
        for item in devs {
            let dev_name = item.0;
            let path = PathBuf::from(format!("/dev/mapper/{}", dev_name.to_string()));
            dm_devices.push(DmDevice::new(dev_name.to_string(), path));
        }
        
        Ok(dm_devices)
    }
    
    pub fn device_exists(&self, name: &str) -> Result<bool> {
        let dm_name = DmName::new(name).map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        match self.dm.device_info(&DevId::Name(dm_name)) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

