use anyhow::{Result, anyhow};
use std::process::{Command, Child};
use std::path::Path;
use tracing::{info, error};

#[derive(Debug)]
pub struct SlirpManager {
    process: Option<Child>,
    socket_path: String,
}

impl SlirpManager {
    pub fn new(socket_path: &str) -> Self {
        Self {
            process: None,
            socket_path: socket_path.to_string(),
        }
    }

    /// Checks if slirp4netns is installed.
    pub fn check_available() -> Result<()> {
        let status = Command::new("slirp4netns").arg("--version").output();
        match status {
            Ok(o) if o.status.success() => Ok(()),
            _ => Err(anyhow!("slirp4netns not found. Please install it for rootless networking.")),
        }
    }

    /// Spawns slirp4netns attached to the target PID.
    /// Creates interface `tapName` (default tap0) inside the netns.
    pub fn spawn(&mut self, target_pid: u32, interface_name: &str) -> Result<()> {
        info!("Starting slirp4netns for PID {}", target_pid);
        
        let child = Command::new("slirp4netns")
            .arg("--configure")
            .arg("--mtu=65520")
            .arg("--disable-host-loopback")
            .arg("--api-socket").arg(&self.socket_path)
            .arg(target_pid.to_string())
            .arg(interface_name)
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn slirp4netns: {}", e))?;
            
        self.process = Some(child);
        Ok(())
    }
    
    pub fn kill(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for SlirpManager {
    fn drop(&mut self) {
        self.kill();
    }
}
