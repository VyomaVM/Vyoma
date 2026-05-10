use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::process::Command;
use tracing::{info, error, warn};

use vyoma_net::{WireGuardNode, WireGuardConfig, PeerConfig, add_route_to_peer_endpoint, add_route_to_subnet, remove_route_to_subnet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub ip: String,
    pub role: String,
    pub subnet_id: u8,
    pub wireguard_public_key: Option<String>,
    pub wireguard_port: Option<u16>,
}

impl NodeInfo {
    pub fn new(id: String, ip: String, role: String, subnet_id: u8) -> Self {
        Self {
            id,
            ip,
            role,
            subnet_id,
            wireguard_public_key: None,
            wireguard_port: None,
        }
    }
    
    pub fn with_wireguard(mut self, public_key: String, port: u16) -> Self {
        self.wireguard_public_key = Some(public_key);
        self.wireguard_port = Some(port);
        self
    }
}

#[derive(Clone)]
#[deprecated(since = "0.2.0", note = "Use SwarmRaft instead - all swarm operations now go through Raft")]
pub struct ClusterManager {
    pub self_node: Arc<Mutex<Option<NodeInfo>>>,
    pub peers: Arc<Mutex<HashMap<String, NodeInfo>>>,
    pub subnet_allocator: Arc<Mutex<u8>>,
    pub wireguard_node: Arc<Mutex<Option<WireGuardNode>>>,
    pub data_dir: PathBuf,
}

fn get_outbound_ip() -> String {
    match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => match s.connect("8.8.8.8:80") {
            Ok(_) => s.local_addr().map(|a| a.ip().to_string()).unwrap_or("127.0.0.1".to_string()),
            Err(_) => "127.0.0.1".to_string(),
        },
        Err(_) => "127.0.0.1".to_string(),
    }
}

