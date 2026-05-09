use anyhow::{Context, Result};
use std::process::Command;
use tracing::{info, warn};

use super::types::{VmNetworkConfig, NetworkInfo};
use crate::state::AppState;

pub async fn setup_network(
    state: &AppState,
    vm_id: &str,
    networks: &[String],
) -> Result<VmNetworkConfig> {
    if state.rootless {
        setup_rootless_network()
    } else {
        setup_privileged_network(state, vm_id, networks).await
    }
}

fn setup_rootless_network() -> Result<VmNetworkConfig> {
    Ok(VmNetworkConfig {
        ip_address: "10.0.2.15".to_string(),
        primary_tap: "tap0".to_string(),
        gateway: String::new(),
        network_infos: vec![NetworkInfo {
            ip: "10.0.2.15".to_string(),
            tap_name: "tap0".to_string(),
            gateway: None,
            interface_name: "eth0".to_string(),
            network_name: "slirp".to_string(),
        }],
        netns_path: None,
    })
}

async fn setup_privileged_network(
    state: &AppState,
    vm_id: &str,
    networks: &[String],
) -> Result<VmNetworkConfig> {
    let netns_name = format!("vm-{}", vm_id);
    let netns_path = format!("/var/run/netns/{}", netns_name);

    std::fs::create_dir_all("/var/run/netns")
        .context("Failed to create /var/run/netns")?;

    let _ = Command::new("ip")
        .args(&["netns", "add", &netns_name])
        .output()
        .context("Failed to create netns")?;

    let networks_to_attach: Vec<String> = if networks.is_empty() {
        vec!["default".to_string()]
    } else {
        networks.to_vec()
    };

    let attachments = state
        .cni_manager
        .add_multiple(&networks_to_attach, vm_id, &netns_path)
        .context("CNI ADD failed")?;

    let mut network_infos = Vec::new();
    for (idx, attachment) in attachments.iter().enumerate() {
        info!(
            "CNI: Attached {} to network '{}' with IP {}",
            attachment.interface_name, attachment.network_name, attachment.result.ip_address
        );

        let tap_name = if idx == 0 {
            format!("tap{}", &vm_id[0..8])
        } else {
            format!("tap{}-{}", &vm_id[0..8], idx)
        };

        network_infos.push(NetworkInfo {
            ip: attachment.result.ip_address.clone(),
            tap_name,
            gateway: attachment.result.gateway.clone(),
            interface_name: attachment.interface_name.clone(),
            network_name: attachment.network_name.clone(),
        });
    }

    if network_infos.is_empty() {
        anyhow::bail!("No networks could be attached to VM");
    }

    let ip_address = network_infos.first().map(|n| n.ip.clone())
        .unwrap_or_else(|| "10.0.2.15".to_string());
    let primary_tap = network_infos.first().map(|n| n.tap_name.clone())
        .unwrap_or_else(|| "tap0".to_string());
    let gateway = network_infos.first()
        .and_then(|n| n.gateway.clone())
        .unwrap_or_else(|| "172.16.0.1".to_string());

    Ok(VmNetworkConfig {
        ip_address,
        primary_tap,
        gateway,
        network_infos,
        netns_path: Some(netns_path),
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
        
        let _ = Command::new("ip")
            .args(&["netns", "delete", &netns_name])
            .output();
    }
    Ok(())
}