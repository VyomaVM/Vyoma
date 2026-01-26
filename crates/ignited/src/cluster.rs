use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::process::Command;
use tracing::{info, error, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub ip: String,
    pub role: String, // "seed" or "worker"
}

#[derive(Clone)]
pub struct ClusterManager {
    // Current Node Info
    pub self_node: Arc<Mutex<Option<NodeInfo>>>,
    // Known Peers
    pub peers: Arc<Mutex<HashMap<String, NodeInfo>>>,
}

impl ClusterManager {
    pub fn new() -> Self {
        Self {
            self_node: Arc::new(Mutex::new(None)),
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn init(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        // Detect IP: In production, find outgoing IP. MVP: 127.0.0.1 or prompt user.
        // We will assume the user has configured --ip in CLI or we take first non-local.
        // For MVP, if we run locally, we might conflict on ports if we use same IP. 
        // We assume 127.0.0.1 for local dev swarm.
        let node = NodeInfo {
            id: id.clone(),
            ip: "127.0.0.1".to_string(), // TODO: Get real IP
            role: "seed".to_string(),
        };
        *self.self_node.lock().unwrap() = Some(node.clone());
        self.peers.lock().unwrap().insert(id.clone(), node);
        
        info!("Swarm Initialized. Node ID: {}", id);
        id
    }

    pub async fn join(&self, seed_ip: &str) -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        // Determine my IP
        let my_ip = "127.0.0.1"; // TODO: Discovery
        
        let node = NodeInfo {
            id: id.clone(),
            ip: my_ip.to_string(),
            role: "worker".to_string(),
        };
        *self.self_node.lock().unwrap() = Some(node.clone());
        
        info!("Joining swarm at {}", seed_ip);
        
        // 1. Register with Seed
        let client = reqwest::Client::new();
        let url = format!("http://{}:3000/swarm/register", seed_ip);
        let resp = client.post(&url)
            .json(&node)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to join swarm: {}", resp.status()));
        }
        
        // 2. Get Cluster State (Peers)
        let peers: Vec<NodeInfo> = resp.json().await?;
        
        // 3. Update Local State & Mesh
        {
            let mut p = self.peers.lock().unwrap();
            for peer in peers {
                p.insert(peer.id.clone(), peer.clone());
                // Establish Tunnel to Peer (if not self)
                if peer.id != id {
                   self.establish_tunnel(&peer);
                }
            }
        }
        
        Ok(())
    }
    
    // Called by Handler when a new node registers
    pub fn add_node(&self, node: NodeInfo) {
        let mut peers = self.peers.lock().unwrap();
        // If new, establish tunnel
        if !peers.contains_key(&node.id) {
            info!("New Node Joined: {} ({})", node.id, node.ip);
            self.establish_tunnel(&node);
            peers.insert(node.id.clone(), node);
        }
    }
    
    pub fn list_nodes(&self) -> Vec<NodeInfo> {
        self.peers.lock().unwrap().values().cloned().collect()
    }
    
    fn establish_tunnel(&self, peer: &NodeInfo) {
        // VXLAN Mesh Logic
        // ip link add vx-<short-id> type vxlan id 42 remote <peer-ip> dstport 4789 dev <out-dev>
        // brctl addif ign0 vx-<short-id>
        // ip link set vx-<short-id> up
        
        let if_name = format!("vx-{}", &peer.id[0..6]);
        info!("Establishing VXLAN tunnel {} to {}", if_name, peer.ip);
        
        // MVP: Using 'lo' or 'eth0' as dev?
        // We need to know which interface connects to peer.
        // Assuming 'eth0' for now.
        let dev = "eth0"; 
        
        // Check if exists
        let output = Command::new("ip").args(&["link", "show", &if_name]).output();
        if let Ok(o) = output {
            if o.status.success() {
                // Exists, assume ok
                return;
            }
        }
        
        // Create
        let res = Command::new("ip")
            .args(&[
                "link", "add", &if_name, 
                "type", "vxlan", 
                "id", "42", 
                "remote", &peer.ip, 
                "dstport", "4789", 
                "dev", dev
            ])
            .output();
            
        if let Err(e) = res {
            error!("Failed to create VXLAN {}: {}", if_name, e);
            return;
        }
        
        // Attach to Bridge ign0
        let _ = Command::new("brctl").args(&["addif", "ign0", &if_name]).output();
        // Set UP
        let _ = Command::new("ip").args(&["link", "set", &if_name, "up"]).output();
    }
}
