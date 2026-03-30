use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{info, warn, error};
use serde::{Deserialize, Serialize};

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub listen_port: u16,
    pub interface_name: String,
    pub mtu: Option<u16>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            listen_port: 51820,
            interface_name: "ignite-wg0".to_string(),
            mtu: Some(1420),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub public_key: String,
    pub endpoint: SocketAddr,
    pub allowed_ips: Vec<String>,
    pub keepalive: Option<u16>,
}

impl PeerConfig {
    pub fn new(public_key: String, endpoint: SocketAddr) -> Self {
        Self {
            public_key,
            endpoint,
            allowed_ips: vec!["0.0.0.0/0".to_string()],
            keepalive: Some(25),
        }
    }
    
    pub fn with_allowed_ips(mut self, ips: Vec<String>) -> Self {
        self.allowed_ips = ips;
        self
    }
}

pub struct WireGuardNode {
    config: WireGuardConfig,
    peers: Vec<PeerConfig>,
    public_key: Option<String>,
    private_key_path: Option<PathBuf>,
    running: bool,
}

impl WireGuardNode {
    pub fn new(config: WireGuardConfig) -> Result<Self> {
        info!("Creating WireGuard node on port {}", config.listen_port);
        
        Ok(Self {
            config,
            peers: Vec::new(),
            public_key: None,
            private_key_path: None,
            running: false,
        })
    }
    
    pub fn from_key(key_path: PathBuf, config: WireGuardConfig) -> Result<Self> {
        info!("Loading WireGuard key from {:?}", key_path);
        
        let private_key = if key_path.exists() {
            std::fs::read_to_string(&key_path)
                .map_err(|e| NetworkError::Io(e))?
                .trim()
                .to_string()
        } else {
            let key = generate_wireguard_key()?;
            std::fs::write(&key_path, &key)
                .map_err(|e| NetworkError::Io(e))?;
            key
        };
        
        Ok(Self {
            config,
            peers: Vec::new(),
            public_key: Some(private_key),
            private_key_path: Some(key_path),
            running: false,
        })
    }
    
    pub fn public_key_base64(&self) -> String {
        self.public_key.clone().unwrap_or_default()
    }
    
