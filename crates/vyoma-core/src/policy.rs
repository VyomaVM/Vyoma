use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredBootPolicy {
    pub enabled: bool,
    pub required: bool,
    pub pcr_selection: Vec<u32>,
    pub verification_timeout_secs: u64,
    pub block_on_failure: bool,
    /// Path to the build signing key pair for signing manifests during build.
    pub build_signing_key_path: Option<String>,
}

impl Default for MeasuredBootPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            required: false,
            pcr_selection: vec![7, 9, 10],
            verification_timeout_secs: 30,
            block_on_failure: true,
            build_signing_key_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    pub measured_boot: MeasuredBootPolicy,
}

impl PolicyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_measured_boot(mut self, enabled: bool, required: bool) -> Self {
        self.measured_boot.enabled = enabled;
        self.measured_boot.required = required;
        self
    }

    pub fn with_build_signing_key(mut self, key_path: String) -> Self {
        self.measured_boot.build_signing_key_path = Some(key_path);
        self
    }

    pub fn set_require_measured_boot(&mut self, required: bool) {
        self.measured_boot.enabled = true;
        self.measured_boot.required = required;
        if required {
            info!("Policy: Measured boot is now required for all VMs");
        } else {
            info!("Policy: Measured boot is now optional");
        }
    }

    pub fn is_measured_boot_required(&self) -> bool {
        self.measured_boot.enabled && self.measured_boot.required
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyStatus {
    pub policy_name: String,
    pub enabled: bool,
    pub enforced: bool,
    pub details: HashMap<String, String>,
}

impl PolicyStatus {
    pub fn from_config(config: &PolicyConfig) -> Vec<PolicyStatus> {
        vec![
            PolicyStatus {
                policy_name: "measured-boot".to_string(),
                enabled: config.measured_boot.enabled,
                enforced: config.measured_boot.required,
                details: {
                    let mut d = HashMap::new();
                    d.insert(
                        "pcr_selection".to_string(),
                        config
                            .measured_boot
                            .pcr_selection
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                    d.insert(
                        "verification_timeout".to_string(),
                        config.measured_boot.verification_timeout_secs.to_string(),
                    );
                    d.insert(
                        "build_signing_key".to_string(),
                        config
                            .measured_boot
                            .build_signing_key_path
                            .clone()
                            .unwrap_or_else(|| "none".to_string()),
                    );
                    d
                },
            },
        ]
    }
}

pub struct PolicyManager {
    config: PolicyConfig,
}

impl PolicyManager {
    pub fn new() -> Self {
        Self {
            config: PolicyConfig::new(),
        }
    }

    pub fn load_from_file(&mut self, path: &std::path::Path) -> Result<()> {
        if path.exists() {
            let data = std::fs::read_to_string(path)?;
            self.config = serde_json::from_str(&data)?;
            info!("Loaded policy config from {:?}", path);
        }
        Ok(())
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let data = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(path, data)?;
        info!("Saved policy config to {:?}", path);
        Ok(())
    }

    pub fn get_config(&self) -> &PolicyConfig {
        &self.config
    }

    pub fn set_require_measured_boot(&mut self, required: bool) {
        self.config.set_require_measured_boot(required);
    }

    pub fn should_verify_on_boot(&self) -> bool {
        self.config.measured_boot.enabled
    }

    pub fn must_verify_on_boot(&self) -> bool {
        self.config.is_measured_boot_required()
    }
}

impl Default for PolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_config_default() {
        let config = PolicyConfig::new();
        assert_eq!(config.measured_boot.enabled, false);
        assert_eq!(config.measured_boot.required, false);
    }

    #[test]
    fn test_policy_config_set_require_measured_boot() {
        let mut config = PolicyConfig::new();
        config.set_require_measured_boot(true);
        assert_eq!(config.measured_boot.enabled, true);
        assert_eq!(config.measured_boot.required, true);
    }

    #[test]
    fn test_policy_manager() {
        let mut manager = PolicyManager::new();
        manager.set_require_measured_boot(true);
        assert!(manager.should_verify_on_boot());
        assert!(manager.must_verify_on_boot());
    }

    #[test]
    fn test_policy_status_from_config() {
        let config = PolicyConfig::new().with_measured_boot(true, true);
        let status = PolicyStatus::from_config(&config);
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].policy_name, "measured-boot");
        assert_eq!(status[0].enabled, true);
        assert_eq!(status[0].enforced, true);
    }
}