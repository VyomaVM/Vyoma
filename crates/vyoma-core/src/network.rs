use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{info, warn};

pub struct NetworkManager;

impl NetworkManager {
    pub fn setup_bridge(name: &str, ip_cidr: &str) -> Result<()> {
        info!("Delegating to vyoma-net BridgeManager for bridge {}", name);
        
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let bm = vyoma_net::BridgeManager::new().await?;
            let idx = bm.create_bridge(name).await?;
            bm.set_up(name).await?;
            bm.set_ip(name, ip_cidr).await?;
            info!("Bridge {} created with index {}", name, idx);
            Ok(())
        })
    }

    pub fn setup_tap(tap_name: &str, bridge_name: &str) -> Result<()> {
        info!("Delegating to vyoma-net for TAP {}", tap_name);
        
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let tm = vyoma_net::TapManager::new().await?;
            tm.create_tap(tap_name).await?;
            tm.set_up(tap_name).await?;
            
            let bm = vyoma_net::BridgeManager::new().await?;
            bm.add_tap_to_bridge(tap_name, bridge_name).await?;
            
            info!("TAP {} attached to bridge {}", tap_name, bridge_name);
            Ok(())
        })
    }

    pub fn remove_interface(name: &str) -> Result<()> {
        info!("Removing network interface '{}'", name);
        
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
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
        })
    }

    pub fn setup_nat(bridge_cidr: &str) -> Result<()> {
        info!("Enabling NAT/Masquerade for source {}", bridge_cidr);
        
        let check_status = Command::new("iptables")
            .args(&[
                "-t", "nat", "-C", "POSTROUTING",
                "-s", bridge_cidr,
                "!", "-d", bridge_cidr,
                "-j", "MASQUERADE"
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        let exists = match check_status {
            Ok(s) => s.success(),
            Err(_) => false,
        };

        if !exists {
            let output = Command::new("iptables")
                .args(&[
                    "-t", "nat", "-A", "POSTROUTING",
                    "-s", bridge_cidr,
                    "!", "-d", bridge_cidr,
                    "-j", "MASQUERADE"
                ])
                .output()?;

            if !output.status.success() {
                 let stderr = String::from_utf8_lossy(&output.stderr);
                 return Err(anyhow!("Failed to setup iptables NAT. Output: {}", stderr));
            }
        } else {
            info!("NAT rule for {} already exists.", bridge_cidr);
        }

        let output = Command::new("sysctl")
            .args(&["-w", "net.ipv4.ip_forward=1"])
            .output()?;

        if !output.status.success() {
             let stderr = String::from_utf8_lossy(&output.stderr);
             warn!("Failed to enable ip_forward: {}", stderr);
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