    pub fn add_peer(&mut self, peer: PeerConfig) -> Result<()> {
        info!("Adding peer {} with endpoint {}", peer.public_key, peer.endpoint);
        
        if self.running {
            let allowed_ips = peer.allowed_ips.join(",");
            
            let output = std::process::Command::new("wg")
                .args(&["set", &self.config.interface_name, "peer", &peer.public_key, 
                    "endpoint", &peer.endpoint.to_string(),
                    "allowed-ips", &allowed_ips])
                .output()
                .map_err(|e| NetworkError::Io(e))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NetworkError::Netlink(format!("Failed to add peer: {}", stderr)));
            }
        }
        
        self.peers.push(peer);
        Ok(())
    }
    
    pub fn remove_peer(&mut self, public_key: &str) -> Result<()> {
        info!("Removing peer {}", public_key);
        
        if self.running {
            let output = std::process::Command::new("wg")
                .args(&["set", &self.config.interface_name, "peer", public_key, "remove"])
                .output()
                .map_err(|e| NetworkError::Io(e))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NetworkError::Netlink(format!("Failed to remove peer: {}", stderr)));
            }
        }
        
        self.peers.retain(|p| p.public_key != public_key);
        Ok(())
    }
    
    pub fn list_peers(&self) -> &[PeerConfig] {
        &self.peers
    }
    
    pub fn start(&mut self) -> Result<()> {
        info!("Starting WireGuard node on {}", self.config.interface_name);
        
        // Create WireGuard interface using ip command
        let output = std::process::Command::new("ip")
            .args(&["link", "add", &self.config.interface_name, "type", "wireguard"])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("File exists") {
                return Err(NetworkError::Netlink(format!("Failed to create interface: {}", stderr)));
            }
        }
        
        // Set listen port
        if let Some(ref key) = self.public_key {
            let output = std::process::Command::new("wg")
                .args(&["set", &self.config.interface_name, "private-key", "-"])
                .output()
                .map_err(|e| NetworkError::Io(e))?;
            
            // Note: In production, we'd use stdin to pass the key
        }
        
        // Set IP address
        let _ = std::process::Command::new("ip")
            .args(&["addr", "add", "10.0.0.1/24", "dev", &self.config.interface_name])
            .output();
        
        // Set MTU
        if let Some(mtu) = self.config.mtu {
            let _ = std::process::Command::new("ip")
                .args(&["link", "set", "mtu", &mtu.to_string(), "dev", &self.config.interface_name])
                .output();
        }
        
        // Bring up
        let output = std::process::Command::new("ip")
            .args(&["link", "set", "up", "dev", &self.config.interface_name])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            return Err(NetworkError::Netlink("Failed to bring up interface".to_string()));
        }
        
        // Add existing peers
        for peer in &self.peers {
            let allowed_ips = peer.allowed_ips.join(",");
            let _ = std::process::Command::new("wg")
                .args(&["set", &self.config.interface_name, "peer", &peer.public_key, 
                    "endpoint", &peer.endpoint.to_string(),
                    "allowed-ips", &allowed_ips])
                .output();
        }
        
        self.running = true;
        info!("WireGuard node started successfully");
        Ok(())
    }
    
    pub fn stop(&mut self) -> Result<()> {
        info!("Stopping WireGuard node");
        
        // Bring down interface
        let _ = std::process::Command::new("ip")
            .args(&["link", "set", "down", "dev", &self.config.interface_name])
            .output();
        
        // Delete interface
        let _ = std::process::Command::new("ip")
            .args(&["link", "del", &self.config.interface_name])
            .output();
        
        self.running = false;
        info!("WireGuard node stopped");
        Ok(())
    }
    
    pub fn is_running(&self) -> bool {
        self.running
    }
}

fn generate_wireguard_key() -> Result<String> {
    use std::process::Command;
    
    let output = Command::new("wg")
        .arg("genkey")
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        return Err(NetworkError::Netlink("Failed to generate wireguard key".to_string()));
    }
    
    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(key)
}

pub fn get_public_key(private_key: &str) -> Result<String> {
    use std::process::Command;
    
    let output = Command::new("wg")
        .arg("pubkey")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        return Err(NetworkError::Netlink("Failed to derive public key".to_string()));
    }
    
    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wireguard_config_default() {
        let config = WireGuardConfig::default();
        assert_eq!(config.listen_port, 51820);
        assert_eq!(config.interface_name, "ignite-wg0");
    }
    
    #[test]
    fn test_peer_config_builder() {
        let peer = PeerConfig::new(
            "test_public_key".to_string(),
            "192.168.1.1:51820".parse().unwrap()
        ).with_allowed_ips(vec!["10.0.0.0/24".to_string()]);
        
        assert_eq!(peer.keepalive, Some(25));
        assert_eq!(peer.allowed_ips.len(), 1);
    }
    
    #[test]
    fn test_wireguard_node_creation() {
        let config = WireGuardConfig::default();
        let node = WireGuardNode::new(config).unwrap();
        
        assert!(!node.is_running());
        assert!(node.list_peers().is_empty());
    }
    
    #[test]
    fn test_peer_list_management() {
        let mut node = WireGuardNode::new(WireGuardConfig::default()).unwrap();
        
        let peer1 = PeerConfig::new("key1".to_string(), "1.1.1.1:51820".parse().unwrap());
        let peer2 = PeerConfig::new("key2".to_string(), "2.2.2.2:51820".parse().unwrap());
        
        node.add_peer(peer1).unwrap();
        node.add_peer(peer2).unwrap();
        
        assert_eq!(node.list_peers().len(), 2);
        
        node.remove_peer("key1").unwrap();
        assert_eq!(node.list_peers().len(), 1);
    }
}
