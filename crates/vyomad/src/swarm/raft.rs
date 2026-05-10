use std::collections::BTreeMap;
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwarmCommand {
    AddNode {
        node_id: u64,
        addr: String,
        public_key: String,
        wireguard_key: Option<String>,
        wireguard_port: Option<u16>,
    },
    RemoveNode {
        node_id: u64,
    },
    UpdateNodeEndpoint {
        node_id: u64,
        addr: Option<String>,
        wireguard_key: Option<String>,
        wireguard_port: Option<u16>,
    },
    RegisterNode { node_id: u64, addr: String, public_key: String },
    DeregisterNode { node_id: u64 },
    UpdateVmPlacement { vm_id: String, node_id: u64 },
    RemoveVmPlacement { vm_id: String },
    CreateService { name: String, spec: ServiceSpec },
    UpdateService { name: String, spec: ServiceSpec },
    DeleteService { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceSpec {
    pub image: String,
    pub replicas: u32,
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortMapping {
    pub host: u16,
    pub vm: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub addr: String,
    pub public_key: String,
    pub wireguard_key: Option<String>,
    pub wireguard_port: Option<u16>,
    pub subnet_id: Option<u8>,
    pub is_leader: bool,
}

impl NodeInfo {
    pub fn new(id: u64, addr: String, public_key: String) -> Self {
        Self {
            id,
            addr,
            public_key,
            wireguard_key: None,
            wireguard_port: None,
            subnet_id: None,
            is_leader: false,
        }
    }

    pub fn with_wireguard(mut self, key: String, port: u16) -> Self {
        self.wireguard_key = Some(key);
        self.wireguard_port = Some(port);
        self
    }

    pub fn subnet(&self) -> Option<String> {
        self.subnet_id.map(|id| format!("10.42.{}.0/24", id))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmPlacement {
    pub vm_id: String,
    pub node_id: u64,
}

pub struct SwarmRaft {
    node_id: u64,
    nodes: BTreeMap<u64, NodeInfo>,
    vm_placements: BTreeMap<String, VmPlacement>,
    services: BTreeMap<String, ServiceSpec>,
    subnet_map: BTreeMap<u64, u8>,
    is_initialized: bool,
    next_subnet_id: u8,
    last_applied_index: u64,
    applied_commands: Vec<u64>,
    side_effect_callback: Option<Box<dyn Fn(&SwarmSideEffect) + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub enum SwarmSideEffect {
    NodeAdded {
        node_id: u64,
        addr: String,
        wireguard_key: Option<String>,
        wireguard_port: Option<u16>,
        subnet_id: u8,
    },
    NodeRemoved {
        node_id: u64,
        subnet_id: u8,
    },
    NodeUpdated {
        node_id: u64,
        old_subnet_id: u8,
        new_addr: Option<String>,
        new_wireguard_key: Option<String>,
        new_wireguard_port: Option<u16>,
    },
    LocalNodeConfigured {
        node_id: u64,
        subnet_id: u8,
        peers: Vec<NodeInfo>,
    },
}

impl SwarmRaft {
    pub fn new(node_id: u64) -> Self {
        info!("Creating SwarmRaft node {}", node_id);
        
        Self {
            node_id,
            nodes: BTreeMap::new(),
            vm_placements: BTreeMap::new(),
            services: BTreeMap::new(),
            subnet_map: BTreeMap::new(),
            is_initialized: false,
            next_subnet_id: 1,
            last_applied_index: 0,
            applied_commands: Vec::new(),
            side_effect_callback: None,
        }
    }

    pub fn set_side_effect_callback(&mut self, callback: Box<dyn Fn(&SwarmSideEffect) + Send + Sync>) {
        self.side_effect_callback = Some(callback);
    }

    pub fn node_id(&self) -> u64 {
        self.node_id
    }
    
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    pub fn last_applied_index(&self) -> u64 {
        self.last_applied_index
    }

    pub fn set_last_applied_index(&mut self, index: u64) {
        self.last_applied_index = index;
    }
    
    fn compute_subnet_id(node_id: u64) -> u8 {
        if node_id == 0 {
            return 1;
        }
        ((node_id % 254) + 1) as u8
    }

    fn allocate_subnet(&mut self, node_id: u64) -> u8 {
        if let Some(existing) = self.subnet_map.get(&node_id) {
            return *existing;
        }
        let subnet_id = Self::compute_subnet_id(node_id);
        self.subnet_map.insert(node_id, subnet_id);
        subnet_id
    }
    
    pub fn bootstrap(&mut self, addr: String, public_key: String, wireguard_key: Option<String>, wireguard_port: Option<u16>) -> Result<(), String> {
        if self.is_initialized {
            return Err("Already initialized".to_string());
        }
        
        info!("Bootstrapping cluster with node {} at {}", self.node_id, addr);
        
        let subnet_id = self.allocate_subnet(self.node_id);
        
        let mut node = NodeInfo::new(self.node_id, addr, public_key);
        node.is_leader = true;
        node.subnet_id = Some(subnet_id);
        if let (Some(key), Some(port)) = (wireguard_key, wireguard_port) {
            node = node.with_wireguard(key, port);
        }
        
        self.nodes.insert(self.node_id, node);
        self.is_initialized = true;
        
        self.trigger_side_effect(&SwarmSideEffect::LocalNodeConfigured {
            node_id: self.node_id,
            subnet_id,
            peers: vec![],
        });
        
        Ok(())
    }
    
    pub fn add_node(&mut self, node_id: u64, addr: String, public_key: String, wireguard_key: Option<String>, wireguard_port: Option<u16>) -> Result<u8, String> {
        if !self.is_initialized {
            return Err("Cluster not initialized".to_string());
        }
        
        if self.nodes.contains_key(&node_id) {
            return Err(format!("Node {} already exists", node_id));
        }
        
        info!("Adding node {} at {} to cluster", node_id, addr);
        
        let subnet_id = self.allocate_subnet(node_id);
        
        let wg_key = wireguard_key.clone();
        let wg_port = wireguard_port;
        let addr_clone = addr.clone();
        
        let mut node = NodeInfo::new(node_id, addr, public_key);
        node.subnet_id = Some(subnet_id);
        if let (Some(key), Some(port)) = (wg_key, wg_port) {
            node = node.with_wireguard(key, port);
        }
        
        self.nodes.insert(node_id, node);
        
        self.trigger_side_effect(&SwarmSideEffect::NodeAdded {
            node_id,
            addr: addr_clone,
            wireguard_key,
            wireguard_port,
            subnet_id,
        });
        
        self.trigger_side_effect(&SwarmSideEffect::LocalNodeConfigured {
            node_id: self.node_id,
            subnet_id: self.subnet_map.get(&self.node_id).copied().unwrap_or(1),
            peers: self.nodes.values().cloned().filter(|n| n.id != self.node_id).collect(),
        });
        
        Ok(subnet_id)
    }
    
    pub fn remove_node(&mut self, node_id: u64) -> Result<(), String> {
        if node_id == self.node_id {
            return Err("Cannot remove self".to_string());
        }
        
        if !self.nodes.contains_key(&node_id) {
            return Err(format!("Node {} not found", node_id));
        }
        
        let subnet_id = self.subnet_map.get(&node_id).copied().unwrap_or(0);
        
        info!("Removing node {} from cluster", node_id);
        
        self.nodes.remove(&node_id);
        self.subnet_map.remove(&node_id);
        
        self.vm_placements.retain(|_, p| p.node_id != node_id);
        
        self.trigger_side_effect(&SwarmSideEffect::NodeRemoved {
            node_id,
            subnet_id,
        });
        
        self.trigger_side_effect(&SwarmSideEffect::LocalNodeConfigured {
            node_id: self.node_id,
            subnet_id: self.subnet_map.get(&self.node_id).copied().unwrap_or(1),
            peers: self.nodes.values().cloned().filter(|n| n.id != self.node_id).collect(),
        });
        
        Ok(())
    }

    pub fn update_node_endpoint(&mut self, node_id: u64, addr: Option<String>, wireguard_key: Option<String>, wireguard_port: Option<u16>) -> Result<(), String> {
        let node = self.nodes.get_mut(&node_id).ok_or("Node not found")?;
        let old_subnet_id = node.subnet_id.unwrap_or(0);
        
        let mut new_addr_to_set = None;
        let mut new_wg_key_to_set = None;
        let mut new_wg_port_to_set = None;
        
        if let Some(ref new_addr) = addr {
            new_addr_to_set = Some(new_addr.clone());
            node.addr = new_addr.clone();
        }
        if let Some(ref key) = wireguard_key {
            new_wg_key_to_set = Some(key.clone());
            node.wireguard_key = Some(key.clone());
        }
        if let Some(port) = wireguard_port {
            new_wg_port_to_set = Some(port);
            node.wireguard_port = Some(port);
        }
        
        self.trigger_side_effect(&SwarmSideEffect::NodeUpdated {
            node_id,
            old_subnet_id,
            new_addr: new_addr_to_set,
            new_wireguard_key: new_wg_key_to_set,
            new_wireguard_port: new_wg_port_to_set,
        });
        
        Ok(())
    }
    
    pub fn submit_command(&mut self, cmd: SwarmCommand, command_index: u64) -> Result<(), String> {
        if !self.is_initialized {
            return Err("Cluster not initialized".to_string());
        }
        
        if self.applied_commands.contains(&command_index) {
            info!("Command {} already applied, skipping side effects (idempotent replay)", command_index);
            return Ok(());
        }
        
        info!("Processing command: {:?}", cmd);
        
        match cmd {
            SwarmCommand::AddNode { node_id, addr, public_key, wireguard_key, wireguard_port } => {
                self.add_node(node_id, addr, public_key, wireguard_key, wireguard_port)?;
            }
            SwarmCommand::RemoveNode { node_id } => {
                self.remove_node(node_id)?;
            }
            SwarmCommand::UpdateNodeEndpoint { node_id, addr, wireguard_key, wireguard_port } => {
                self.update_node_endpoint(node_id, addr, wireguard_key, wireguard_port)?;
            }
            SwarmCommand::RegisterNode { node_id, addr, public_key } => {
                self.add_node(node_id, addr, public_key, None, None)?;
            }
            SwarmCommand::DeregisterNode { node_id } => {
                self.remove_node(node_id)?;
            }
            SwarmCommand::UpdateVmPlacement { vm_id, node_id } => {
                let placement = VmPlacement { vm_id: vm_id.clone(), node_id };
                self.vm_placements.insert(vm_id, placement);
            }
            SwarmCommand::RemoveVmPlacement { vm_id } => {
                self.vm_placements.remove(&vm_id);
            }
            SwarmCommand::CreateService { name, spec } => {
                self.services.insert(name, spec);
            }
            SwarmCommand::UpdateService { name, spec } => {
                self.services.insert(name, spec);
            }
            SwarmCommand::DeleteService { name } => {
                self.services.remove(&name);
            }
        }
        
        self.applied_commands.push(command_index);
        if self.applied_commands.len() > 1000 {
            self.applied_commands.drain(0..500);
        }
        
        Ok(())
    }

    fn trigger_side_effect(&self, effect: &SwarmSideEffect) {
        if let Some(ref callback) = self.side_effect_callback {
            info!("Triggering side effect: {:?}", effect);
            callback(effect);
        }
    }
    
    pub fn get_nodes(&self) -> Vec<&NodeInfo> {
        self.nodes.values().collect()
    }
    
    pub fn get_node(&self, node_id: u64) -> Option<&NodeInfo> {
        self.nodes.get(&node_id)
    }
    
    pub fn get_leader(&self) -> Option<&NodeInfo> {
        self.nodes.values().find(|n| n.is_leader)
    }
    
    pub fn get_vm_placements(&self) -> Vec<&VmPlacement> {
        self.vm_placements.values().collect()
    }
    
    pub fn get_services(&self) -> Vec<(&String, &ServiceSpec)> {
        self.services.iter().collect()
    }
    
    pub fn get_service(&self, name: &str) -> Option<&ServiceSpec> {
        self.services.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bootstrap_cluster() {
        let mut raft = SwarmRaft::new(1);
        
        assert!(!raft.is_initialized());
        
        raft.bootstrap("10.0.0.1:7946".to_string(), "test_key".to_string(), None, None).unwrap();
        
        assert!(raft.is_initialized());
        assert_eq!(raft.get_nodes().len(), 1);
        assert!(raft.get_leader().is_some());
    }
    
    #[test]
    fn test_add_remove_node() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        let subnet = raft.add_node(2, "10.0.0.2:7946".to_string(), "node2_key".to_string(), None, None).unwrap();
        assert_eq!(subnet, 3);
        
        assert_eq!(raft.get_nodes().len(), 2);
        
        raft.remove_node(2).unwrap();
        
        assert_eq!(raft.get_nodes().len(), 1);
    }

    #[test]
    fn test_deterministic_subnet_allocation() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        let subnet1 = raft.add_node(100, "10.0.0.100:7946".to_string(), "key100".to_string(), None, None).unwrap();
        assert_eq!(subnet1, 100 % 254 + 1);
        
        let subnet2 = raft.add_node(200, "10.0.0.200:7946".to_string(), "key200".to_string(), None, None).unwrap();
        assert_eq!(subnet2, 200 % 254 + 1);
        
        assert_ne!(subnet1, subnet2, "Different nodes should have different subnets");
    }
    
    #[test]
    fn test_vm_placement() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key".to_string(), None, None).unwrap();
        
        let cmd = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm-123".to_string(),
            node_id: 1,
        };
        
        raft.submit_command(cmd, 1).unwrap();
        
        assert_eq!(raft.get_vm_placements().len(), 1);
    }
    
    #[test]
    fn test_service_management() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key".to_string(), None, None).unwrap();
        
        let spec = ServiceSpec {
            image: "nginx:latest".to_string(),
            replicas: 2,
            ports: vec![PortMapping { host: 80, vm: 80 }],
        };
        
        let cmd = SwarmCommand::CreateService {
            name: "web".to_string(),
            spec: spec.clone(),
        };
        
        raft.submit_command(cmd, 1).unwrap();
        
        assert_eq!(raft.get_services().len(), 1);
        assert!(raft.get_service("web").is_some());
        
        let delete_cmd = SwarmCommand::DeleteService {
            name: "web".to_string(),
        };
        
        raft.submit_command(delete_cmd, 2).unwrap();
        
        assert_eq!(raft.get_services().len(), 0);
    }

    #[test]
    fn test_add_node_with_wireguard() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), Some("wg_key_1".to_string()), Some(51820)).unwrap();
        
        let subnet = raft.add_node(
            2,
            "10.0.0.2:7946".to_string(),
            "node2_key".to_string(),
            Some("wg_key_2".to_string()),
            Some(51821)
        ).unwrap();
        
        let node2 = raft.get_node(2).unwrap();
        assert_eq!(node2.wireguard_key.as_deref(), Some("wg_key_2"));
        assert_eq!(node2.wireguard_port, Some(51821));
        assert_eq!(node2.subnet_id, Some(subnet));
    }

    #[test]
    fn test_update_node_endpoint() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        raft.add_node(2, "10.0.0.2:7946".to_string(), "node2_key".to_string(), None, None).unwrap();
        
        raft.update_node_endpoint(2, Some("10.0.0.22:7946".to_string()), Some("new_wg_key".to_string()), Some(51830)).unwrap();
        
        let node = raft.get_node(2).unwrap();
        assert_eq!(node.addr, "10.0.0.22:7946");
        assert_eq!(node.wireguard_key.as_deref(), Some("new_wg_key"));
        assert_eq!(node.wireguard_port, Some(51830));
    }

    #[test]
    fn test_remove_nonexistent_node() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        let result = raft.remove_node(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_duplicate_node() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        raft.add_node(2, "10.0.0.2:7946".to_string(), "node2_key".to_string(), None, None).unwrap();
        
        let result = raft.add_node(2, "10.0.0.2:7946".to_string(), "node2_key".to_string(), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_idempotent_command_replay() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string(), None, None).unwrap();
        
        let cmd = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm-replay".to_string(),
            node_id: 1,
        };
        
        raft.submit_command(cmd.clone(), 1).unwrap();
        assert_eq!(raft.get_vm_placements().len(), 1);
        
        let result = raft.submit_command(cmd.clone(), 1);
        assert!(result.is_ok(), "Idempotent replay should not fail");
        assert_eq!(raft.get_vm_placements().len(), 1, "Should still have only one placement");
        
        let result2 = raft.submit_command(cmd, 2);
        assert!(result2.is_ok(), "Different command index should work");
        assert_eq!(raft.get_vm_placements().len(), 1, "Still should have only one placement (key exists)");
    }
}
