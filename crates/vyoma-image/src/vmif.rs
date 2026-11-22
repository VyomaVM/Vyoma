use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

pub use vyoma_core::oci::OciImageConfig;

pub mod media_types {
    pub const VMIF_INDEX_V2: &str = "application/vnd.vyoma.image.index.v2+json";
    pub const VMIF_MANIFEST_V2: &str = "application/vnd.vyoma.image.manifest.v2+json";
    pub const VMIF_SOFTWARE_V2: &str = "application/vnd.vyoma.image.software.v2+json";
    pub const VMIF_KERNEL_V2: &str = "application/vnd.vyoma.image.kernel.v2+json";
    pub const VMIF_COMPOSITE_V2: &str = "application/vnd.vyoma.image.composite.v2+json";
    pub const VMIF_VERIFICATION_V2: &str = "application/vnd.vyoma.image.verification.v2+json";
    pub const LAYER_ROOTFS_SQUASHFS: &str = "application/vnd.vyoma.layer.rootfs.squashfs";
    pub const LAYER_ROOTFS_VERITY: &str = "application/vnd.vyoma.layer.rootfs.verity";
    pub const LAYER_KERNEL_VMLINUX: &str = "application/vnd.vyoma.layer.kernel.vmlinux";
    pub const LAYER_KERNEL_INITRD: &str = "application/vnd.vyoma.layer.kernel.initrd";
}

fn default_true() -> bool {
    true
}

/// VMIF artifact classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmifArtifactType {
    Software,
    Kernel,
    Composite,
    Appliance,
}

impl Default for VmifArtifactType {
    fn default() -> Self {
        Self::Appliance
    }
}

/// App Store certification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    Certified,
    Community,
    Flagged,
    PendingReview,
}

impl Default for VerificationStatus {
    fn default() -> Self {
        Self::PendingReview
    }
}

/// Kernel artifact specific metadata and capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KernelArtifactSpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub workload: String,
    #[serde(default)]
    pub description: Option<String>,
    pub kernel_digest: String,
    pub kernel_size_bytes: u64,
    #[serde(default)]
    pub format: String,
    #[serde(default = "default_true")]
    pub pvh_boot: bool,
    #[serde(default)]
    pub initrd_digest: Option<String>,
    #[serde(default)]
    pub initrd_size_bytes: Option<u64>,
    #[serde(default)]
    pub recommended_cmdline: Vec<String>,
    #[serde(default)]
    pub sysctl_defaults: HashMap<String, String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub golden_pcrs: HashMap<u32, String>,
}

/// Software rootfs artifact specific metadata and verity integrity
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SoftwareArtifactSpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub compatible_workloads: Vec<String>,
    pub rootfs_digest: String,
    pub rootfs_size_bytes: u64,
    #[serde(default)]
    pub compression: String,
    #[serde(default)]
    pub verity_root_hash: Option<String>,
    #[serde(default)]
    pub verity_algorithm: Option<String>,
    #[serde(default)]
    pub verity_block_size: Option<u32>,
    #[serde(default)]
    pub suggested_kernel: Option<String>,
}

/// Composite artifact specifying the pairing of a software rootfs and a kernel
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CompositeArtifactSpec {
    pub name: String,
    pub version: String,
    pub software_ref: String,
    #[serde(default)]
    pub software_digest: Option<String>,
    pub kernel_ref: String,
    #[serde(default)]
    pub kernel_digest: Option<String>,
    #[serde(default)]
    pub cmdline_overrides: Vec<String>,
    #[serde(default)]
    pub env_overrides: HashMap<String, String>,
}

