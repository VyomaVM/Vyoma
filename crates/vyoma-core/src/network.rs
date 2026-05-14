use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{info, warn};

pub struct NetworkManager;

impl NetworkManager {
    pub async fn setup_bridge(name: &str, ip_cidr: &str) -> Result<()> {
        info!("Delegating to vyoma-net BridgeManager for bridge {}", name);
        let bm = vyoma_net::BridgeManager::new().await?;
        let idx = bm.create_bridge(name).await?;
        bm.set_up(name).await?;
        bm.set_ip(name, ip_cidr).await?;
        info!("Bridge {} created with index {}", name, idx);
        Ok(())
    }

    pub async fn setup_tap(tap_name: &str, bridge_name: &str) -> Result<()> {
        info!("Delegating to vyoma-net for TAP {}", tap_name);
        let tm = vyoma_net::TapManager::new().await?;
        tm.create_tap(tap_name).await?;
        tm.set_up(tap_name).await?;
        let bm = vyoma_net::BridgeManager::new().await?;
        bm.add_tap_to_bridge(tap_name, bridge_name).await?;
        info!("TAP {} attached to bridge {}", tap_name, bridge_name);
        Ok(())
    }

    pub async fn remove_interface(name: &str) -> Result<()> {
        info!("Removing network interface '{}'", name);
        let bm = vyoma_net::BridgeManager::new().await?;
        match bm.delete_bridge(name).await {
            Ok(_) => {
                info!("Bridge {} deleted", name);
                Ok(())
            }
            Err(_) => {
                let tm = vyoma_net::TapManager::new().await?;
                tm.delete_tap(name).await?;
                info!("Interface {} deleted", name);
                Ok(())
            }
        }
    }

    pub fn setup_nat(bridge_cidr: &str) -> Result<()> {
        info!("Enabling NAT/Masquerade for source {}", bridge_cidr);

        // Enable IP forwarding by writing to procfs (native approach)
        if let Err(e) = std::fs::write("/proc/sys/net/ipv4/ip_forward", "1\n") {
            warn!("Failed to enable ip_forward via procfs: {}", e);
        }

        // Use native iptables crate instead of CLI
        let ip = match iptables::new(false) {
            Ok(ip) => ip,
            Err(e) => return Err(anyhow!("Failed to initialize iptables: {}", e)),
        };

        // Check if MASQUERADE rule already exists using the exists() method
        let rule = format!("-s {} ! -d {} -j MASQUERADE", bridge_cidr, bridge_cidr);
        let rule_exists = ip.exists("nat", "POSTROUTING", &rule).unwrap_or(false);

        if !rule_exists {
            // Add MASQUERADE rule for traffic from the bridge network going to other networks
            if let Err(e) = ip.append("nat", "POSTROUTING", &rule) {
                return Err(anyhow!("Failed to setup NAT: {}", e));
            }
            info!("NAT rule added for {}", bridge_cidr);
        } else {
            info!("NAT rule for {} already exists.", bridge_cidr);
        }

        Ok(())
    }

    pub fn setup_tc_redirect(if_ingress: &str, if_egress: &str) -> Result<()> {
        info!("Setting up TC redirect: {} <-> {}", if_ingress, if_egress);

        let run_tc = |args: &[&str]| -> Result<()> {
            let out = Command::new("tc")
                .args(args)
                .output()?;
            if !out.status.success() {
                 let stderr = String::from_utf8_lossy(&out.stderr);
                 if !stderr.contains("File exists") {
                     return Err(anyhow!("TC failed ({:?}): {}", args, stderr));
                 }
             }
             Ok(())
         };

        run_tc(&["qdisc", "add", "dev", if_ingress, "ingress"])?;
        run_tc(&["filter", "add", "dev", if_ingress, "parent", "ffff:", "protocol", "all", "u32", "match", "u32", "0", "0", "action", "mirred", "egress", "redirect", "dev", if_egress])?;

        run_tc(&["qdisc", "add", "dev", if_egress, "ingress"])?;
        run_tc(&["filter", "add", "dev", if_egress, "parent", "ffff:", "protocol", "all", "u32", "match", "u32", "0", "0", "action", "mirred", "egress", "redirect", "dev", if_ingress])?;

        Ok(())
    }
}