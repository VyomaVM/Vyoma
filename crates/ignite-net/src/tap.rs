use tracing::{info, warn, error};
use rtnetlink::{new_connection, Handle};
use netlink_packet_route::link::State;
use netlink_packet_route::link::LinkAttribute;
use futures::stream::TryStreamExt;
use std::process::Command;

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone)]
pub struct TapInfo {
    pub name: String,
    pub index: u32,
    pub state: String,
}

pub struct TapManager {
    handle: Handle,
}

impl TapManager {
    pub async fn new() -> Result<Self> {
        info!("Initializing native TAP manager via rtnetlink");
        let (connection, handle, _) = new_connection().map_err(|e| NetworkError::Io(e))?;
        tokio::spawn(connection);
        
        Ok(Self { handle })
    }
    
    pub async fn create_tap(&self, name: &str) -> Result<String> {
        info!("Creating TAP device: {}", name);
        
        if name.is_empty() {
            return Err(NetworkError::InvalidInput("TAP name cannot be empty".to_string()));
        }
        
        // Use ip tuntap natively-ish via fallback, because rust rtnetlink does not fully map the tuntap TUNSETIFF ioctls cleanly yet without nix/libc crates.
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
    
    pub async fn delete_tap(&self, name: &str) -> Result<()> {
        info!("Deleting native TAP device: {}", name);
        
        match self.get_interface_index(name).await {
            Ok(index) => {
                if let Err(e) = self.handle.link().del(index).execute().await {
                    return Err(NetworkError::Netlink(e.to_string()));
                }
            }
            Err(_) => return Ok(()), // Already deleted
        }
        
        Ok(())
    }
    
    pub async fn set_up(&self, name: &str) -> Result<()> {
        info!("Setting TAP {} up natively", name);
        
        let index = self.get_interface_index(name).await?;
        if let Err(e) = self.handle.link().set(index).up().execute().await {
            return Err(NetworkError::Netlink(format!("Failed to set TAP up: {}", e)));
        }
        
        Ok(())
    }
    
    pub async fn get_info(&self, name: &str) -> Result<TapInfo> {
        info!("Getting TAP info natively: {}", name);
        
        let mut links = self.handle.link().get().match_name(name.to_string()).execute();
        
        if let Ok(Some(link)) = links.try_next().await {
            let index = link.header.index;
            let mut state = "unknown".to_string();
            
            for nla in link.attributes.into_iter() {
                if let LinkAttribute::OperState(s) = nla {
                    state = match s {
                        State::Up => "up".to_string(),
                        State::Down => "down".to_string(),
                        _ => "unknown".to_string(),
                    };
                }
            }
            
            return Ok(TapInfo { name: name.to_string(), index, state });
        }
        
        Err(NetworkError::NotFound(format!("TAP {} not found", name)))
    }
    
    pub async fn list_taps(&self) -> Result<Vec<TapInfo>> {
        info!("Listing TAP devices natively");
        
        let mut links = self.handle.link().get().execute();
        let mut taps = Vec::new();
        
        while let Ok(Some(link)) = links.try_next().await {
            let index = link.header.index;
            let mut name = String::new();
            let mut state = "unknown".to_string();
            
            for nla in link.attributes.into_iter() {
                match nla {
                    LinkAttribute::IfName(n) => name = n,
                    LinkAttribute::OperState(s) => {
                        state = match s {
                            State::Up => "up".to_string(),
                            State::Down => "down".to_string(),
                            _ => "unknown".to_string(),
                        };
                    }
                    _ => {}
                }
            }
            
            if name.starts_with("tap") {
                taps.push(TapInfo {
                    name,
                    index,
                    state,
                });
            }
        }
        
        Ok(taps)
    }
    
    async fn get_interface_index(&self, name: &str) -> Result<u32> {
        let mut links = self.handle.link().get().match_name(name.to_string()).execute();
        
        if let Ok(Some(link)) = links.try_next().await {
            return Ok(link.header.index);
        }
        
        Err(NetworkError::NotFound(format!("Interface {} not found", name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_tap_manager_creation() {
        let tm = TapManager::new().await.unwrap();
        let taps = tm.list_taps().await.unwrap();
        println!("Found {} TAP devices natively", taps.len());
    }
}