/// Hub App Store verification and security certification report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReport {
    pub report_version: u32,
    pub artifact_digest: String,
    pub status: VerificationStatus,
    pub issuer: String,
    pub signed_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub static_scan_passed: bool,
    #[serde(default)]
    pub cve_critical_count: u32,
    #[serde(default)]
    pub detonation_passed: bool,
    #[serde(default)]
    pub golden_pcrs: HashMap<u32, String>,
    #[serde(default)]
    pub verity_root_hash: Option<String>,
    #[serde(default)]
    pub seal_signature: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    pub kernel_profile: Option<String>,
    #[serde(default)]
    pub kernel_cmdline: Option<Vec<String>>,
    #[serde(default)]
    pub firmware: FirmwareInfo,
    #[serde(default)]
    pub measured_boot: MeasuredBootInfo,
    #[serde(default)]
    pub confidential_computing: ConfidentialComputingInfo,
    // VMIF v2.0 Extensions
    #[serde(default)]
    pub artifact_type: VmifArtifactType,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub software_spec: Option<SoftwareArtifactSpec>,
    #[serde(default)]
    pub kernel_spec: Option<KernelArtifactSpec>,
    #[serde(default)]
    pub composite_spec: Option<CompositeArtifactSpec>,
    #[serde(default)]
    pub verity_spec: Option<crate::verity::VeritySpec>,
    #[serde(default)]
    pub verification_report: Option<VerificationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    #[error("Invalid schema version: {0} (supported: 1, 2)")]
    InvalidSchemaVersion(u32),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Invalid artifact type {0:?} for manifest: {1}")]
    InvalidArtifactType(VmifArtifactType, String),
}

