use anyhow::{Result, anyhow};
use std::process::Command;

pub struct SlirpManager;

impl SlirpManager {
    /// Checks if slirp4netns is installed and available.
    pub fn check_available() -> Result<()> {
        let status = Command::new("slirp4netns").arg("--version").output();
        match status {
            Ok(o) if o.status.success() => Ok(()),
            _ => Err(anyhow!("slirp4netns not found. Please install it for rootless networking.")),
        }
    }
}