#[deprecated(since = "0.2.0", note = "Use SwarmRaft instead")]
impl ClusterManager {
    #[deprecated(since = "0.2.0", note = "Use SwarmRaft::new() instead")]
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            self_node: Arc::new(Mutex::new(None)),
            peers: Arc::new(Mutex::new(HashMap::new())),
            subnet_allocator: Arc::new(Mutex::new(1)),
            wireguard_node: Arc::new(Mutex::new(None)),
            data_dir,
        }
    }
    
    #[deprecated(since = "0.2.0", note = "Use SwarmRaft instead")]
    fn get_wireguard_key_path(&self) -> PathBuf {
        let mut path = self.data_dir.clone();
        path.push("wireguard");
        path.push("private.key");
        path
    }
    
    #[deprecated(since = "0.2.0", note = "Network operations now handled by NetworkIntegration")]
    fn ensure_wireguard_node(&self, subnet_id: u8) -> anyhow::Result<WireGuardNode> {
        let key_path = self.get_wireguard_key_path();
        
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let node_ip = Ipv4Addr::new(10, 42, subnet_id, 1);
        
        let mut config = WireGuardConfig::default();
        config.node_ip = Some(node_ip);
        
        let mut node = WireGuardNode::from_key(key_path, config)?;
        node.start()?;
        
        Ok(node)
    }
    
    #[deprecated(since = "0.2.0", note = "Use /swarm/init with Raft instead")]
    pub fn init(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        
        if let Ok(wg_node) = self.ensure_wireguard_node(1) {
            let pub_key = wg_node.get_public_key_base64();
            let listen_port = wg_node.get_listen_port().unwrap_or(51820);
            *self.wireguard_node.lock().unwrap() = Some(wg_node);
            info!("WireGuard started on port {}", listen_port);
            
            let node = NodeInfo {
                id: id.clone(),
                ip: get_outbound_ip(),
                role: "seed".to_string(),
                subnet_id: 1,
                wireguard_public_key: Some(pub_key),
                wireguard_port: Some(listen_port),
            };
            *self.self_node.lock().unwrap() = Some(node.clone());
            self.peers.lock().unwrap().insert(id.clone(), node.clone());
            *self.subnet_allocator.lock().unwrap() = 2;
            info!("Swarm Initialized (Legacy). Node ID: {} (Subnet 10.42.1.0/24, WG Port {})", id, listen_port);
            self.setup_local_networking(1);
            id
        } else {
            let node = NodeInfo {
                id: id.clone(),
                ip: get_outbound_ip(),
                role: "seed".to_string(),
                subnet_id: 1,
                wireguard_public_key: None,
                wireguard_port: None,
            };
            *self.self_node.lock().unwrap() = Some(node.clone());
            self.peers.lock().unwrap().insert(id.clone(), node.clone());
            *self.subnet_allocator.lock().unwrap() = 2;
            info!("Swarm Initialized (Legacy without WireGuard). Node ID: {} (Subnet 10.42.1.0/24)", id);
            self.setup_local_networking(1);
            id
        }
    }

    #[deprecated(since = "0.2.0", note = "Use /swarm/join with Raft instead")]
    pub async fn join(&self, seed_ip: &str) -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let my_ip = get_outbound_ip();
        
        let key_path = self.get_wireguard_key_path();
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let mut wg_config = WireGuardConfig::default();
        wg_config.node_ip = None;
        
        let mut wg_node = WireGuardNode::from_key(key_path, wg_config)
            .map_err(|e| anyhow::anyhow!("Failed to init WG: {:?}", e))?;
        let my_pub_key = wg_node.get_public_key_base64();
        wg_node.start()
            .map_err(|e| anyhow::anyhow!("Failed to start WG: {:?}", e))?;
        let my_port = wg_node.get_listen_port().unwrap_or(51820);
        *self.wireguard_node.lock().unwrap() = Some(wg_node);
        
        let req_node = NodeInfo {
            id: id.clone(),
            ip: my_ip,
            role: "worker".to_string(),
            subnet_id: 0,
            wireguard_public_key: Some(my_pub_key),
            wireguard_port: Some(my_port),
        };
        
        info!("Joining swarm at {} (Legacy)...", seed_ip);
        
        let client = reqwest::Client::new();
        let port = std::env::var("IGNITE_DAEMON_PORT").unwrap_or_else(|_| "3000".to_string());
        let url = format!("http://{}:{}/swarm/register", seed_ip, port);
        let resp = client.post(&url)
            .json(&req_node)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to join swarm: {}", resp.status()));
        }
        
        #[derive(Deserialize)]
        struct RegisterResponse {
            assigned: NodeInfo,
            peers: Vec<NodeInfo>,
        }
        
        let body: RegisterResponse = resp.json().await?;
        let assigned_node = body.assigned;
        let peers = body.peers;
        
        info!("Joined Swarm (Legacy)! Assigned Subnet: 10.42.{}.0/24, WG Port: {}", assigned_node.subnet_id, my_port);
        
        *self.self_node.lock().unwrap() = Some(assigned_node.clone());
        self.setup_local_networking(assigned_node.subnet_id);
        
        for peer in &peers {
            if peer.id != assigned_node.id {
                self.add_wireguard_peer(peer)?;
                self.establish_route(peer);
            }
        }
        
        if let Some(wg) = self.wireguard_node.lock().unwrap().as_mut() {
            if let Some(seed_wg_key) = &assigned_node.wireguard_public_key {
                let seed_endpoint = format!("{}:{}",
                    assigned_node.ip,
                    assigned_node.wireguard_port.unwrap_or(51820));
                let peer = PeerConfig::new(
                    seed_wg_key.clone(),
                    seed_endpoint.parse().unwrap()
                ).with_allowed_ips(vec![format!("10.42.{}.0/24", assigned_node.subnet_id)]);
                wg.add_peer(peer)?;
                info!("Added seed node {} as WireGuard peer", assigned_node.id);
            }
        }
        
        Ok(())
    }
    
    #[deprecated(since = "0.2.0", note = "WireGuard peers now managed by NetworkIntegration")]
    pub fn add_wireguard_peer(&self, peer: &NodeInfo) -> anyhow::Result<()> {
        let mut wg_guard = self.wireguard_node.lock().unwrap();
        if let Some(wg) = wg_guard.as_mut() {
            if let (Some(pub_key), Some(port)) = (&peer.wireguard_public_key, peer.wireguard_port) {
                let endpoint = format!("{}:{}", peer.ip, port)
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid endpoint"))?;
                let peer_config = PeerConfig::new(pub_key.clone(), endpoint)
                    .with_allowed_ips(vec![format!("10.42.{}.0/24", peer.subnet_id)]);
                wg.add_peer(peer_config)?;
                info!("Added WireGuard peer (legacy): {} at {}", peer.id, endpoint);
            }
        }
        Ok(())
    }
    
    #[deprecated(since = "0.2.0", note = "Use Raft-based registration instead")]
    pub fn handle_registration(&self, mut node: NodeInfo) -> (NodeInfo, Vec<NodeInfo>) {
        let mut peers = self.peers.lock().unwrap();
        
        if let Some(existing) = peers.get(&node.id) {
            return (existing.clone(), peers.values().cloned().collect());
        }
        
        let mut alloc = self.subnet_allocator.lock().unwrap();
        node.subnet_id = *alloc;
        *alloc += 1;
        
        info!("Registered Node (Legacy) {} -> Subnet {}, WG Key: {:?}, Port: {:?}",
            node.id, node.subnet_id, node.wireguard_public_key, node.wireguard_port);
        
        peers.insert(node.id.clone(), node.clone());
        self.establish_route(&node);
        
        if let Some(wg) = self.wireguard_node.lock().unwrap().as_mut() {
            if let Some(pub_key) = &node.wireguard_public_key {
                let port = node.wireguard_port.unwrap_or(51820);
                let endpoint = format!("{}:{}", node.ip, port)
                    .parse()
                    .unwrap();
                let peer = PeerConfig::new(pub_key.clone(), endpoint)
                    .with_allowed_ips(vec![format!("10.42.{}.0/24", node.subnet_id)]);
                if let Err(e) = wg.add_peer(peer) {
                    warn!("Failed to add WG peer: {:?}", e);
                } else {
                    info!("Added {} as WireGuard peer", node.id);
                }
            }
        }
        
        let all_peers: Vec<NodeInfo> = peers.values().cloned().collect();
        (node, all_peers)
    }
    
    #[allow(dead_code)]
    #[deprecated(since = "0.2.0", note = "Node discovery now via Raft")]
    pub fn add_node_notify(&self, node: NodeInfo) {
        let mut peers = self.peers.lock().unwrap();
        if !peers.contains_key(&node.id) {
             info!("Discovered Node (Legacy) {} (Subnet {})", node.id, node.subnet_id);
             self.establish_route(&node);
             peers.insert(node.id.clone(), node);
        }
    }
    
    #[deprecated(since = "0.2.0", note = "Use /swarm/nodes with SwarmRaft instead")]
    pub fn list_nodes(&self) -> Vec<NodeInfo> {
        self.peers.lock().unwrap().values().cloned().collect()
    }
    
    #[deprecated(since = "0.2.0", note = "Network setup now handled by NetworkIntegration")]
    fn setup_local_networking(&self, subnet_id: u8) {
        let subnet = format!("10.42.{}.0/24", subnet_id);
        
        let home = dirs::home_dir().expect("No home dir");
        let cni_config_dir = home.join(".ignite").join("cni").join("net.d");
        let bridge_conf = cni_config_dir.join("10-ignite-bridge.conf");
        
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
            serde_json::to_writer_pretty(f, &conf).unwrap();
        }
        
        self.ensure_vxlan_device();
    }
    
    #[deprecated(since = "0.2.0", note = "VXLAN now managed by NetworkIntegration")]
    fn ensure_vxlan_device(&self) {
        let if_name = "ign-vxlan";
        
        let output = Command::new("ip").args(&["link", "show", if_name]).output();
        if let Ok(o) = output {
            if o.status.success() { return; }
        }
        
        info!("Creating VXLAN device (legacy) {}", if_name);
        let _ = Command::new("ip")
            .args(&["link", "add", if_name, "type", "vxlan", "id", "42", "dstport", "4789", "external"])
            .output();
        let _ = Command::new("ip")
            .args(&["link", "set", if_name, "up"])
            .output();
    }
    
    #[deprecated(since = "0.2.0", note = "Routes now managed by NetworkIntegration")]
    fn establish_route(&self, peer: &NodeInfo) {
        let wg_iface = "vyoma-wg0";
        
        if let Some(wg) = self.wireguard_node.lock().unwrap().as_ref() {
            if wg.is_running() {
                if let Err(e) = add_route_to_peer_endpoint(&peer.ip, wg_iface) {
                    warn!("Failed to add route to peer {} via WG: {:?}", peer.ip, e);
                }
                
                let subnet = format!("10.42.{}.0/24", peer.subnet_id);
                if let Err(e) = add_route_to_subnet(&subnet, &peer.ip, wg_iface) {
                    warn!("Failed to add subnet route {}: {:?}", subnet, e);
                }
                
                info!("Established WireGuard route (legacy) to {} (Subnet {})", peer.ip, subnet);
                return;
            }
        }
        
        let peer_subnet = format!("10.42.{}.0/24", peer.subnet_id);
        let if_name = "ign-vxlan";
        
        info!("Adding route (legacy) to {} via VXLAN -> {}", peer_subnet, peer.ip);
        
        let res = Command::new("ip")
            .args(&[
                "route", "add", &peer_subnet,
                "encap", "vxlan", "id", "42", "dst", &peer.ip,
                "dev", if_name
            ])
            .output();
            
        if let Err(e) = res {
            error!("Failed to add route: {}", e);
        } else {
             let out = res.unwrap();
             if !out.status.success() {
                  warn!("Route add failed (maybe exists): {}", String::from_utf8_lossy(&out.stderr));
             }
        }
    }
    
    #[deprecated(since = "0.2.0", note = "Routes now managed by NetworkIntegration")]
    pub fn remove_route_for_peer(&self, peer: &NodeInfo) -> anyhow::Result<()> {
        let subnet = format!("10.42.{}.0/24", peer.subnet_id);
        
        if let Err(e) = remove_route_to_subnet(&subnet) {
            warn!("Failed to remove subnet route {}: {:?}", subnet, e);
        }
        
        info!("Removed route (legacy) for peer {} (Subnet {})", peer.id, peer.subnet_id);
        Ok(())
    }
    
    #[deprecated(since = "0.2.0", note = "Shutdown now handled by NetworkIntegration")]
    pub fn shutdown(&self) {
        info!("Shutting down ClusterManager WireGuard (legacy)...");
        
        if let Some(mut wg) = self.wireguard_node.lock().unwrap().take() {
            for peer in self.peers.lock().unwrap().values() {
                let subnet = format!("10.42.{}.0/24", peer.subnet_id);
                let _ = remove_route_to_subnet(&subnet);
            }
            let _ = wg.stop();
        }
        
        info!("ClusterManager WireGuard shutdown (legacy) complete");
    }
}