use std::path::{Path, PathBuf};
use tracing::info;

// Add devicemapper imports
use devicemapper::{DM, DmName, DmOptions, DmUuid, DevId};
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

        // 1. Create origin linear device via devicemapper API
        let origin_name = DmName::new(&origin_name_str)
            .map_err(|e| StorageError::Other(format!("Invalid DM name {}: {}", origin_name_str, e)))?;
        let origin_uuid_str = uuid::Uuid::new_v4().to_string();
        let origin_uuid = DmUuid::new(&origin_uuid_str)
            .map_err(|e| StorageError::Other(format!("Invalid UUID for origin: {}", e)))?;

        let origin_table = vec![(
            0u64,
            sectors,
            "linear".to_string(),
            format!("{} 0", base_dev.display()),
        )];

        self.dm
            .device_create(&origin_name, Some(&origin_uuid), DmOptions::default())
            .map_err(|e| StorageError::Other(format!("Failed to create origin device via dm API: {}", e)))?;
        self.dm
            .table_load(&DevId::Name(&origin_name), &origin_table, DmOptions::default())
            .map_err(|e| StorageError::Other(format!("Failed to load origin table via dm API: {}", e)))?;

        // 2. Create snapshot device via devicemapper API
        let snap_name = DmName::new(name)
            .map_err(|e| StorageError::Other(format!("Invalid DM name {}: {}", name, e)))?;
        let snap_uuid_str = uuid::Uuid::new_v4().to_string();
        let snap_uuid = DmUuid::new(&snap_uuid_str)
            .map_err(|e| StorageError::Other(format!("Invalid UUID for snapshot: {}", e)))?;

        let origin_dev_path = format!("/dev/mapper/{}", origin_name_str);
        let snap_table = vec![(
            0u64,
            sectors,
            "snapshot".to_string(),
            format!("{} {} P 8", origin_dev_path, cow_dev.display()),
        )];

        self.dm
            .device_create(&snap_name, Some(&snap_uuid), DmOptions::default())
            .map_err(|e| StorageError::Other(format!("Failed to create snapshot device via dm API: {}", e)))?;
        self.dm
            .table_load(&DevId::Name(&snap_name), &snap_table, DmOptions::default())
            .map_err(|e| StorageError::Other(format!("Failed to load snapshot table via dm API: {}", e)))?;

        self.activate_device(&origin_name)?;
        self.activate_device(&snap_name)?;

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

    fn activate_device(&self, name: &DmName) -> Result<()> {
        let _ = self
            .dm
            .device_suspend(&DevId::Name(name), DmOptions::default())
            .map_err(|e| StorageError::Other(format!("Failed to activate device {}: {}", name, e)))?;
        Ok(())
    }
}

