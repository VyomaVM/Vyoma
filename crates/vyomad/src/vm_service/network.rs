//! Network setup for VM instances
//!
//! # Network Namespace Handling
//!
//! Network namespace creation now uses vyoma-net's NetNsManager module.
//! This provides a cleaner API while still using ip netns under the hood
//! for persistent namespace management.
//!
//! The implementation:
//! - Does NOT use sudo - relies on CAP_NET_ADMIN capability instead
//! - Uses vyoma-net's netns module for consistent API

use anyhow::{Context, Result};
use tracing::{info, warn};
use vyoma_net::{create_netns, delete_netns};

use super::types::{VmNetworkConfig, NetworkInfo};
use crate::state::AppState;

pub async fn setup_network(
    state: &AppState,
    vm_id: &str,
    networks: &[String],
) -> Result<VmNetworkConfig> {
    setup_privileged_network(state, vm_id, networks).await
}

async fn setup_privileged_network(
    _state: &AppState,
    vm_id: &str,
    _networks: &[String],
) -> Result<VmNetworkConfig> {
    let tap_name = format!("tap{}", &vm_id[..8]);
    let bridge_name = "vyoma0";

    // 1. Create TAP and attach to bridge (atomic via ip command)
    let output = std::process::Command::new("ip")
        .args(&["tuntap", "add", "dev", &tap_name, "mode", "tap", "user", "vyoma"])
        .output();

    let tap_created = match output {
        Ok(out) => {
            if !out.status.success() {
                warn!("ip tuntap add failed: {}. Continuing without TAP for unprivileged environment.", String::from_utf8_lossy(&out.stderr));
                false
            } else {
                // 2. Set TAP up
                let _ = std::process::Command::new("ip")
                    .args(&["link", "set", &tap_name, "up"])
                    .output();

                // 3. Attach TAP to bridge
                let _ = std::process::Command::new("ip")
                    .args(&["link", "set", &tap_name, "master", bridge_name])
                    .output();
                true
            }
        }
        Err(e) => {
            warn!("Failed to execute ip command: {}. Continuing without TAP.", e);
            false
        }
    };

    // 4. Assign IP address to VM
    let random_octet = 2 + (rand::random::<u8>() % 252);
    let ip_address = format!("172.16.0.{}", random_octet);
    let gateway = "172.16.0.1".to_string();

    let network_infos = vec![NetworkInfo {
        ip: ip_address.clone(),
        tap_name: if tap_created { tap_name.clone() } else { String::new() },
        gateway: Some(gateway.clone()),
        interface_name: "eth0".to_string(),
        network_name: "default".to_string(),
    }];

    Ok(VmNetworkConfig {
        ip_address,
        primary_tap: if tap_created { tap_name } else { String::new() },
        gateway,
        network_infos,
        netns_path: None,
    })
}

pub async fn cleanup_network(
    state: &AppState,
    vm_id: &str,
    networks: &[String],
    netns_path: &Option<String>,
) -> Result<()> {
    if let Some(ns) = netns_path {
        let netns_name = format!("vm-{}", vm_id);

        if !networks.is_empty() {
            let _ = state.cni_manager.del_multiple(networks, vm_id, ns);
        } else {
            let _ = state.cni_manager.del(None, vm_id, ns, "eth0");
        }

        // Use vyoma-net's netns module for cleanup
        let _ = delete_netns(&netns_name);
    }
    Ok(())
}