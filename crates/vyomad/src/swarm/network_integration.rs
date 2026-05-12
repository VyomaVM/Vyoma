use std::sync::{Arc, Mutex};
use std::process::Command;
use std::net::{Ipv4Addr, SocketAddr};
use tracing::{info, warn, error};

use vyoma_net::{WireGuardNode, WireGuardConfig, PeerConfig, add_route_to_peer_endpoint, add_route_to_subnet, remove_route_to_subnet};

use crate::swarm::SwarmSideEffect;

pub struct NetworkIntegration {
    wireguard_node: Arc<Mutex<Option<WireGuardNode>>>,
    data_dir: std::path::PathBuf,
}

impl NetworkIntegration {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self {
            wireguard_node: Arc::new(Mutex::new(None)),
            data_dir,
        }
    }

    fn ensure_wireguard(&self, subnet_id: u8) -> bool {
        let mut guard = match self.wireguard_node.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("Failed to lock wireguard: {}", e);
                return false;
            }
        };
        
        if let Some(ref wg) = *guard {
            if wg.is_running() {
                return true;
            }
        }
        
        let key_path = self.data_dir.join("wireguard").join("private.key");
        if let Some(parent) = key_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Failed to create wireguard dir: {}", e);
                return false;
            }
        }
        
        let node_ip = Ipv4Addr::new(10, 42, subnet_id, 1);
        let mut config = WireGuardConfig::default();
        config.node_ip = Some(node_ip);
        
        match WireGuardNode::from_key(key_path, config) {
            Ok(mut node) => {
                if node.start().is_ok() {
                    info!("WireGuard started on subnet 10.42.{}.0/24", subnet_id);
                    *guard = Some(node);
                    return true;
                }
            }
            Err(e) => {
                warn!("Failed to init WireGuard: {}", e);
            }
        }
        
        false
    }

    pub fn setup_local_node(&self, node_id: u64, subnet_id: u8, peers: &[crate::swarm::NodeInfo]) {
        info!("Setting up local node {} with subnet 10.42.{}.0/24", node_id, subnet_id);
        
        self.ensure_wireguard(subnet_id);
        
        self.configure_bridge(subnet_id);
        
        self.ensure_vxlan_device();
        
        for peer in peers {
            if let (Some(wg_key), Some(wg_port)) = (&peer.wireguard_key, peer.wireguard_port) {
                if let Some(addr) = peer.addr.split(':').next() {
                    self.add_wireguard_peer(
                        wg_key,
                        &format!("{}:{}", addr, wg_port),
                        &format!("10.42.{}.0/24", peer.subnet_id.unwrap_or(0)),
                    );
                }
            }
            
            if let Some(addr) = peer.addr.split(':').next() {
                self.add_vxlan_route(addr, peer.subnet_id.unwrap_or(0));
            }
        }
        
        info!("Local node setup complete");
    }

    pub fn add_node(&self, node_id: u64, addr: &str, wireguard_key: Option<&str>, wireguard_port: Option<u16>, subnet_id: u8) {
        info!("Adding node {} at {} to network (subnet: 10.42.{}.0/24)", node_id, addr, subnet_id);
        
        if let (Some(key), Some(port)) = (wireguard_key, wireguard_port) {
            self.add_wireguard_peer(key, &format!("{}:{}", addr, port), &format!("10.42.{}.0/24", subnet_id));
        }
        
        self.add_vxlan_route(addr, subnet_id);
    }

    pub fn remove_node(&self, node_id: u64, subnet_id: u8) {
        info!("Removing node {} from network (subnet: 10.42.{}.0/24)", node_id, subnet_id);
        
        self.remove_subnet_route(subnet_id);
    }

    pub fn update_node(&self, _node_id: u64, old_subnet_id: u8, new_addr: Option<&str>, new_wg_key: Option<&str>, new_wg_port: Option<u16>, _new_subnet_id: u8) {
        if new_addr.is_some() || new_wg_key.is_some() {
            if let Some(addr) = new_addr {
                if let (Some(key), Some(port)) = (new_wg_key, new_wg_port) {
                    self.add_wireguard_peer(key, &format!("{}:{}", addr, port), &format!("10.42.{}.0/24", old_subnet_id));
                }
            }
        }
    }

    fn add_wireguard_peer(&self, public_key: &str, endpoint: &str, allowed_ips: &str) {
        let mut guard = match self.wireguard_node.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("Failed to lock wireguard: {}", e);
                return;
            }
        };
        
        if let Some(ref mut wg) = *guard {
            if wg.is_running() {
                let endpoint_addr: SocketAddr = match endpoint.parse() {
                    Ok(addr) => addr,
                    Err(e) => {
                        warn!("Invalid endpoint {}: {}", endpoint, e);
                        return;
                    }
                };
                
                let peer_config = PeerConfig::new(public_key.to_string(), endpoint_addr)
                    .with_allowed_ips(vec![allowed_ips.to_string()]);
                
                match wg.add_peer(peer_config) {
                    Ok(_) => info!("Added WireGuard peer: {} at {}", public_key, endpoint),
                    Err(e) => warn!("Failed to add WG peer: {}", e),
                }
            }
        } else {
            warn!("WireGuard not available, skipping peer addition");
        }
    }

    fn add_vxlan_route(&self, peer_ip: &str, subnet_id: u8) {
        let subnet = format!("10.42.{}.0/24", subnet_id);
        
        let wg_iface = "vyoma-wg0";
        
        {
            let guard = match self.wireguard_node.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("Failed to lock wireguard: {}", e);
                    return;
                }
            };
            
            if let Some(ref wg) = *guard {
                if wg.is_running() {
                    if let Some(handle) = wg.get_rt_handle() {
                        let _ = add_route_to_peer_endpoint(handle, peer_ip, wg_iface);
                        let _ = add_route_to_subnet(handle, &subnet, peer_ip, wg_iface);
                    }
                    info!("Added route to {} via WireGuard", subnet);
                    return;
                }
            }
        }
        
        let if_name = "vyoma-vxlan";
        let vxlan_id = "42";
        
        info!("Adding route to {} via VXLAN -> {}", subnet, peer_ip);
        
        let res = Command::new("ip")
            .args(&[
                "route", "add", &subnet,
                "encap", "vxlan", "id", vxlan_id, "dst", peer_ip,
                "dev", if_name
            ])
            .output();
            
        match res {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !stderr.contains("File exists") && !stderr.contains("exists") {
                        warn!("Route add failed: {}", stderr);
                    }
                } else {
                    info!("Added VXLAN route to {}", subnet);
                }
            }
            Err(e) => error!("Failed to add route: {}", e),
        }
    }

    fn remove_subnet_route(&self, subnet_id: u8) {
        let subnet = format!("10.42.{}.0/24", subnet_id);

        let handle = {
            let guard = match self.wireguard_node.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if let Some(ref wg) = *guard {
                wg.get_rt_handle().map(|h| h.clone())
            } else {
                None
            }
        };

        if let Some(handle) = handle {
            if let Err(e) = remove_route_to_subnet(&handle, &subnet) {
                warn!("Failed to remove subnet route {}: {:?}", subnet, e);
            } else {
                info!("Removed route for subnet {}", subnet);
            }
        }
    }

    fn configure_bridge(&self, subnet_id: u8) {
        let subnet = format!("10.42.{}.0/24", subnet_id);
        
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => {
                warn!("No home dir found");
                return;
            }
        };
        let cni_config_dir = home.join(".vyoma").join("cni").join("net.d");
        let bridge_conf = cni_config_dir.join("10-vyoma-bridge.conf");
        
        if !cni_config_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&cni_config_dir) {
                warn!("Failed to create CNI config dir: {}", e);
                return;
            }
        }
        
        let conf = serde_json::json!({
            "cniVersion": "0.4.0",
            "name": "vyoma-net",
            "type": "bridge",
            "bridge": "ign0",
            "isGateway": true,
            "ipMasq": true,
            "ipam": {
                "type": "host-local",
                "subnet": subnet,
                "routes": [ { "dst": "0.0.0.0/0" } ]
            }
        });
        
        if let Ok(f) = std::fs::File::create(&bridge_conf) {
            if let Err(e) = serde_json::to_writer_pretty(f, &conf) {
                warn!("Failed to write CNI config: {}", e);
            } else {
                info!("Configured bridge for subnet {}", subnet);
            }
        }
    }

    fn ensure_vxlan_device(&self) {
        let if_name = "vyoma-vxlan";
        
        let output = Command::new("ip").args(&["link", "show", if_name]).output();
        if let Ok(o) = output {
            if o.status.success() { return; }
        }
        
        info!("Creating VXLAN device {}", if_name);
        let _ = Command::new("ip")
            .args(&["link", "add", if_name, "type", "vxlan", "id", "42", "dstport", "4789", "external"])
            .output();
        let _ = Command::new("ip")
            .args(&["link", "set", if_name, "up"])
            .output();
    }

    pub fn shutdown(&self) {
        info!("Shutting down NetworkIntegration...");
        
        if let Ok(mut guard) = self.wireguard_node.lock() {
            if let Some(mut wg) = guard.take() {
                let _ = wg.stop();
            }
        }
        
        info!("NetworkIntegration shutdown complete");
    }
}

