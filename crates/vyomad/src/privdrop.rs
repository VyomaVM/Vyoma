use anyhow::{Context, Result};
use caps::{Capability, CapSet, CapsHashSet};
use libc::{getpwnam, passwd, setgid, setuid, gid_t, uid_t};
use std::collections::HashSet;
use tracing::{info, error};

const TARGET_USER: &str = "vyoma";

#[derive(Debug, thiserror::Error)]
pub enum PrivDropError {
    #[error("User '{0}' not found. Please ensure the vyoma user exists.")]
    UserNotFound(String),
    #[error("Failed to get user info: {0}")]
    UserInfoError(String),
    #[error("Failed to set capabilities: {0}")]
    CapabilityError(String),
    #[error("Failed to set groups: {0}")]
    SetgroupsError(String),
    #[error("Failed to setgid: {0}")]
    SetgidError(String),
    #[error("Failed to setuid: {0}")]
    SetuidError(String),
    #[error("Privilege drop verification failed: still running as root")]
    VerificationFailed,
}

fn get_pwentry(username: &str) -> Result<*const passwd> {
    let c_str = std::ffi::CString::new(username)
        .context("Failed to create C string for username")?;

    let pw = unsafe { getpwnam(c_str.as_ptr()) };

    if pw.is_null() {
        return Err(PrivDropError::UserNotFound(username.to_string()).into());
    }

    Ok(pw)
}

pub fn drop_privileges() -> Result<()> {
    let pw = get_pwentry(TARGET_USER)
        .context("Failed to resolve vyoma user")?;

    let vyoma_uid: uid_t = unsafe { (*pw).pw_uid };
    let vyoma_gid: gid_t = unsafe { (*pw).pw_gid };

    info!("Resolved vyoma user: uid={}, gid={}", vyoma_uid, vyoma_gid);

    let allowed_caps: CapsHashSet = [
        Capability::CAP_SYS_ADMIN,
        Capability::CAP_NET_ADMIN,
        Capability::CAP_NET_RAW,
        Capability::CAP_SYS_PTRACE,
    ].iter().cloned().collect();

    info!("Setting bounding set capabilities...");
    caps::set(None, CapSet::Bounding, &allowed_caps)
        .map_err(|e| PrivDropError::CapabilityError(format!("Bounding set failed: {:?}", e)))?;

    info!("Setting ambient capabilities...");
    caps::set(None, CapSet::Ambient, &allowed_caps)
        .map_err(|e| PrivDropError::CapabilityError(format!("Ambient set failed: {:?}", e)))?;

    info!("Setting inheritable capabilities...");
    caps::set(None, CapSet::Inheritable, &allowed_caps)
        .map_err(|e| PrivDropError::CapabilityError(format!("Inheritable set failed: {:?}", e)))?;

    info!("Clearing supplementary groups...");
    unsafe {
        let result = libc::setgroups(0, std::ptr::null());
        if result != 0 {
            return Err(PrivDropError::SetgroupsError(
                std::io::Error::last_os_error().to_string()
            ).into());
        }
    }

    info!("Setting group to vyoma ({})...", vyoma_gid);
    unsafe {
        if setgid(vyoma_gid) != 0 {
            return Err(PrivDropError::SetgidError(
                std::io::Error::last_os_error().to_string()
            ).into());
        }
    }

    info!("Setting user to vyoma ({})...", vyoma_uid);
    unsafe {
        if setuid(vyoma_uid) != 0 {
            return Err(PrivDropError::SetuidError(
                std::io::Error::last_os_error().to_string()
            ).into());
        }
    }

    let current_uid = unsafe { libc::geteuid() };
    if current_uid == 0 {
        error!("Privilege drop verification failed: still running as root (uid=0)");
        return Err(PrivDropError::VerificationFailed.into());
    }

    info!(
        "Privilege drop successful: now running as uid={}, euid={}",
        current_uid,
        unsafe { libc::geteuid() }
    );

    Ok(())
}