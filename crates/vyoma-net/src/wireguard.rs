use std::net::{Ipv4Addr, IpAddr, SocketAddr};
use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn};
use serde::{Deserialize, Serialize};

use boringtun::device::{DeviceConfig, DeviceHandle};
use x25519_dalek::{StaticSecret, PublicKey};
use ipnetwork::IpNetwork;
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose};
use rtnetlink::{new_connection, Handle};

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub listen_port: u16,
    pub interface_name: String,
    pub mtu: Option<u16>,
    pub node_ip: Option<Ipv4Addr>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            listen_port: 51820,
            interface_name: "vyoma-wg0".to_string(),
            mtu: Some(1420),
            node_ip: None,
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
    pub config: WireGuardConfig,
    secret_key: StaticSecret,
    public_key: PublicKey,
    handle: Option<DeviceHandle>,
    rt_handle: Option<Handle>,
    peers: Vec<PeerConfig>,
    running: bool,
    interface_index: Option<u32>,
}

impl WireGuardNode {
    pub fn new(config: WireGuardConfig) -> Result<Self> {
        info!("Creating WireGuard node on port {}", config.listen_port);
        let secret_key = StaticSecret::random_from_rng(OsRng);
        let public_key = PublicKey::from(&secret_key);
        
        Ok(Self {
            config,
            secret_key,
            public_key,
            handle: None,
            rt_handle: None,
            peers: Vec::new(),
            running: false,
            interface_index: None,
        })
    }
    
    pub fn from_key(key_path: PathBuf, config: WireGuardConfig) -> Result<Self> {
        info!("Loading/Saving WireGuard key from/to {:?}", key_path);
        
        let secret_key = if key_path.exists() {
            let key_str = std::fs::read_to_string(&key_path)
                .map_err(|e| NetworkError::Io(e))?
                .trim()
                .to_string();
            
            let bytes = general_purpose::STANDARD.decode(&key_str)
                .map_err(|_| NetworkError::Netlink("Invalid base64 key".to_string()))?;
            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&bytes[..32]);
            StaticSecret::from(key_bytes)
        } else {
            let sk = StaticSecret::random_from_rng(OsRng);
            let encoded = general_purpose::STANDARD.encode(sk.to_bytes());
            std::fs::write(&key_path, encoded)
                .map_err(|e| NetworkError::Io(e))?;
            sk
        };
        
        let public_key = PublicKey::from(&secret_key);
        
