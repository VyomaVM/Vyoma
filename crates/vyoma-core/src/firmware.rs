use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub const DEFAULT_OVMF_PATH: &str = "/var/lib/vyoma/firmware/OVMF_CODE.fd";
pub const DEFAULT_UEFI_VARS_TEMPLATE: &str = "/var/lib/vyoma/firmware/ovmf_vars.fd";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirmwareConfig {
    pub firmware_path: String,
    pub uefi_vars_template: Option<String>,
    pub secure_boot: bool,
    pub enforce_secure_boot: bool,
}

impl Default for FirmwareConfig {
    fn default() -> Self {
        Self {
            firmware_path: DEFAULT_OVMF_PATH.to_string(),
            uefi_vars_template: Some(DEFAULT_UEFI_VARS_TEMPLATE.to_string()),
            secure_boot: true,
            enforce_secure_boot: false,
        }
    }
}

pub struct FirmwareManager {
    data_dir: PathBuf,
}

impl FirmwareManager {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn firmware_dir(&self) -> PathBuf {
        self.data_dir.join("firmware")
    }

    pub fn ensure_firmware_dirs(&self) -> Result<()> {
        let firmware_dir = self.firmware_dir();
        std::fs::create_dir_all(&firmware_dir)?;
        info!("Firmware directory: {:?}", firmware_dir);
        Ok(())
    }

    pub fn get_firmware_path(&self, secure_boot: bool) -> PathBuf {
        if secure_boot {
            self.firmware_dir().join("OVMF_CODE.secboot.fd")
        } else {
            self.firmware_dir().join("OVMF_CODE.fd")
        }
    }

    pub fn get_uefi_vars_path(&self, vm_id: &str) -> PathBuf {
        self.data_dir.join("vms").join(vm_id).join("uefi_vars.fd")
    }

    pub fn copy_uefi_vars_template(&self, vm_id: &str) -> Result<PathBuf> {
        let template_path = PathBuf::from(DEFAULT_UEFI_VARS_TEMPLATE);
        let target_path = self.get_uefi_vars_path(vm_id);

        if template_path.exists() {
            std::fs::copy(&template_path, &target_path)?;
            info!("Copied UEFI vars template to {:?}", target_path);
        } else {
            warn!("UEFI vars template not found at {:?}, creating empty", template_path);
        }

        Ok(target_path)
    }

    pub fn is_firmware_available(&self, secure_boot: bool) -> bool {
        self.get_firmware_path(secure_boot).exists()
    }

    pub fn check_firmware(&self, secure_boot: bool) -> Result<FirmwareStatus> {
        let firmware_path = self.get_firmware_path(secure_boot);

        if !firmware_path.exists() {
            return Ok(FirmwareStatus::NotFound {
                path: firmware_path,
                message: "OVMF firmware not found. Please install edk2-ovmf package".to_string(),
            });
        }

        let metadata = std::fs::metadata(&firmware_path)?;
        Ok(FirmwareStatus::Available {
            path: firmware_path,
            size: metadata.len(),
            secure_boot,
        })
    }

    pub fn get_default_config(&self, secure_boot: bool) -> FirmwareConfig {
        FirmwareConfig {
            firmware_path: self.get_firmware_path(secure_boot).to_string_lossy().to_string(),
            uefi_vars_template: Some(DEFAULT_UEFI_VARS_TEMPLATE.to_string()),
            secure_boot,
            enforce_secure_boot: false,
        }
    }
}

#[derive(Debug)]
pub enum FirmwareStatus {
    Available {
        path: PathBuf,
        size: u64,
        secure_boot: bool,
    },
    NotFound {
        path: PathBuf,
        message: String,
    },
}

pub fn find_ovmf_binary() -> Option<PathBuf> {
    let possible_paths = vec![
        "/usr/share/ovmf/x64/OVMF_CODE.fd",
        "/usr/share/ovmf/x64/OVMF_CODE.secboot.fd",
        "/usr/share/edk2/ovmf/OVMF_CODE.fd",
        "/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd",
        "/usr/share/qemu/ovmf-x86_64-code.bin",
        "/usr/share/qemu/ovmf-x86_64.bin",
    ];

    for path in &possible_paths {
        if Path::new(path).exists() {
            return Some(PathBuf::from(path));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_config_default() {
        let config = FirmwareConfig::default();
        assert_eq!(config.secure_boot, true);
    }

    #[test]
    fn test_firmware_manager_path() {
        let manager = FirmwareManager::new(Path::new("/tmp/test"));
        assert_eq!(manager.firmware_dir(), PathBuf::from("/tmp/test/firmware"));
    }

    #[test]
    fn test_get_uefi_vars_path() {
        let manager = FirmwareManager::new(Path::new("/tmp/test"));
        let path = manager.get_uefi_vars_path("test-vm");
        assert_eq!(path, PathBuf::from("/tmp/test/vms/test-vm/uefi_vars.fd"));
    }
}