use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{info, warn};

pub struct NetworkManager;

impl NetworkManager {
    /// Creates a bridge interface and assigns it a static IP/CIDR.
    /// Example: name="ign0", ip_cidr="172.16.0.1/24"
    pub fn setup_bridge(name: &str, ip_cidr: &str) -> Result<()> {
        info!("Setting up bridge '{}' with IP {}", name, ip_cidr);

        // 0. Check if already exists
        // sudo ip link show <name>
        let check_status = Command::new("sudo")
             .args(&["ip", "link", "show", name])
             .stdout(std::process::Stdio::null())
             .stderr(std::process::Stdio::null())
             .status();

        let exists = match check_status {
            Ok(s) => s.success(),
            Err(_) => false,
        };

        if !exists {
            // 1. Create bridge
            // sudo ip link add name <name> type bridge
            let status = Command::new("sudo")
                .args(&["ip", "link", "add", "name", name, "type", "bridge"])
                .status()?;
                
            if !status.success() {
                 return Err(anyhow!("Failed to create bridge {}. Status: {}", name, status));
            }
        } else {
             info!("Bridge {} already exists, skipping creation.", name);
        }

        // 2. Assign IP
        // Check if IP is already assigned: ip addr show <name> | grep <ip>
        // Getting exact match is tricky with shell commands.
        // Alternative: Just try `ip addr add` and ignore "File exists" (exit code 2).
        
        let status = Command::new("sudo")
            .args(&["ip", "addr", "add", ip_cidr, "dev", name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
            
        if !status.success() {
             // If exit code is 2 (or 254 on some systems), it usually means "File exists" (IP exists)
             // We can check the code or just verify if the IP is there.
             // Let's verify.
             let check = Command::new("ip")
                .args(&["addr", "show", "dev", name])
                .output()?;
             let output = String::from_utf8_lossy(&check.stdout);
             // ip_cidr is like "172.16.0.1/24"
             // verification: just check for the IP part "172.16.0.1"
             let ip_only = ip_cidr.split('/').next().unwrap_or(ip_cidr);
             
             if !output.contains(ip_only) {
                  // Real failure
                  let _ = Self::remove_interface(name);
                  return Err(anyhow!("Failed to assign IP to bridge. Status: {}", status));
             } else {
                 info!("IP {} already assigned to {}", ip_only, name);
             }
        }

        // 3. Set UP
        // sudo ip link set dev <name> up
        let status = Command::new("sudo")
            .args(&["ip", "link", "set", "dev", name, "up"])
            .status()?;
            
        // ... remainder ...

        if !status.success() {
             let _ = Self::remove_interface(name);
             return Err(anyhow!("Failed to set bridge UP. Status: {}", status));
        }

        Ok(())
    }

    /// Creates a TAP interface and attaches it to the master bridge.
    pub fn setup_tap(tap_name: &str, master_bridge: &str) -> Result<()> {
        info!("Creating TAP '{}' attached to '{}'", tap_name, master_bridge);
        
        // 1. Create TAP
        // sudo ip tuntap add dev <tap_name> mode tap
        let status = Command::new("sudo")
            .args(&["ip", "tuntap", "add", "dev", tap_name, "mode", "tap"])
            .status()?;

        if !status.success() {
             return Err(anyhow!("Failed to create TAP {}. Status: {}", tap_name, status));
        }

        // 2. Attach to Bridge
        // sudo ip link set dev <tap_name> master <master_bridge>
        let status = Command::new("sudo")
            .args(&["ip", "link", "set", "dev", tap_name, "master", master_bridge])
            .status()?;

        if !status.success() {
             let _ = Self::remove_interface(tap_name);
             return Err(anyhow!("Failed to attach TAP to bridge. Status: {}", status));
        }

        // 3. Set UP
        let status = Command::new("sudo")
            .args(&["ip", "link", "set", "dev", tap_name, "up"])
            .status()?;

        if !status.success() {
             let _ = Self::remove_interface(tap_name);
             return Err(anyhow!("Failed to set TAP UP. Status: {}", status));
        }

        Ok(())
    }

    /// Generic removal of an interface (Bridge or TAP).
    pub fn remove_interface(name: &str) -> Result<()> {
        info!("Removing network interface '{}'", name);
        // sudo ip link delete <name>
        let status = Command::new("sudo")
            .args(&["ip", "link", "delete", name])
            .status()?;

        if !status.success() {
             // Warn but don't error hard, maybe it's already gone
             warn!("Failed to delete interface {}. Status: {}", name, status);
        }
        Ok(())
    }

    /// Sets up NAT (Masquerade) for traffic leaving the bridge subnet.
    /// This allows VMs to access the internet.
    /// bridge_cidr: e.g., "172.16.0.0/24"
    /// warning: This modifies global iptables.
    pub fn setup_nat(bridge_cidr: &str) -> Result<()> {
        info!("Enabling NAT/Masquerade for source {}", bridge_cidr);
        
        // Check if rule exists: -C (Check)
        let check_status = Command::new("sudo")
            .args(&[
                "iptables", "-t", "nat", "-C", "POSTROUTING",
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
            let output = Command::new("sudo")
                .args(&[
                    "iptables", "-t", "nat", "-A", "POSTROUTING",
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

        // Enable IPv4 forwarding
        // sudo sysctl -w net.ipv4.ip_forward=1
        let output = Command::new("sudo")
            .args(&["sysctl", "-w", "net.ipv4.ip_forward=1"])
            .output()?;

        if !output.status.success() {
             let stderr = String::from_utf8_lossy(&output.stderr);
             warn!("Failed to enable ip_forward: {}", stderr);
        }

        Ok(())
    }
}