        Ok(Self {
            config,
            secret_key,
            public_key,
            handle: None,
            rt_handle: None,
            peers: Vec::new(),
            running: false,
            interface_index: None,
        })
    }
    
    pub fn public_key_base64(&self) -> String {
        general_purpose::STANDARD.encode(self.public_key.as_bytes())
    }
    
    pub fn start(&mut self) -> Result<()> {
        info!("Starting WireGuard node natively via boringtun...");
        
        let (conn, handle, _) = new_connection()
            .map_err(|e| NetworkError::Netlink(format!("Failed to create rtnetlink connection: {}", e)))?;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(conn);
        });
        self.rt_handle = Some(handle.clone());
        
        let dev_config = DeviceConfig {
            n_threads: 2,
            ..Default::default()
        };
        
        let handle = DeviceHandle::new(&self.config.interface_name, dev_config)
            .map_err(|e| NetworkError::Netlink(format!("Boringtun Device creation failed: {}", e)))?;
        
        self.handle = Some(handle);
        
        let rt_handle = self.rt_handle.as_ref()
            .ok_or_else(|| NetworkError::Netlink("rtnetlink handle not initialized".to_string()))?;
        
        let if_index = rtnetlink_get_interface_index(&self.config.interface_name)
            .map_err(|e| NetworkError::Netlink(format!("Failed to get interface index: {}", e)))?;
        self.interface_index = Some(if_index);
        info!("WireGuard interface {} has index {}", self.config.interface_name, if_index);
        
        if let Some(node_ip) = self.config.node_ip {
            let ip_cidr = IpNetwork::new(IpAddr::V4(ip_octets_to_ip(node_ip, 0)), 24)
                .unwrap_or_else(|_| IpNetwork::new(IpAddr::V4(Ipv4Addr::new(10, 42, 0, 1)), 24).unwrap());
            async_set_interface_ip(rt_handle, if_index, ip_cidr)
                .map_err(|e| NetworkError::Netlink(format!("Failed to set IP: {}", e)))?;
            info!("Set IP {} on WireGuard interface", ip_cidr);
        }
        
        async_set_interface_up(rt_handle, if_index)
            .map_err(|e| NetworkError::Netlink(format!("Failed to bring interface up: {}", e)))?;
        info!("Brought WireGuard interface up");
        
        let private_key_bytes = self.secret_key.to_bytes();
        let private_key_hex = hex::encode(private_key_bytes);
        set_wireguard_private_key(&self.config.interface_name, &private_key_hex)
            .map_err(|e| NetworkError::Netlink(format!("Failed to set private key: {}", e)))?;
        
        if self.config.listen_port > 0 {
            set_wireguard_listen_port(&self.config.interface_name, self.config.listen_port)
                .map_err(|e| NetworkError::Netlink(format!("Failed to set listen port: {}", e)))?;
        }
        
        self.running = true;
        
        for peer in self.peers.clone() {
            self.apply_peer(&peer)?;
        }
        
        Ok(())
    }
    
    pub fn get_interface_index(&self) -> Option<u32> {
        self.interface_index
    }
    
    pub fn get_listen_port(&self) -> Option<u16> {
        None
    }
    
    pub fn add_peer(&mut self, peer: PeerConfig) -> Result<()> {
        info!("Adding peer {} with endpoint {}", peer.public_key, peer.endpoint);
        
        if self.running {
            self.apply_peer(&peer)?;
        }
        
        self.peers.push(peer);
        Ok(())
    }
    
    fn apply_peer(&mut self, peer: &PeerConfig) -> Result<()> {
        if !self.running {
            return Err(NetworkError::Netlink("WireGuard not running".to_string()));
        }
        
        set_wireguard_peer(&self.config.interface_name, peer)?;
        info!("Applied peer {} via wg command", peer.public_key);
        Ok(())
    }
    
    pub fn remove_peer(&mut self, public_key: &str) -> Result<()> {
        info!("Removing peer {}", public_key);
        
        if self.running {
            remove_wireguard_peer(&self.config.interface_name, public_key)?;
        }
        
        self.peers.retain(|p| p.public_key != public_key);
        Ok(())
    }
    
    pub fn list_peers(&self) -> &[PeerConfig] {
        &self.peers
    }
    
    pub fn stop(&mut self) -> Result<()> {
        info!("Stopping WireGuard node");
        
        for peer in self.peers.clone() {
            let _ = remove_wireguard_peer(&self.config.interface_name, &peer.public_key);
        }
        
        if self.running {
            let _ = delete_wireguard_interface(&self.config.interface_name);
        }
        
        self.handle = None;
        self.rt_handle = None;
        self.interface_index = None;
        self.peers.clear();
        self.running = false;
        
        Ok(())
    }
    
    pub fn is_running(&self) -> bool {
        self.running
    }
    
    pub fn get_public_key_base64(&self) -> String {
        self.public_key_base64()
    }
}

fn ip_octets_to_ip(ip: Ipv4Addr, last_octet: u8) -> Ipv4Addr {
    let mut octets = ip.octets();
    octets[3] = last_octet;
    Ipv4Addr::from(octets)
}

fn rtnetlink_get_interface_index(name: &str) -> std::result::Result<u32, NetworkError> {
    let output = Command::new("ip")
        .args(&["link", "show", name])
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        return Err(NetworkError::NotFound(format!("Interface {} not found", name)));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    for line in output_str.lines() {
        if let Some(idx) = line.find(':') {
            let before_colon = line[..idx].trim();
            if let Ok(index) = before_colon.parse::<u32>() {
                return Ok(index);
            }
        }
    }
    
    Err(NetworkError::NotFound(format!("Could not parse interface index for {}", name)))
}

