use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMessage {
    pub message_type: MessageType,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    Handshake,
    Header,
    Pages,
    DirtyPages,
    Snapshot,
    Signal,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRequest {
    pub vm_id: String,
    pub source_node: String,
    pub dest_node: String,
    pub memory_mb: u64,
    pub bandwidth_limit_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResponse {
    pub accepted: bool,
    pub dest_ip: Ipv4Addr,
    pub dest_port: u16,
    pub message: Option<String>,
}

impl MigrationMessage {
    pub fn new(message_type: MessageType, payload: Vec<u8>) -> Self {
        Self {
            message_type,
            payload,
        }
    }

    pub fn handshake(source_node: &str) -> Self {
        Self::new(MessageType::Handshake, source_node.as_bytes().to_vec())
    }

    pub fn pages(pages: &[u8]) -> Self {
        Self::new(MessageType::Pages, pages.to_vec())
    }

    pub fn signal(signal: &str) -> Self {
        Self::new(MessageType::Signal, signal.as_bytes().to_vec())
    }
}

impl MigrationRequest {
    pub fn new(vm_id: String, source_node: String, dest_node: String, memory_mb: u64) -> Self {
        Self {
            vm_id,
            source_node,
            dest_node,
            memory_mb,
            bandwidth_limit_mbps: None,
        }
    }

    pub fn with_bandwidth_limit(mut self, limit_mbps: u32) -> Self {
        self.bandwidth_limit_mbps = Some(limit_mbps);
        self
    }
}

impl MigrationResponse {
    pub fn accepted(dest_ip: Ipv4Addr, dest_port: u16) -> Self {
        Self {
            accepted: true,
            dest_ip,
            dest_port,
            message: None,
        }
    }

    pub fn rejected(message: String) -> Self {
        Self {
            accepted: false,
            dest_ip: Ipv4Addr::new(0, 0, 0, 0),
            dest_port: 0,
            message: Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_message_creation() {
        let msg = MigrationMessage::new(MessageType::Handshake, b"node1".to_vec());
        assert_eq!(msg.message_type, MessageType::Handshake);
    }

    #[test]
    fn test_migration_request() {
        let req = MigrationRequest::new(
            "vm-123".to_string(),
            "node-1".to_string(),
            "node-2".to_string(),
            1024,
        );
        assert_eq!(req.vm_id, "vm-123");
        assert_eq!(req.memory_mb, 1024);
    }

    #[test]
    fn test_migration_request_with_bandwidth() {
        let req = MigrationRequest::new(
            "vm-123".to_string(),
            "node-1".to_string(),
            "node-2".to_string(),
            1024,
        )
        .with_bandwidth_limit(100);

        assert_eq!(req.bandwidth_limit_mbps, Some(100));
    }

    #[test]
    fn test_migration_response_accepted() {
        let resp = MigrationResponse::accepted(Ipv4Addr::new(192, 168, 1, 100), 9000);
        assert!(resp.accepted);
        assert_eq!(resp.dest_ip, Ipv4Addr::new(192, 168, 1, 100));
        assert_eq!(resp.dest_port, 9000);
    }

    #[test]
    fn test_migration_response_rejected() {
        let resp = MigrationResponse::rejected("Insufficient memory".to_string());
        assert!(!resp.accepted);
        assert!(resp.message.is_some());
    }
}
