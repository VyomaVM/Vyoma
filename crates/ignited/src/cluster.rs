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
    pub subnet_id: u8,
}

#[derive(Clone)]
pub struct ClusterManager {
    // Current Node Info
    pub self_node: Arc<Mutex<Option<NodeInfo>>>,
    // Known Peers
    pub peers: Arc<Mutex<HashMap<String, NodeInfo>>>,
    // Subnet Allocator (Only used if Seed)
    pub subnet_allocator: Arc<Mutex<u8>>,
}

fn get_outbound_ip() -> String {
    // Best effort to find the primary interface IP
    match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => match s.connect("8.8.8.8:80") {
            Ok(_) => s.local_addr().map(|a| a.ip().to_string()).unwrap_or("127.0.0.1".to_string()),
            Err(_) => "127.0.0.1".to_string(),
        },
        Err(_) => "127.0.0.1".to_string(),
    }
}

impl ClusterManager {
    pub fn new() -> Self {
        Self {
            self_node: Arc::new(Mutex::new(None)),
            peers: Arc::new(Mutex::new(HashMap::new())),
            subnet_allocator: Arc::new(Mutex::new(1)),
        }
    }

    pub fn init(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        
        // Seed gets Subnet 1
        let node = NodeInfo {
            id: id.clone(),
            ip: get_outbound_ip(),
            role: "seed".to_string(),
            subnet_id: 1,
        };
        
        *self.self_node.lock().unwrap() = Some(node.clone());
        self.peers.lock().unwrap().insert(id.clone(), node.clone());
        
        // Next available: 2
        *self.subnet_allocator.lock().unwrap() = 2;
        
        info!("Swarm Initialized. Node ID: {} (Subnet 10.42.1.0/24)", id);
        
        // Setup Local Network
        self.setup_local_networking(1);
        
        id
    }

    pub async fn join(&self, seed_ip: &str) -> anyhow::Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let my_ip = get_outbound_ip();
        
        // Temporary info for Request
        let req_node = NodeInfo {
            id: id.clone(),
            ip: my_ip,
            role: "worker".to_string(),
            subnet_id: 0, // Requesting allocation
        };
        
        info!("Joining swarm at {}...", seed_ip);
        
        // 1. Register with Seed
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
        
        info!("Joined Swarm! Assigned Subnet: 10.42.{}.0/24", assigned_node.subnet_id);
        
        // 2. Update Self
        *self.self_node.lock().unwrap() = Some(assigned_node.clone());
        
        // 3. Setup Local Network (Bridge + CNI)
        self.setup_local_networking(assigned_node.subnet_id);
        
        // 4. Update Peers & Routes
        {
            let mut p = self.peers.lock().unwrap();
            for peer in peers {
                p.insert(peer.id.clone(), peer.clone());
                if peer.id != assigned_node.id {
                   self.establish_route(&peer);
                }
            }
        }
        
        Ok(())
    }
    
    // Seed Logic: Validate and Assign
    pub fn handle_registration(&self, mut node: NodeInfo) -> (NodeInfo, Vec<NodeInfo>) {
        let mut peers = self.peers.lock().unwrap();
        
        // If re-joining?
        if let Some(existing) = peers.get(&node.id) {
            return (existing.clone(), peers.values().cloned().collect());
        }
        
        // Assign Subnet
        let mut alloc = self.subnet_allocator.lock().unwrap();
        node.subnet_id = *alloc;
        *alloc += 1; // Increment for next
        
        info!("Registered Node {} -> Subnet {}", node.id, node.subnet_id);
        
        // Add to peers
        peers.insert(node.id.clone(), node.clone());
        
        // Establish route to new node (Seed needs route to Worker too)
        self.establish_route(&node);
        
        (node, peers.values().cloned().collect())
    }
    
    #[allow(dead_code)]
    pub fn add_node_notify(&self, node: NodeInfo) {
        let mut peers = self.peers.lock().unwrap();
        if !peers.contains_key(&node.id) {
             info!("Discovered Node {} (Subnet {})", node.id, node.subnet_id);
             self.establish_route(&node);
             peers.insert(node.id.clone(), node);
        }
    }
    
    pub fn list_nodes(&self) -> Vec<NodeInfo> {
        self.peers.lock().unwrap().values().cloned().collect()
    }
    
    // --- Networking Logic ---
    
    fn setup_local_networking(&self, subnet_id: u8) {
        // 1. Configure CNI Config
        let subnet = format!("10.42.{}.0/24", subnet_id);
        let bridge_ip = format!("10.42.{}.1", subnet_id);
        
        // Write CNI (Overwrite existing)
        // We need path to CNI dir. Hardcoded here as per main.rs
        let home = dirs::home_dir().expect("No home dir");
        let cni_config_dir = home.join(".ignite").join("cni").join("net.d");
        let bridge_conf = cni_config_dir.join("10-ignite-bridge.conf");
        
        let conf = serde_json::json!({
            "cniVersion": "0.4.0",
            "name": "ignite-net",
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
        
        // 2. Configure Bridge IP manually (CNI does it on first Container run usually, 
        // but for Overlay reachability we might want it up?
        // Actually, CNI creates the bridge.
        // We just need to Ensure VXLAN interface exists.
        
        self.ensure_vxlan_device();
    }
    
    fn ensure_vxlan_device(&self) {
        // ip link add ign-vxlan type vxlan id 42 dstport 4789 external
        let if_name = "ign-vxlan";
        
        let output = Command::new("ip").args(&["link", "show", if_name]).output();
        if let Ok(o) = output {
            if o.status.success() { return; } // Exists
        }
        
        info!("Creating VXLAN device {}", if_name);
        let _ = Command::new("ip")
            .args(&["link", "add", if_name, "type", "vxlan", "id", "42", "dstport", "4789", "external"])
            .output();
        let _ = Command::new("ip")
            .args(&["link", "set", if_name, "up"])
            .output();
    }
    
    fn establish_route(&self, peer: &NodeInfo) {
        // ip route add 10.42.{id}.0/24 encap vxlan id 42 dst {peer.ip} dev ign-vxlan
        let peer_subnet = format!("10.42.{}.0/24", peer.subnet_id);
        let if_name = "ign-vxlan";
        
        info!("Adding route to {} via VXLAN -> {}", peer_subnet, peer.ip);
        
        // Check if route exists?
        // ip route add ...
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
                  // Might actuall fail if exists
                  warn!("Route add failed (maybe exists): {}", String::from_utf8_lossy(&out.stderr));
             }
        }
    }
}