impl VmifManifest {
    /// Constructs a backward-compatible v1 VMIF Appliance manifest.
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
            kernel_profile: None,
            kernel_cmdline: None,
            firmware: FirmwareInfo::default(),
            measured_boot: MeasuredBootInfo::default(),
            confidential_computing: ConfidentialComputingInfo::default(),
            artifact_type: VmifArtifactType::Appliance,
            media_type: Some(media_types::VMIF_MANIFEST_V2.to_string()),
            software_spec: None,
            kernel_spec: None,
            composite_spec: None,
            verity_spec: None,
            verification_report: None,
        }
    }

    /// Constructs a VMIF v2.0 Software RootFS manifest.
    pub fn new_v2_software(
        arch: String,
        spec: SoftwareArtifactSpec,
        config: OciImageConfig,
    ) -> Self {
        let rootfs_digest = spec.rootfs_digest.clone();
        let size_bytes = spec.rootfs_size_bytes;
        Self {
            schema_version: 2,
            created: chrono::Utc::now().to_rfc3339(),
            arch,
            kernel: None,
            initrd: None,
            rootfs: rootfs_digest,
            config,
            labels: HashMap::new(),
            size_bytes,
            kernel_profile: None,
            kernel_cmdline: None,
            firmware: FirmwareInfo::default(),
            measured_boot: MeasuredBootInfo::default(),
            confidential_computing: ConfidentialComputingInfo::default(),
            artifact_type: VmifArtifactType::Software,
            media_type: Some(media_types::VMIF_SOFTWARE_V2.to_string()),
            software_spec: Some(spec),
            kernel_spec: None,
            composite_spec: None,
            verity_spec: None,
            verification_report: None,
        }
    }

    /// Constructs a VMIF v2.0 Kernel manifest.
    pub fn new_v2_kernel(
        arch: String,
        spec: KernelArtifactSpec,
    ) -> Self {
        let kernel_digest = spec.kernel_digest.clone();
        let initrd_digest = spec.initrd_digest.clone();
        let size_bytes = spec.kernel_size_bytes + spec.initrd_size_bytes.unwrap_or(0);
        let cmdline = if spec.recommended_cmdline.is_empty() {
            None
        } else {
            Some(spec.recommended_cmdline.clone())
        };

        Self {
            schema_version: 2,
            created: chrono::Utc::now().to_rfc3339(),
            arch,
            kernel: Some(kernel_digest),
            initrd: initrd_digest,
            rootfs: String::new(),
            config: OciImageConfig::default(),
            labels: HashMap::new(),
            size_bytes,
            kernel_profile: Some(spec.name.clone()),
            kernel_cmdline: cmdline,
            firmware: FirmwareInfo::default(),
            measured_boot: MeasuredBootInfo::default(),
            confidential_computing: ConfidentialComputingInfo::default(),
            artifact_type: VmifArtifactType::Kernel,
            media_type: Some(media_types::VMIF_KERNEL_V2.to_string()),
            software_spec: None,
            kernel_spec: Some(spec),
            composite_spec: None,
            verity_spec: None,
            verification_report: None,
        }
    }

    /// Constructs a VMIF v2.0 Composite manifest binding Software and Kernel.
    pub fn new_v2_composite(
        arch: String,
        spec: CompositeArtifactSpec,
        config: OciImageConfig,
    ) -> Self {
        Self {
            schema_version: 2,
            created: chrono::Utc::now().to_rfc3339(),
            arch,
            kernel: Some(spec.kernel_ref.clone()),
            initrd: None,
            rootfs: spec.software_ref.clone(),
            config,
            labels: HashMap::new(),
            size_bytes: 0,
            kernel_profile: None,
            kernel_cmdline: if spec.cmdline_overrides.is_empty() {
                None
            } else {
                Some(spec.cmdline_overrides.clone())
            },
            firmware: FirmwareInfo::default(),
            measured_boot: MeasuredBootInfo::default(),
            confidential_computing: ConfidentialComputingInfo::default(),
            artifact_type: VmifArtifactType::Composite,
            media_type: Some(media_types::VMIF_COMPOSITE_V2.to_string()),
            software_spec: None,
            kernel_spec: None,
            composite_spec: Some(spec),
            verity_spec: None,
            verification_report: None,
        }
    }

    pub fn with_kernel_profile(mut self, profile: impl Into<String>) -> Self {
        self.kernel_profile = Some(profile.into());
        self
    }

    pub fn with_kernel_cmdline(mut self, cmdline: Vec<String>) -> Self {
        self.kernel_cmdline = Some(cmdline);
        self
    }

    pub fn with_verity(mut self, verity: crate::verity::VeritySpec) -> Self {
        self.verity_spec = Some(verity);
        self
    }

    pub fn with_verification_report(mut self, report: VerificationReport) -> Self {
        self.verification_report = Some(report);
        self
    }

    pub fn effective_cmdline(&self) -> Vec<String> {
        self.kernel_cmdline.clone().unwrap_or_default()
    }

    pub fn validate(&self) -> Result<(), VmifError> {
        if self.schema_version != 1 && self.schema_version != 2 {
            return Err(VmifError::InvalidSchemaVersion(self.schema_version));
        }

        if self.arch.is_empty() {
            return Err(VmifError::MissingField("arch".to_string()));
        }

        match self.artifact_type {
            VmifArtifactType::Appliance => {
                if self.rootfs.is_empty() {
                    return Err(VmifError::MissingField("rootfs".to_string()));
                }
            }
            VmifArtifactType::Software => {
                if self.rootfs.is_empty() && self.software_spec.is_none() {
                    return Err(VmifError::MissingField("rootfs or software_spec".to_string()));
                }
            }
            VmifArtifactType::Kernel => {
                if self.kernel.is_none() && self.kernel_spec.is_none() {
                    return Err(VmifError::MissingField("kernel or kernel_spec".to_string()));
                }
            }
            VmifArtifactType::Composite => {
                if self.composite_spec.is_none() {
                    return Err(VmifError::MissingField("composite_spec".to_string()));
                }
            }
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

        // Software and Appliance require rootfs file existence
        if self.manifest.artifact_type == VmifArtifactType::Appliance
            || self.manifest.artifact_type == VmifArtifactType::Software
        {
            if !self.rootfs_path.exists() {
                return Err(VmifError::MissingField(format!(
                    "rootfs not found at {:?}",
                    self.rootfs_path
                )));
            }
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
        assert_eq!(manifest.artifact_type, VmifArtifactType::Appliance);
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

    #[test]
    fn test_v2_software_manifest_roundtrip() {
        let spec = SoftwareArtifactSpec {
            name: "vyoma/postgres".to_string(),
            version: "16-alpine".to_string(),
            compatible_workloads: vec!["database".to_string()],
            rootfs_digest: "sha256:77f1981cb31e50f58d927c945a0db8316c02eb92965313936a71bbd792e34567".to_string(),
            rootfs_size_bytes: 184549376,
            compression: "zstd".to_string(),
            verity_root_hash: Some("3f2504e0ad3017cf65b83592c73087fac60b07118460c1ecd677180cfb1d2345".to_string()),
            verity_algorithm: Some("sha256".to_string()),
            verity_block_size: Some(4096),
            suggested_kernel: Some("vyomahub.io/kernels/db:6.16".to_string()),
        };

        let manifest = VmifManifest::new_v2_software("x86_64".to_string(), spec, OciImageConfig::default());
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.artifact_type, VmifArtifactType::Software);
        assert!(manifest.validate().is_ok());

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: VmifManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.artifact_type, VmifArtifactType::Software);
        assert_eq!(deserialized.software_spec.unwrap().name, "vyoma/postgres");
    }

    #[test]
    fn test_v2_kernel_manifest_validation() {
        let mut pcrs = HashMap::new();
        pcrs.insert(0, "a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0".to_string());

        let spec = KernelArtifactSpec {
            name: "vyoma/kernel-db".to_string(),
            version: "6.16.9-vyoma-db-v1".to_string(),
            workload: "database".to_string(),
            description: Some("Database optimized kernel".to_string()),
            kernel_digest: "sha256:4a8b79e18b871c5ef91b920e4b8593671234abcd5678ef0123456789abcdef01".to_string(),
            kernel_size_bytes: 14680064,
            format: "elf64".to_string(),
            pvh_boot: true,
            initrd_digest: None,
            initrd_size_bytes: None,
            recommended_cmdline: vec!["elevator=deadline".to_string(), "transparent_hugepage=always".to_string()],
            sysctl_defaults: HashMap::new(),
            features: vec!["io_uring".to_string(), "hugepages".to_string()],
            golden_pcrs: pcrs,
        };

        let manifest = VmifManifest::new_v2_kernel("x86_64".to_string(), spec);
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.artifact_type, VmifArtifactType::Kernel);
        assert_eq!(manifest.rootfs, "");
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_v2_composite_manifest_with_verification_report() {
        let spec = CompositeArtifactSpec {
            name: "myorg/custom-postgres".to_string(),
            version: "1.0.0".to_string(),
            software_ref: "vyomahub.io/library/postgres:16".to_string(),
            software_digest: Some("sha256:77f1981c...".to_string()),
            kernel_ref: "vyomahub.io/kernels/db:6.16".to_string(),
            kernel_digest: Some("sha256:4a8b79e1...".to_string()),
            cmdline_overrides: vec!["max_connections=200".to_string()],
            env_overrides: HashMap::new(),
        };

        let report = VerificationReport {
            report_version: 1,
            artifact_digest: "sha256:composite123".to_string(),
            status: VerificationStatus::Certified,
            issuer: "Vyoma Hub Trust Authority".to_string(),
            signed_at: "2026-11-21T12:00:00Z".to_string(),
            expires_at: Some("2027-11-21T12:00:00Z".to_string()),
            static_scan_passed: true,
            cve_critical_count: 0,
            detonation_passed: true,
            golden_pcrs: HashMap::new(),
            verity_root_hash: None,
            seal_signature: Some(vec![1, 2, 3, 4]),
        };

        let manifest = VmifManifest::new_v2_composite("x86_64".to_string(), spec, OciImageConfig::default())
            .with_verification_report(report);

        assert_eq!(manifest.artifact_type, VmifArtifactType::Composite);
        assert!(manifest.validate().is_ok());
        assert_eq!(manifest.verification_report.unwrap().status, VerificationStatus::Certified);
    }
}
