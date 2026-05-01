use std::collections::BTreeMap;
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmCommand {
    RegisterNode { node_id: u64, addr: String, public_key: String },
    DeregisterNode { node_id: u64 },
    UpdateVmPlacement { vm_id: String, node_id: u64 },
    RemoveVmPlacement { vm_id: String },
    CreateService { name: String, spec: ServiceSpec },
    UpdateService { name: String, spec: ServiceSpec },
    DeleteService { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub image: String,
    pub replicas: u32,
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host: u16,
    pub vm: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: u64,
    pub addr: String,
    pub public_key: String,
    pub is_leader: bool,
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
    is_initialized: bool,
}

impl SwarmRaft {
    pub fn new(node_id: u64) -> Self {
        info!("Creating SwarmRaft node {}", node_id);
        
        Self {
            node_id,
            nodes: BTreeMap::new(),
            vm_placements: BTreeMap::new(),
            services: BTreeMap::new(),
            is_initialized: false,
        }
    }
    
    pub fn node_id(&self) -> u64 {
        self.node_id
    }
    
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }
    
    pub fn bootstrap(&mut self, addr: String, public_key: String) -> Result<(), String> {
        if self.is_initialized {
            return Err("Already initialized".to_string());
        }
        
        info!("Bootstrapping cluster with node {} at {}", self.node_id, addr);
        
        let node = NodeInfo {
            id: self.node_id,
            addr,
            public_key,
            is_leader: true,
        };
        
        self.nodes.insert(self.node_id, node);
        self.is_initialized = true;
        
        Ok(())
    }
    
    pub fn add_node(&mut self, node_id: u64, addr: String, public_key: String) -> Result<(), String> {
        if !self.is_initialized {
            return Err("Cluster not initialized".to_string());
        }
        
        info!("Adding node {} at {} to cluster", node_id, addr);
        
        let node = NodeInfo {
            id: node_id,
            addr,
            public_key,
            is_leader: false,
        };
        
        self.nodes.insert(node_id, node);
        
        Ok(())
    }
    
    pub fn remove_node(&mut self, node_id: u64) -> Result<(), String> {
        if node_id == self.node_id {
            return Err("Cannot remove self".to_string());
        }
        
        info!("Removing node {} from cluster", node_id);
        
        self.nodes.remove(&node_id);
        
        // Remove VM placements for this node
        self.vm_placements.retain(|_, p| p.node_id != node_id);
        
        Ok(())
    }
    
    pub fn submit_command(&mut self, cmd: SwarmCommand) -> Result<(), String> {
        if !self.is_initialized {
            return Err("Cluster not initialized".to_string());
        }
        
        info!("Processing command: {:?}", cmd);
        
        match cmd {
            SwarmCommand::RegisterNode { node_id, addr, public_key } => {
                self.add_node(node_id, addr, public_key)?;
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
        
        Ok(())
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
        
        raft.bootstrap("10.0.0.1:7946".to_string(), "test_key".to_string()).unwrap();
        
        assert!(raft.is_initialized());
        assert_eq!(raft.get_nodes().len(), 1);
        assert!(raft.get_leader().is_some());
    }
    
    #[test]
    fn test_add_remove_node() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "leader_key".to_string()).unwrap();
        
        raft.add_node(2, "10.0.0.2:7946".to_string(), "node2_key".to_string()).unwrap();
        
        assert_eq!(raft.get_nodes().len(), 2);
        
        raft.remove_node(2).unwrap();
        
        assert_eq!(raft.get_nodes().len(), 1);
    }
    
    #[test]
    fn test_vm_placement() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key".to_string()).unwrap();
        
        let cmd = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm-123".to_string(),
            node_id: 1,
        };
        
        raft.submit_command(cmd).unwrap();
        
        assert_eq!(raft.get_vm_placements().len(), 1);
    }
    
    #[test]
    fn test_service_management() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key".to_string()).unwrap();
        
        let spec = ServiceSpec {
            image: "nginx:latest".to_string(),
            replicas: 2,
            ports: vec![PortMapping { host: 80, vm: 80 }],
        };
        
        let cmd = SwarmCommand::CreateService {
            name: "web".to_string(),
            spec: spec.clone(),
        };
        
        raft.submit_command(cmd).unwrap();
        
        assert_eq!(raft.get_services().len(), 1);
        assert!(raft.get_service("web").is_some());
        
        let delete_cmd = SwarmCommand::DeleteService {
            name: "web".to_string(),
        };
        
        raft.submit_command(delete_cmd).unwrap();
        
        assert_eq!(raft.get_services().len(), 0);
    }
}