fn async_set_interface_ip(handle: &Handle, if_index: u32, ip: IpNetwork) -> std::result::Result<(), NetworkError> {
    let handle = handle.clone();
    let ip_addr = ip.ip();
    let prefix = ip.prefix();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(handle
            .address()
            .add(if_index, ip_addr, prefix)
            .execute());
    }).join().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error")))
}

fn async_set_interface_up(handle: &Handle, if_index: u32) -> std::result::Result<(), NetworkError> {
    let handle = handle.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(handle
            .link()
            .set(if_index)
            .up()
            .execute());
    }).join().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error")))
}

fn set_wireguard_private_key(interface_name: &str, private_key_hex: &str) -> std::result::Result<(), NetworkError> {
    let mut child = Command::new("wg")
        .args(&["set", interface_name, "private-key", "/dev/stdin"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| NetworkError::Io(e))?;
    
    if let Some(ref mut stdin) = child.stdin {
        let key_bytes = hex::decode(private_key_hex)
            .map_err(|_| NetworkError::Netlink("Invalid hex key".to_string()))?;
        let base64_key = general_purpose::STANDARD.encode(&key_bytes);
        use std::io::Write;
        stdin.write_all(base64_key.as_bytes())
            .map_err(|e| NetworkError::Io(e))?;
    }
    
    let status = child.wait()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !status.success() {
        return Err(NetworkError::Netlink("Failed to set WireGuard private key".to_string()));
    }
    
    Ok(())
}

fn set_wireguard_listen_port(interface_name: &str, port: u16) -> std::result::Result<(), NetworkError> {
    let output = Command::new("wg")
        .args(&["set", interface_name, "listen-port", &port.to_string()])
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetworkError::Netlink(format!("Failed to set listen port: {}", stderr)));
    }
    
    Ok(())
}

fn set_wireguard_peer(interface_name: &str, peer_config: &PeerConfig) -> std::result::Result<(), NetworkError> {
    let mut args_vec: Vec<String> = vec!["set".to_string(), interface_name.to_string(), "peer".to_string(), peer_config.public_key.clone()];
    
    if let Some(keepalive) = peer_config.keepalive {
        args_vec.push("persistent-keepalive".to_string());
        args_vec.push(keepalive.to_string());
    }
    
    let args_refs: Vec<&str> = args_vec.iter().map(|s| s.as_str()).collect();
    let output = Command::new("wg")
        .args(&args_refs)
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetworkError::Netlink(format!("Failed to set peer: {}", stderr)));
    }
    
    for allowed_ip in &peer_config.allowed_ips {
        let output = Command::new("wg")
            .args(&["set", interface_name, "peer", &peer_config.public_key, "allowed-ips", allowed_ip])
            .output()
            .map_err(|e| NetworkError::Io(e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("Failed to set allowed-ip {}: {}", allowed_ip, stderr);
        }
    }
    
    let endpoint = peer_config.endpoint;
    let endpoint_str = format!("{}:{}", endpoint.ip(), endpoint.port());
    let output = Command::new("wg")
        .args(&["set", interface_name, "peer", &peer_config.public_key, "endpoint", &endpoint_str])
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("Failed to set endpoint: {}", stderr);
    }
    
    Ok(())
}

fn remove_wireguard_peer(interface_name: &str, public_key: &str) -> std::result::Result<(), NetworkError> {
    let output = Command::new("wg")
        .args(&["set", interface_name, "peer", public_key, "remove"])
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetworkError::Netlink(format!("Failed to remove peer: {}", stderr)));
    }
    
    Ok(())
}