impl Clone for NetworkIntegration {
    fn clone(&self) -> Self {
        Self {
            wireguard_node: self.wireguard_node.clone(),
            data_dir: self.data_dir.clone(),
        }
    }
}

pub fn create_network_callback(
    network_integration: NetworkIntegration,
) -> Box<dyn Fn(&SwarmSideEffect) + Send + Sync> {
    Box::new(move |effect| {
        match effect {
            SwarmSideEffect::LocalNodeConfigured { node_id, subnet_id, peers } => {
                network_integration.setup_local_node(*node_id, *subnet_id, peers);
            }
            SwarmSideEffect::NodeAdded { node_id, addr, wireguard_key, wireguard_port, subnet_id } => {
                network_integration.add_node(
                    *node_id,
                    addr,
                    wireguard_key.as_deref(),
                    *wireguard_port,
                    *subnet_id,
                );
            }
            SwarmSideEffect::NodeRemoved { node_id, subnet_id } => {
                network_integration.remove_node(*node_id, *subnet_id);
            }
            SwarmSideEffect::NodeUpdated { node_id, old_subnet_id, new_addr, new_wireguard_key, new_wireguard_port } => {
                network_integration.update_node(
                    *node_id,
                    *old_subnet_id,
                    new_addr.as_deref(),
                    new_wireguard_key.as_deref(),
                    *new_wireguard_port,
                    *old_subnet_id,
                );
            }
        }
    })
}