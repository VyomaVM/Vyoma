use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct NetworkResult {
    pub interface_name: String,
    pub ip_address: String,
    pub gateway: Option<String>,
    pub mac_address: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CniConfig {
    #[serde(rename = "cniVersion")]
    pub cni_version: String,
    pub name: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    #[serde(flatten)]
    pub args: HashMap<String, serde_json::Value>,
}

#[derive(Debug)]
pub struct NetworkAttachment {
    pub network_name: String,
    pub interface_name: String,
    pub result: NetworkResult,
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

    ///
    /// # Arguments
    /// * `network_name`: Optional specific network to use. If None, uses first found.
    pub fn add(
        &self,
        network_name: Option<&str>,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> Result<NetworkResult> {
        let res = self.exec("ADD", network_name, container_id, netns, ifname)?;
        let json = res.ok_or_else(|| anyhow!("CNI Plugin returned no output for ADD"))?;
        self.parse_network_result(&json, ifname)
    }

    /// Attach a VM to multiple networks.
    /// Returns a list of network attachments with parsed results.
    pub fn add_multiple(
        &self,
        networks: &[String],
        container_id: &str,
        netns: &str,
    ) -> Result<Vec<NetworkAttachment>> {
        let mut attachments = Vec::new();
        let mut used_interfaces = HashMap::new();

        for (idx, network_name) in networks.iter().enumerate() {
            let ifname = format!("eth{}", idx);
            info!(
                "CNI: Adding VM {} to network '{}' as {}",
                container_id, network_name, ifname
            );

            match self.add(Some(network_name), container_id, netns, &ifname) {
                Ok(result) => {
                    used_interfaces.insert(ifname.clone(), result.ip_address.clone());
                    attachments.push(NetworkAttachment {
                        network_name: network_name.clone(),
                        interface_name: ifname,
                        result,
                    });
                }
                Err(e) => {
                    warn!(
                        "Failed to attach to network '{}': {}. Continuing with remaining networks.",
                        network_name, e
                    );
                }
            }
        }

        if attachments.is_empty() && !networks.is_empty() {
            return Err(anyhow!(
                "Failed to attach to any of the {} requested networks",
                networks.len()
            ));
        }

        Ok(attachments)
    }

    /// Executed CNI DEL command.
    pub fn del(
        &self,
        network_name: Option<&str>,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> Result<()> {
        self.exec("DEL", network_name, container_id, netns, ifname)
            .map(|_| ())
    }

    /// Delete all network interfaces for a VM with multiple network attachments.
    pub fn del_multiple(&self, networks: &[String], container_id: &str, netns: &str) -> Result<()> {
        for (idx, network_name) in networks.iter().enumerate() {
            let ifname = format!("eth{}", idx);
            if let Err(e) = self.del(Some(network_name), container_id, netns, &ifname) {
                warn!(
                    "Failed to delete interface {} from network '{}': {}",
                    ifname, network_name, e
                );
            }
        }
        Ok(())
    }

    fn parse_network_result(&self, json: &serde_json::Value, ifname: &str) -> Result<NetworkResult> {
        let ips = json
            .get("ips")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| {
                let address = first.get("address")?.as_str()?;
                let cidr_prefix = address.find('/')?;
                let ip = &address[..cidr_prefix];
                Some(ip.to_string())
            })
            .ok_or_else(|| anyhow!("No IPs found in CNI result"))?;

        let gateway = json
            .get("dns")
            .and_then(|d| d.get("gateway"))
            .and_then(|g| g.as_str())
            .map(String::from)
            .or_else(|| {
                json.get("gateway")
                    .and_then(|g| g.as_str())
                    .map(String::from)
            });

        let mac = json
            .get("mac")
            .and_then(|m| m.as_str())
            .map(String::from);

        Ok(NetworkResult {
            interface_name: ifname.to_string(),
            ip_address: ips,
            gateway,
            mac_address: mac,
        })
    }

    fn exec(
        &self,
        command: &str,
        network_name: Option<&str>,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> Result<Option<serde_json::Value>> {
        // 1. Find config file
        let config_file = self.find_config(network_name)?;
        let config_bytes = std::fs::read(&config_file).context("Failed to read CNI config")?;
        let config: CniConfig =
            serde_json::from_slice(&config_bytes).context("Failed to parse CNI config")?;

        // 2. Resolve Plugin Binary
        let plugin_binary = self.plugin_path.join(&config.plugin_type);
        if !plugin_binary.exists() {
            return Err(anyhow!("CNI plugin not found: {:?}", plugin_binary));
        }

        info!(
            "CNI {}: Invoking plugin {:?} for {} (Net: {})",
            command, config.plugin_type, container_id, config.name
        );

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
            let stdout_str = String::from_utf8_lossy(&output.stdout);
            debug!("CNI ADD Output: {}", stdout_str);
            let res: serde_json::Value =
                serde_json::from_slice(&output.stdout).context("Failed to parse CNI output")?;
            return Ok(Some(res));
        }

        Ok(None)
    }

    fn find_config(&self, name_filter: Option<&str>) -> Result<PathBuf> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.config_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map_or(false, |ext| {
                    ext == "conf" || ext == "conflist" || ext == "json"
                })
            })
            .collect();

        entries.sort();

        if let Some(target_name) = name_filter {
            // We need to parse content to find name
            for path in &entries {
                if let Ok(content) = std::fs::read(path) {
                    if let Ok(config) = serde_json::from_slice::<CniConfig>(&content) {
                        if config.name == target_name {
                            return Ok(path.clone());
                        }
                    }
                }
            }
            Err(anyhow!("Network config named '{}' not found", target_name))
        } else {
            entries
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("No CNI config found in {:?}", self.config_dir))
        }
    }

    pub fn list_networks(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |e| e == "conf" || e == "json")
            {
                let content = std::fs::read(&path)?;
                if let Ok(config) = serde_json::from_slice::<CniConfig>(&content) {
                    names.push(config.name);
                }
            }
        }
        Ok(names)
    }

    pub fn create_network(&self, name: &str, subnet: &str) -> Result<()> {
        let bridge_name = format!("vyoma-{}", name);
        let config = serde_json::json!({
            "cniVersion": "0.3.1",
            "name": name,
            "type": "bridge",
            "bridge": bridge_name,
            "isGateway": true,
            "ipMasq": true,
            "ipam": {
                "type": "host-local",
                "subnet": subnet,
                "routes": [{ "dst": "0.0.0.0/0" }]
            }
        });

        let path = self.config_dir.join(format!("{}.conf", name));
        if path.exists() {
            return Err(anyhow!("Network config file exists"));
        }

        let f = std::fs::File::create(&path)?;
        serde_json::to_writer_pretty(f, &config)?;
        Ok(())
    }

    pub fn create_overlay_network(&self, name: &str, _subnet: &str) -> Result<()> {
        let config = serde_json::json!({
            "cniVersion": "0.4.0",
            "name": name,
            "type": "flannel",
            "delegate": {
                "isDefaultGateway": true,
                "hairpinMode": true
            }
        });

        let path = self.config_dir.join(format!("{}.conf", name));
        if path.exists() {
            return Err(anyhow!("Network config file exists"));
        }

        let f = std::fs::File::create(&path)?;
        serde_json::to_writer_pretty(f, &config)?;
        Ok(())
    }

    pub fn delete_network(&self, name: &str) -> Result<()> {
        for entry in std::fs::read_dir(&self.config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .map_or(false, |e| e == "conf" || e == "json")
            {
                let content = std::fs::read(&path)?;
                if let Ok(config) = serde_json::from_slice::<CniConfig>(&content) {
                    if config.name == name {
                        std::fs::remove_file(path)?;
                        return Ok(());
                    }
                }
            }
        }
        Err(anyhow!("Network not found"))
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
        writeln!(
            file,
            "{{ \"cniVersion\": \"0.4.0\", \"name\": \"dbnet\", \"type\": \"bridge\" }}"
        )
        .unwrap();

        let cni = CniManager::new(PathBuf::from("/bin"), config_dir.to_path_buf());
        let found = cni.find_config(None).unwrap();

        assert_eq!(found, config_path);
    }
}