fn delete_wireguard_interface(interface_name: &str) -> std::result::Result<(), NetworkError> {
    let output = Command::new("ip")
        .args(&["link", "del", interface_name])
        .output()
        .map_err(|e| NetworkError::Io(e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("does not exist") {
            return Err(NetworkError::Netlink(format!("Failed to delete interface: {}", stderr)));
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn create_test_node() -> WireGuardNode {
        let mut config = WireGuardConfig::default();
        config.interface_name = format!("test-wg-{}", rand::random::<u16>());
        config.node_ip = Some(Ipv4Addr::new(10, 42, 0, 1));
        WireGuardNode::new(config).unwrap()
    }

    #[test]
    fn test_wireguard_node_creation() {
        let node = create_test_node();
        assert!(!node.is_running());
        assert_eq!(node.list_peers().len(), 0);
    }

    #[test]
    fn test_public_key_generation() {
        let node = create_test_node();
        let pub_key = node.public_key_base64();
        assert!(!pub_key.is_empty());
        assert_eq!(pub_key.len(), 44);
    }

    #[test]
    fn test_peer_config_creation() {
        let peer = PeerConfig::new(
            "test_public_key_base64".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 51820),
        );
        assert_eq!(peer.allowed_ips.len(), 1);
        assert_eq!(peer.keepalive, Some(25));
    }

    #[test]
    fn test_peer_config_with_allowed_ips() {
        let peer = PeerConfig::new(
            "test_public_key".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 51820),
        ).with_allowed_ips(vec![
            "10.42.0.0/24".to_string(),
            "10.43.0.0/24".to_string(),
        ]);
        
        assert_eq!(peer.allowed_ips.len(), 2);
        assert!(peer.allowed_ips.contains(&"10.42.0.0/24".to_string()));
        assert!(peer.allowed_ips.contains(&"10.43.0.0/24".to_string()));
    }

    #[test]
    fn test_add_peer_to_node() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "test_peer_key".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 51820),
        );
        
        node.add_peer(peer.clone()).unwrap();
        assert_eq!(node.list_peers().len(), 1);
        assert_eq!(node.list_peers()[0].public_key, "test_peer_key");
    }

    #[test]
    fn test_remove_peer_from_node() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "peer_to_remove".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3)), 51820),
        );
        
        node.add_peer(peer.clone()).unwrap();
        assert_eq!(node.list_peers().len(), 1);
        
        node.remove_peer("peer_to_remove").unwrap();
        assert_eq!(node.list_peers().len(), 0);
    }

    #[test]
    fn test_multiple_peers() {
        let mut node = create_test_node();
        
        for i in 0..5 {
            let peer = PeerConfig::new(
                format!("peer_key_{}", i),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10 + i)), 51820),
            );
            node.add_peer(peer).unwrap();
        }
        
        assert_eq!(node.list_peers().len(), 5);
    }

    #[test]
    fn test_stop_clears_state() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "test_peer".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 4)), 51820),
        );
        
        node.add_peer(peer).unwrap();
        assert_eq!(node.list_peers().len(), 1);
        
        node.stop().unwrap();
        assert_eq!(node.list_peers().len(), 0);
        assert!(!node.is_running());
    }

    #[test]
    fn test_config_defaults() {
        let config = WireGuardConfig::default();
        assert_eq!(config.listen_port, 51820);
        assert_eq!(config.interface_name, "vyoma-wg0");
        assert_eq!(config.mtu, Some(1420));
        assert!(config.node_ip.is_none());
    }

    #[test]
    fn test_config_custom_values() {
        let mut config = WireGuardConfig::default();
        config.listen_port = 12345;
        config.interface_name = "custom-wg".to_string();
        config.mtu = Some(1500);
        config.node_ip = Some(Ipv4Addr::new(10, 100, 0, 1));
        
        assert_eq!(config.listen_port, 12345);
        assert_eq!(config.interface_name, "custom-wg");
        assert_eq!(config.mtu, Some(1500));
        assert_eq!(config.node_ip, Some(Ipv4Addr::new(10, 100, 0, 1)));
    }
}
