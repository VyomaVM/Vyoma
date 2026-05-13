//! Network namespace management for Vyoma
//!
//! This module provides network namespace operations.
//! Currently uses 'ip netns' command but provides a cleaner API.

use std::path::Path;
use std::process::Command;
use tracing::{info, warn, error};

pub struct NetNsManager;

impl NetNsManager {
    /// Check if a network namespace exists
    pub fn exists(ns_path: &Path) -> bool {
        ns_path.exists()
    }

    /// Get the path to a network namespace
    pub fn ns_path(name: &str) -> String {
        format!("/var/run/netns/{}", name)
    }
}

/// Create a network namespace
pub fn create_netns(name: &str) -> Result<(), String> {
    info!("Creating network namespace: {}", name);

    // Create /var/run/netns if it doesn't exist
    if let Err(e) = std::fs::create_dir_all("/var/run/netns") {
        return Err(format!("Failed to create /var/run/netns: {}", e));
    }

    // Use ip netns add to create the namespace
    let output = Command::new("ip")
        .args(&["netns", "add", name])
        .output()
        .map_err(|e| format!("Failed to execute ip netns: {}", e))?;

    if output.status.success() {
        info!("Network namespace {} created successfully", name);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check if it already exists
        if stderr.contains("File exists") || stderr.contains("17") {
            info!("Network namespace {} already exists", name);
            Ok(())
        } else {
            error!("Failed to create network namespace: {}", stderr);
            Err(format!("Failed to create network namespace: {}", stderr))
        }
    }
}

/// Delete a network namespace
pub fn delete_netns(name: &str) -> Result<(), String> {
    info!("Deleting network namespace: {}", name);

    let output = Command::new("ip")
        .args(&["netns", "del", name])
        .output()
        .map_err(|e| format!("Failed to execute ip netns del: {}", e))?;

    if output.status.success() {
        info!("Network namespace {} deleted successfully", name);
        Ok(())
    } else {
        // Namespace might not exist - that's okay
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such file") || stderr.contains("Operation not permitted") {
            warn!("Network namespace {} may not exist: {}", name, stderr);
            Ok(())
        } else {
            error!("Failed to delete network namespace: {}", stderr);
            Err(format!("Failed to delete network namespace: {}", stderr))
        }
    }
}