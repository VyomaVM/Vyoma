use std::process::Command;
use tracing::{info, warn};

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone)]
pub struct TapInfo {
    pub name: String,
    pub index: u32,
    pub state: String,
}

pub struct TapManager {
    // In production, this would hold the rtnetlink Handle
    _phantom: std::marker::PhantomData<()>,
}

impl TapManager {
    pub fn new() -> Result<Self> {
        info!("Initializing TAP manager");
        Ok(Self {
            _phantom: std::marker::PhantomData,
        })
    }
    
    /// Create a TAP device
    pub async fn create_tap(&self, name: &str) -> Result<String> {
        info!("Creating TAP device: {}", name);
        
        // Validate name
        if name.is_empty() {
            return Err(NetworkError::InvalidInput("TAP name cannot be empty".to_string()));
        }
        
        // Use ip command to create TAP
        let output = Command::new("ip")
            .args(&["tuntap", "add", name, "mode", "tap"])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("File exists") {
                return Ok(name.to_string()); // Already exists, that's fine
            }
            return Err(NetworkError::Netlink(stderr.to_string()));
        }
        
        Ok(name.to_string())
    }
    
    /// Delete a TAP device
    pub async fn delete_tap(&self, name: &str) -> Result<()> {
        info!("Deleting TAP device: {}", name);
        
        let output = Command::new("ip")
            .args(&["tuntap", "del", name, "mode", "tap"])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such device") {
                return Ok(()); // Already deleted
            }
            return Err(NetworkError::Netlink(stderr.to_string()));
        }
        
        Ok(())
    }
    
    /// Set TAP up
    pub async fn set_up(&self, name: &str) -> Result<()> {
        info!("Setting TAP {} up", name);
        
        let output = Command::new("ip")
            .args(&["link", "set", name, "up"])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            return Err(NetworkError::Netlink("Failed to set TAP up".to_string()));
        }
        
        Ok(())
    }
    
    /// Get TAP interface info
    pub async fn get_info(&self, name: &str) -> Result<TapInfo> {
        info!("Getting TAP info: {}", name);
        
        let output = Command::new("ip")
            .args(&["link", "show", name])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            return Err(NetworkError::NotFound(format!("TAP {} not found", name)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut index = 0u32;
        let mut state = "unknown".to_string();
        
        for line in stdout.lines() {
            if line.contains(":") && !line.starts_with(" ") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 1 {
                    index = parts[0].trim().parse().unwrap_or(0);
                }
            }
            if line.contains("state") {
                if let Some(s) = line.split("state").nth(1) {
                    state = s.trim().split_whitespace().next().unwrap_or("unknown").to_string();
                }
            }
        }
        
        Ok(TapInfo { name: name.to_string(), index, state })
    }
    
    /// List all TAP devices
    pub async fn list_taps(&self) -> Result<Vec<TapInfo>> {
        info!("Listing TAP devices");
        
        let output = Command::new("ip")
            .args(&["link", "show"])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut taps = Vec::new();
        
        for line in stdout.lines() {
            if line.contains(":") && !line.starts_with(" ") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    let name = parts[1].trim().to_string();
                    if name.starts_with("tap") {
                        taps.push(TapInfo {
                            name,
                            index: 0,
                            state: "unknown".to_string(),
                        });
                    }
                }
            }
        }
        
        Ok(taps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tap_manager_creation() {
        let tm = TapManager::new().unwrap();
        let taps = tm.list_taps().await.unwrap();
        println!("Found {} TAP devices", taps.len());
    }
}
