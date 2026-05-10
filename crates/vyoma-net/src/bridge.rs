use tracing::info;
use rtnetlink::{new_connection, Handle, Error as RtNetlinkError};
use netlink_packet_route::link::State;
use netlink_packet_route::link::LinkAttribute;
use netlink_packet_route::link::LinkInfo;
use netlink_packet_route::link::InfoKind;
use futures::stream::TryStreamExt;
use std::process::Command;

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone)]
pub struct BridgeInfo {
    pub name: String,
    pub index: u32,
    pub state: String,
    pub mac_address: Option<String>,
}

pub struct BridgeManager {
    handle: Handle,
}

impl BridgeManager {
    pub async fn new() -> Result<Self> {
        info!("Initializing native Bridge manager via rtnetlink");
        let (connection, handle, _) = new_connection().map_err(|e| NetworkError::Io(e))?;
        tokio::spawn(connection);
        
        Ok(Self { handle })
    }
    
    pub async fn create_bridge(&self, name: &str) -> Result<u32> {
        info!("Creating native bridge: {}", name);
        
        if name.is_empty() {
            return Err(NetworkError::InvalidInput("Bridge name cannot be empty".to_string()));
        }
        
        let req = self.handle.link().add().bridge(name.to_string());
        if let Err(e) = req.execute().await {
            match e {
                RtNetlinkError::NetlinkError(ref msg) if msg.code.map_or(0, |c| c.get()) == -17 => { // EEXIST
                    return Err(NetworkError::AlreadyExists(format!("Bridge {} already exists", name)));
                }
                _ => return Err(NetworkError::Netlink(e.to_string())),
            }
        }
        
        let index = self.get_interface_index(name).await?;
        info!("Bridge {} created natively with index {}", name, index);
        Ok(index)
    }
    
    pub async fn delete_bridge(&self, name: &str) -> Result<()> {
        info!("Deleting native bridge: {}", name);
        
        let index = self.get_interface_index(name).await?;
        if let Err(e) = self.handle.link().del(index).execute().await {
            return Err(NetworkError::Netlink(e.to_string()));
        }
        
        Ok(())
    }
    
    pub async fn set_up(&self, name: &str) -> Result<()> {
        info!("Setting bridge {} up natively", name);
        let index = self.get_interface_index(name).await?;
        
        if let Err(e) = self.handle.link().set(index).up().execute().await {
            return Err(NetworkError::Netlink(format!("Failed to set bridge up: {}", e)));
        }
        
        Ok(())
    }
    
    pub async fn set_ip(&self, name: &str, ip_cidr: &str) -> Result<()> {
        info!("Setting IP {} on bridge {}", ip_cidr, name);
        
        let index = self.get_interface_index(name).await?;
        
        let output = Command::new("ip")
            .args(&["addr", "add", ip_cidr, "dev", name])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("File exists") && !stderr.contains("17") {
                return Err(NetworkError::Netlink(format!("Failed to set IP: {}", stderr)));
            }
        }
        
        Ok(())
    }
    
    pub async fn add_tap_to_bridge(&self, tap_name: &str, bridge_name: &str) -> Result<()> {
        info!("Adding {} to bridge {} natively", tap_name, bridge_name);
        
        let tap_index = self.get_interface_index(tap_name).await?;
        let bridge_index = self.get_interface_index(bridge_name).await?;
        
        if let Err(e) = self.handle.link().set(tap_index).controller(bridge_index).execute().await {
            return Err(NetworkError::Netlink(e.to_string()));
        }
        
        Ok(())
    }
    
    pub async fn list_bridges(&self) -> Result<Vec<BridgeInfo>> {
        info!("Listing bridges natively");
        
        let mut links = self.handle.link().get().execute();
        let mut bridges = Vec::new();
        
        while let Ok(Some(link)) = links.try_next().await {
            let index = link.header.index;
            let mut name = String::new();
            let mut state = "unknown".to_string();
            let mut is_bridge = false;
            let mut mac = None;
            
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
                    LinkAttribute::Address(addr) => {
                        mac = Some(addr.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(":"));
                    }
                    LinkAttribute::LinkInfo(infos) => {
                        for info in infos {
                            if let LinkInfo::Kind(kind) = info {
                                if let InfoKind::Bridge = kind {
                                    is_bridge = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if is_bridge {
                bridges.push(BridgeInfo {
                    name,
                    index,
                    state,
                    mac_address: mac,
                });
            }
        }
        
        Ok(bridges)
    }
    
    pub async fn get_interface_index(&self, name: &str) -> Result<u32> {
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
    async fn test_bridge_manager_creation() {
        let bm = BridgeManager::new().await.unwrap();
        let bridges = bm.list_bridges().await.unwrap();
        println!("Found {} bridges natively", bridges.len());
    }
}
