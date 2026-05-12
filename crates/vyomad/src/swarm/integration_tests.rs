#[cfg(test)]
mod tests {
    use crate::swarm::{SwarmRaft, SwarmCommand, SwarmSideEffect};
    use std::sync::{Arc, Mutex};
    use std::collections::VecDeque;

    struct TestNetworkOps {
        events: Arc<Mutex<VecDeque<String>>>,
    }

    impl TestNetworkOps {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(VecDeque::new())),
            }
        }

        fn record(&self, event: String) {
            self.events.lock().unwrap().push_back(event);
        }

        fn get_events(&self) -> Vec<String> {
            self.events.lock().unwrap().iter().cloned().collect()
        }

        fn create_callback(self: Arc<Self>) -> Box<dyn Fn(&SwarmSideEffect) + Send + Sync> {
            let events = self.events.clone();
            Box::new(move |effect| {
                match effect {
                    SwarmSideEffect::LocalNodeConfigured { node_id, subnet_id, peers } => {
                        events.lock().unwrap().push_back(format!(
                            "local_configured: node={}, subnet=10.42.{}.0/24, peers={}",
                            node_id, subnet_id, peers.len()
                        ));
                    }
                    SwarmSideEffect::NodeAdded { node_id, addr, wireguard_key, wireguard_port, subnet_id } => {
                        let wg_info = match (wireguard_key, wireguard_port) {
                            (Some(k), Some(p)) => format!(", wg_key={}, wg_port={}", k, p),
                            _ => String::new(),
                        };
                        events.lock().unwrap().push_back(format!(
                            "node_added: id={}, addr={}, subnet=10.42.{}.0/24{}",
                            node_id, addr, subnet_id, wg_info
                        ));
                    }
                    SwarmSideEffect::NodeRemoved { node_id, subnet_id } => {
                        events.lock().unwrap().push_back(format!(
                            "node_removed: id={}, subnet=10.42.{}.0/24",
                            node_id, subnet_id
                        ));
                    }
                    SwarmSideEffect::NodeUpdated { node_id, old_subnet_id, new_addr, .. } => {
                        let addr_info = new_addr.as_ref().map(|a| format!(", new_addr={}", a)).unwrap_or_default();
                        events.lock().unwrap().push_back(format!(
                            "node_updated: id={}, old_subnet=10.42.{}.0/24{}",
                            node_id, old_subnet_id, addr_info
                        ));
                    }
                }
            })
        }
    }

    #[test]
    fn test_swarm_raft_basic_lifecycle() {
        let test_ops = Arc::new(TestNetworkOps::new());
        let callback = test_ops.clone().create_callback();

        let mut raft = SwarmRaft::new(1);
        raft.set_side_effect_callback(callback);

        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), Some("wg_key1".to_string()), Some(51820)).unwrap();
        
        let events = test_ops.get_events();
        assert!(events.iter().any(|e| e.contains("local_configured: node=1")), "Should trigger local config on bootstrap");
        
        assert!(raft.is_initialized());
        assert_eq!(raft.get_nodes().len(), 1);
        assert!(raft.get_leader().is_some());
        
        let node = raft.get_node(1).unwrap();
        assert_eq!(node.wireguard_key.as_deref(), Some("wg_key1"));
        assert_eq!(node.wireguard_port, Some(51820));
        assert_eq!(node.subnet_id, Some(2));
    }

    #[test]
    fn test_swarm_raft_add_remove_nodes() {
        let test_ops = Arc::new(TestNetworkOps::new());
        let callback = test_ops.clone().create_callback();

        let mut raft = SwarmRaft::new(1);
        raft.set_side_effect_callback(callback);

        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        raft.add_node(2, "10.0.0.2:7946".to_string(), "key2".to_string(), Some("wg_key2".to_string()), Some(51821)).unwrap();
        
        let events = test_ops.get_events();
        assert!(events.iter().any(|e| e.contains("node_added: id=2")), "Should trigger node added event");
        
        assert_eq!(raft.get_nodes().len(), 2);
        
        let node2 = raft.get_node(2).unwrap();
        assert_eq!(node2.subnet_id, Some(3));
        assert_eq!(node2.wireguard_key.as_deref(), Some("wg_key2"));
        
        raft.remove_node(2).unwrap();
        
        let events = test_ops.get_events();
        assert!(events.iter().any(|e| e.contains("node_removed: id=2")), "Should trigger node removed event");
        
        assert_eq!(raft.get_nodes().len(), 1);
    }

    #[test]
    fn test_swarm_raft_update_node_endpoint() {
        let test_ops = Arc::new(TestNetworkOps::new());
        let callback = test_ops.clone().create_callback();

        let mut raft = SwarmRaft::new(1);
        raft.set_side_effect_callback(callback);

        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();
        raft.add_node(2, "10.0.0.2:7946".to_string(), "key2".to_string(), Some("wg_key2".to_string()), Some(51821)).unwrap();

        let events_before = test_ops.get_events().len();

        raft.update_node_endpoint(2, Some("10.0.0.22:7946".to_string()), Some("new_wg_key".to_string()), Some(51830)).unwrap();
        
        let events = test_ops.get_events();
        assert!(events.len() > events_before, "Should trigger update event");
        assert!(events.iter().any(|e| e.contains("node_updated: id=2")), "Should contain node_updated event");

        let node = raft.get_node(2).unwrap();
        assert_eq!(node.addr, "10.0.0.22:7946");
        assert_eq!(node.wireguard_key.as_deref(), Some("new_wg_key"));
        assert_eq!(node.wireguard_port, Some(51830));
    }

    #[test]
    fn test_swarm_raft_deterministic_subnet_allocation() {
        let test_ops = Arc::new(TestNetworkOps::new());
        let callback = test_ops.clone().create_callback();

        let mut raft = SwarmRaft::new(1);
        raft.set_side_effect_callback(callback);

        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        let subnets: Vec<u8> = (2..=10)
            .map(|id| {
                let subnet = raft.add_node(id, format!("10.0.0.{}:7946", id), format!("key{}", id), None, None).unwrap();
                subnet
            })
            .collect();

        for (i, &subnet) in subnets.iter().enumerate() {
            let expected = ((i + 2) % 254 + 1) as u8;
            assert_eq!(subnet, expected, "Node {} should have subnet {}, got {}", i + 2, expected, subnet);
        }
    }

    #[test]
    fn test_swarm_raft_idempotent_command_processing() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        let cmd = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm-123".to_string(),
            node_id: 1,
        };

        raft.submit_command(cmd.clone(), 1).unwrap();
        assert_eq!(raft.get_vm_placements().len(), 1);

        let result = raft.submit_command(cmd.clone(), 1);
        assert!(result.is_ok(), "Duplicate command should succeed (idempotent)");
        assert_eq!(raft.get_vm_placements().len(), 1, "Should not create duplicate");

        let cmd2 = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm-456".to_string(),
            node_id: 1,
        };
        raft.submit_command(cmd2, 2).unwrap();
        assert_eq!(raft.get_vm_placements().len(), 2);
    }

    #[test]
    fn test_swarm_raft_duplicate_node_rejected() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        raft.add_node(2, "10.0.0.2:7946".to_string(), "key2".to_string(), None, None).unwrap();
        
        let result = raft.add_node(2, "10.0.0.2:7946".to_string(), "key2".to_string(), None, None);
        assert!(result.is_err(), "Should reject duplicate node ID");
    }

    #[test]
    fn test_swarm_raft_cannot_remove_self() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        let result = raft.remove_node(1);
        assert!(result.is_err(), "Should not allow removing self");
    }

    #[test]
    fn test_swarm_raft_service_management() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        let spec = crate::swarm::ServiceSpec {
            image: "nginx:latest".to_string(),
            replicas: 3,
             ports: vec![
                 vyoma_core::api::PortMapping { host_port: 80, vm_port: 80 },
                 vyoma_core::api::PortMapping { host_port: 443, vm_port: 443 },
             ],
        };

        let create_cmd = SwarmCommand::CreateService {
            name: "web".to_string(),
            spec: spec.clone(),
        };
        raft.submit_command(create_cmd, 1).unwrap();

        assert_eq!(raft.get_services().len(), 1);
        let services = raft.get_services();
        let (name, retrieved_spec) = services.first().unwrap();
        assert_eq!(name.as_str(), "web");
        assert_eq!(retrieved_spec.replicas, 3);

        let update_cmd = SwarmCommand::UpdateService {
            name: "web".to_string(),
            spec: crate::swarm::ServiceSpec {
                image: "nginx:1.21".to_string(),
                replicas: 5,
                ports: vec![],
            },
        };
        raft.submit_command(update_cmd, 2).unwrap();

        let updated = raft.get_service("web").unwrap();
        assert_eq!(updated.replicas, 5);

        let delete_cmd = SwarmCommand::DeleteService {
            name: "web".to_string(),
        };
        raft.submit_command(delete_cmd, 3).unwrap();

        assert!(raft.get_service("web").is_none());
    }

    #[test]
    fn test_swarm_raft_vm_placement() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        for i in 0..4 {
            let cmd = SwarmCommand::UpdateVmPlacement {
                vm_id: format!("vm-{}", i),
                node_id: 1,
            };
            raft.submit_command(cmd, (i + 1) as u64).unwrap();
        }

        assert_eq!(raft.get_vm_placements().len(), 4);

        raft.submit_command(
            SwarmCommand::RemoveVmPlacement { vm_id: "vm-1".to_string() },
            5
        ).unwrap();

        assert_eq!(raft.get_vm_placements().len(), 3);
        assert!(raft.get_vm_placements().iter().all(|p| p.vm_id != "vm-1"));
    }

    #[test]
    fn test_swarm_raft_not_initialized_error() {
        let mut raft = SwarmRaft::new(1);

        let result = raft.add_node(2, "10.0.0.2:7946".to_string(), "key2".to_string(), None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not initialized"));

        let cmd = SwarmCommand::UpdateVmPlacement {
            vm_id: "vm".to_string(),
            node_id: 1,
        };
        let result = raft.submit_command(cmd, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_swarm_raft_remove_nonexistent_node() {
        let mut raft = SwarmRaft::new(1);
        raft.bootstrap("10.0.0.1:7946".to_string(), "key1".to_string(), None, None).unwrap();

        let result = raft.remove_node(999);
        assert!(result.is_err());
    }
}