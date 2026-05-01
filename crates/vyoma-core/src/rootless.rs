use std::path::Path;
use anyhow::{Result, anyhow};
use std::fs::OpenOptions;

pub struct RootlessManager;

impl RootlessManager {
    /// Checks if the current process is running as root (uid 0).
    pub fn is_root() -> bool {
        unsafe { libc::getuid() == 0 }
    }

    /// Checks if the current user has read/write access to /dev/kvm.
    pub fn check_kvm_permissions() -> Result<()> {
        let path = Path::new("/dev/kvm");
        if !path.exists() {
            return Err(anyhow!("KVM device /dev/kvm not found. Is KVM installed?"));
        }
        
        // Try to open RW
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Cannot access /dev/kvm: {}. ensure user is in 'kvm' group.", e)),
        }
    }
}
