use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
        // MVP: Assume localhost for now, or detect interface IP
        let node = NodeInfo {
            id: id.clone(),
            ip: "127.0.0.1".to_string(),
            role: "seed".to_string(),
        };
        *self.self_node.lock().unwrap() = Some(node.clone());
        // Add self to peers
        self.peers.lock().unwrap().insert(id.clone(), node);
        id
    }

    pub fn join(&self, seed_ip: &str) -> anyhow::Result<()> {
        // In real implementation, we would HTTP POST to seed_ip/cluster/join
        // For MVP skeleton, we just update local state.

        let id = uuid::Uuid::new_v4().to_string();
        let node = NodeInfo {
            id: id.clone(),
            ip: "127.0.0.1".to_string(),
            role: "worker".to_string(),
        };
        *self.self_node.lock().unwrap() = Some(node.clone());

        let mut peers = self.peers.lock().unwrap();
        peers.insert(id, node);
        peers.insert(
            "seed_placeholder".to_string(),
            NodeInfo {
                id: "seed_placeholder".to_string(),
                ip: seed_ip.to_string(),
                role: "seed".to_string(),
            },
        );

        Ok(())
    }

    pub fn list_nodes(&self) -> Vec<NodeInfo> {
        self.peers.lock().unwrap().values().cloned().collect()
    }
}
