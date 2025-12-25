use anyhow::{Result, anyhow, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{info, debug};

#[derive(Serialize, Deserialize, Debug)]
pub struct CniConfig {
    pub cni_version: String,
    pub name: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    #[serde(flatten)]
    pub args: HashMap<String, serde_json::Value>,
}

pub struct CniManager {
    plugin_path: PathBuf,
    config_dir: PathBuf,
}

impl CniManager {
    pub fn new(plugin_path: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            plugin_path,
            config_dir,
        }
    }

    /// Executed CNI ADD command.
    /// 
    /// # Arguments
    /// * `container_id`: Unique ID of the VM/Container
    /// * `netns`: Path to the network namespace (e.g. /var/run/netns/vm-123)
    /// * `ifname`: Interface name inside the container (e.g. eth0)
    pub fn add(&self, container_id: &str, netns: &str, ifname: &str) -> Result<()> {
        self.exec("ADD", container_id, netns, ifname)
    }

    /// Executed CNI DEL command.
    pub fn del(&self, container_id: &str, netns: &str, ifname: &str) -> Result<()> {
        // CNI DEL should be best-effort, but we return error if it fails hard.
        self.exec("DEL", container_id, netns, ifname)
    }

    fn exec(&self, command: &str, container_id: &str, netns: &str, ifname: &str) -> Result<()> {
        // 1. Find config file (lexicographically first in config_dir)
        let config_file = self.find_config()?;
        let config_bytes = std::fs::read(&config_file).context("Failed to read CNI config")?;
        let config: CniConfig = serde_json::from_slice(&config_bytes).context("Failed to parse CNI config")?;
        
        // 2. Resolve Plugin Binary
        let plugin_binary = self.plugin_path.join(&config.plugin_type);
        if !plugin_binary.exists() {
            return Err(anyhow!("CNI plugin not found: {:?}", plugin_binary));
        }

        info!("CNI {}: Invoking plugin {:?} for {}", command, config.plugin_type, container_id);

        // 3. Prepare Environment
        let envs = vec![
            ("CNI_COMMAND", command),
            ("CNI_CONTAINERID", container_id),
            ("CNI_NETNS", netns),
            ("CNI_IFNAME", ifname),
            ("CNI_PATH", self.plugin_path.to_str().unwrap()),
        ];

        // 4. Invoke Plugin
        let mut child = Command::new(&plugin_binary)
            .envs(envs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn CNI plugin")?;

        // Write Config to Stdin
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(&config_bytes)?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
             let stderr = String::from_utf8_lossy(&output.stderr);
             return Err(anyhow!("CNI plugin failed: {}", stderr));
        }

        if command == "ADD" {
            let stdout = String::from_utf8_lossy(&output.stdout);
            debug!("CNI ADD Output: {}", stdout);
        }

        Ok(())
    }

    fn find_config(&self) -> Result<PathBuf> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.config_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "conf" || ext == "conflist" || ext == "json"))
            .collect();
            
        entries.sort();
        
        entries.first().cloned().ok_or_else(|| anyhow!("No CNI config found in {:?}", self.config_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_find_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path();
        
        // Create dummy config
        let config_path = config_dir.join("10-bridge.conf");
        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "{{ \"cniVersion\": \"0.4.0\", \"name\": \"dbnet\", \"type\": \"bridge\" }}").unwrap();
        
        let cni = CniManager::new(PathBuf::from("/bin"), config_dir.to_path_buf());
        let found = cni.find_config().unwrap();
        
        assert_eq!(found, config_path);
    }
}
