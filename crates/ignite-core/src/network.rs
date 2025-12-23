use anyhow::{anyhow, Result};
use std::process::Command;
use tracing::{info, warn};

pub struct NetworkManager;

impl NetworkManager {
    /// Creates a bridge interface and assigns it a static IP/CIDR.
    /// Example: name="ign0", ip_cidr="172.16.0.1/24"
    pub fn setup_bridge(name: &str, ip_cidr: &str) -> Result<()> {
        info!("Setting up bridge '{}' with IP {}", name, ip_cidr);

        // 1. Create bridge
        // sudo ip link add name <name> type bridge
        let status = Command::new("sudo")
            .args(&["ip", "link", "add", "name", name, "type", "bridge"])
            .status()?;
            
        if !status.success() {
             // It might already exist, which is fine, but let's check or fail.
             // For robustness, usually we check existence first or ignore "File exists"
             // But for now, let's treat failure as error (user can run cleanup).
             return Err(anyhow!("Failed to create bridge {}. Status: {}", name, status));
        }

        // 2. Assign IP
        // sudo ip addr add <cidr> dev <name>
        let status = Command::new("sudo")
            .args(&["ip", "addr", "add", ip_cidr, "dev", name])
            .status()?;
            
        if !status.success() {
            // Rollback?
            let _ = Self::remove_interface(name);
            return Err(anyhow!("Failed to assign IP to bridge. Status: {}", status));
        }

        // 3. Set UP
        // sudo ip link set dev <name> up
        let status = Command::new("sudo")
            .args(&["ip", "link", "set", "dev", name, "up"])
            .status()?;

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
        
        // sudo iptables -t nat -A POSTROUTING -s <cidr> ! -d <cidr> -j MASQUERADE
        // Note: The ! -d <cidr> prevents NAT for internal bridge traffic (optional but good practice)
        let status = Command::new("sudo")
            .args(&[
                "iptables", "-t", "nat", "-A", "POSTROUTING",
                "-s", bridge_cidr,
                "!", "-d", bridge_cidr,
                "-j", "MASQUERADE"
            ])
            .status()?;

        if !status.success() {
             return Err(anyhow!("Failed to setup iptables NAT. Status: {}", status));
        }

        // Enable IPv4 forwarding
        // sudo sysctl -w net.ipv4.ip_forward=1
        let status = Command::new("sudo")
            .args(&["sysctl", "-w", "net.ipv4.ip_forward=1"])
            .status()?;

        if !status.success() {
             warn!("Failed to enable ip_forward. Internet access might fail.");
        }

        Ok(())
    }
}
