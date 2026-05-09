use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmifManifest {
    pub schema_version: u32,
    pub created: String,
    pub arch: String,
    pub kernel: Option<String>,
    pub initrd: Option<String>,
    pub rootfs: String,
    pub config: OciImageConfig,
    pub labels: HashMap<String, String>,
    pub size_bytes: u64,
    #[serde(default)]
    pub firmware: FirmwareInfo,
    #[serde(default)]
    pub measured_boot: MeasuredBootInfo,
    #[serde(default)]
    pub confidential_computing: ConfidentialComputingInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OciImageConfig {
    pub entrypoint: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub working_dir: Option<String>,
    pub exposed_ports: Option<HashMap<String, serde_json::Value>>,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FirmwareInfo {
    #[serde(default)]
    pub firmware_type: String,
    #[serde(default)]
    pub firmware_path: Option<String>,
    #[serde(default)]
    pub secure_boot_enabled: bool,
    #[serde(default)]
    pub signature: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MeasuredBootInfo {
    #[serde(default)]
    pub pcr_policy: Option<HashMap<u32, String>>,
    #[serde(default)]
    pub kernel_signature: Option<Vec<u8>>,
    #[serde(default)]
    pub initrd_signature: Option<Vec<u8>>,
    #[serde(default)]
    pub rootfs_hash: Option<String>,
    #[serde(default)]
    pub trust_policy_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ConfidentialComputingInfo {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub technology: String,
    #[serde(default)]
    pub amd_sev_snp: Option<SevSnpInfo>,
    #[serde(default)]
    pub intel_tdx: Option<TdxInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SevSnpInfo {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub expected_measurement: Option<String>,
    #[serde(default)]
    pub certificate_path: Option<String>,
    #[serde(default)]
    pub amd_root_key: Option<Vec<u8>>,
    #[serde(default)]
    pub amd_author_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TdxInfo {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub expected_mrtd: Option<String>,
    #[serde(default)]
    pub expected_rtmr0: Option<String>,
    #[serde(default)]
    pub expected_rtmr1: Option<String>,
    #[serde(default)]
    pub expected_rtmr2: Option<String>,
    #[serde(default)]
    pub expected_rtmr3: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmifImage {
    pub manifest: VmifManifest,
    pub rootfs_path: PathBuf,
    pub kernel_path: Option<PathBuf>,
    pub initrd_path: Option<PathBuf>,
}

#[derive(Error, Debug)]
pub enum VmifError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid schema version: {0}")]
    InvalidSchemaVersion(u32),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

impl VmifManifest {
    pub fn new(
        arch: String,
        kernel: Option<String>,
        initrd: Option<String>,
        rootfs: String,
        config: OciImageConfig,
        size_bytes: u64,
    ) -> Self {
        Self {
            schema_version: 1,
            created: chrono::Utc::now().to_rfc3339(),
            arch,
            kernel,
            initrd,
            rootfs,
            config,
            labels: HashMap::new(),
            size_bytes,
            firmware: FirmwareInfo::default(),
            measured_boot: MeasuredBootInfo::default(),
            confidential_computing: ConfidentialComputingInfo::default(),
        }
    }

    pub fn validate(&self) -> Result<(), VmifError> {
        if self.schema_version != 1 {
            return Err(VmifError::InvalidSchemaVersion(self.schema_version));
        }
        if self.rootfs.is_empty() {
            return Err(VmifError::MissingField("rootfs".to_string()));
        }
        if self.arch.is_empty() {
            return Err(VmifError::MissingField("arch".to_string()));
        }
        Ok(())
    }

    pub fn with_labels(mut self, labels: HashMap<String, String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn full_command(&self) -> Vec<String> {
        let mut cmd = vec![];
        if let Some(ep) = &self.config.entrypoint {
            cmd.extend_from_slice(ep);
        }
        if let Some(c) = &self.config.cmd {
            cmd.extend_from_slice(c);
        }
        if cmd.is_empty() {
            cmd.push("/bin/sh".to_string());
        }
        cmd
    }
}

impl VmifImage {
    pub fn new(manifest: VmifManifest, rootfs_path: PathBuf) -> Self {
        Self {
            manifest,
            rootfs_path,
            kernel_path: None,
            initrd_path: None,
        }
    }

    pub fn with_kernel(mut self, kernel_path: PathBuf) -> Self {
        self.kernel_path = Some(kernel_path);
        self
    }

    pub fn with_initrd(mut self, initrd_path: PathBuf) -> Self {
        self.initrd_path = Some(initrd_path);
        self
    }

    pub fn validate(&self) -> Result<(), VmifError> {
        self.manifest.validate()?;

        if !self.rootfs_path.exists() {
            return Err(VmifError::MissingField(format!(
                "rootfs not found at {:?}",
                self.rootfs_path
            )));
        }

        if let Some(ref kernel) = self.kernel_path {
            if !kernel.exists() {
                return Err(VmifError::MissingField(format!(
                    "kernel not found at {:?}",
                    kernel
                )));
            }
        }

        if let Some(ref initrd) = self.initrd_path {
            if !initrd.exists() {
                return Err(VmifError::MissingField(format!(
                    "initrd not found at {:?}",
                    initrd
                )));
            }
        }

        Ok(())
    }
}

impl Default for OciImageConfig {
    fn default() -> Self {
        Self {
            entrypoint: None,
            cmd: Some(vec!["/bin/sh".to_string()]),
            env: None,
            working_dir: None,
            exposed_ports: None,
            user: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmif_manifest_creation() {
        let config = OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            Some("kernel:v1".to_string()),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.arch, "amd64");
    }

    #[test]
    fn test_vmif_manifest_validation() {
        let config = OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_invalid_schema_version() {
        let config = OciImageConfig::default();
        let mut manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );
        manifest.schema_version = 999;

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_missing_rootfs() {
        let config = OciImageConfig::default();
        let manifest =
            VmifManifest::new("amd64".to_string(), None, None, "".to_string(), config, 1024000);

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_full_command_with_entrypoint() {
        let mut config = OciImageConfig::default();
        config.entrypoint = Some(vec!["/usr/sbin/nginx".to_string()]);
        config.cmd = Some(vec!["-g".to_string(), "daemon off;".to_string()]);

        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let cmd = manifest.full_command();
        assert_eq!(cmd, vec!["/usr/sbin/nginx", "-g", "daemon off;"]);
    }

    #[test]
    fn test_full_command_default() {
        let config = OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let cmd = manifest.full_command();
        assert_eq!(cmd, vec!["/bin/sh"]);
    }

    #[test]
    fn test_vmif_manifest_with_labels() {
        let config = OciImageConfig::default();
        let mut labels = HashMap::new();
        labels.insert("version".to_string(), "1.0".to_string());
        labels.insert("os".to_string(), "ubuntu".to_string());

        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        )
        .with_labels(labels);

        assert_eq!(manifest.labels.get("version"), Some(&"1.0".to_string()));
        assert_eq!(manifest.labels.get("os"), Some(&"ubuntu".to_string()));
    }
}
