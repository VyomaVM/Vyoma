use std::io::Cursor;
use openraft::RaftTypeConfig;
use serde::{Deserialize, Serialize};

pub type NodeId = u64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwarmNode {
    pub addr: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmRequest {
    AddNode {
        node_id: u64,
        addr: String,
        public_key: String,
        wireguard_key: Option<String>,
        wireguard_port: Option<u16>,
    },
    RemoveNode { node_id: u64 },
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeEndpoint {
    pub node_id: u64,
    pub addr: String,
    pub public_key: String,
    pub wireguard_key: Option<String>,
    pub wireguard_port: Option<u16>,
    pub subnet_id: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmResponse {
    pub success: bool,
}

openraft::declare_raft_types!(
    pub SwarmConfig:
        D = SwarmRequest,
        R = SwarmResponse,
        Node = SwarmNode,
);
