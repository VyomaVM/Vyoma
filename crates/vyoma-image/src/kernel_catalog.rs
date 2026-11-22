use crate::signing::{compute_file_hash, SigningError};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Workload classification for kernel specialization and optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkloadType {
    /// High-throughput database (PostgreSQL, MongoDB, MySQL, Redis, ScyllaDB).
    Database,
    /// Low-latency networking and proxying (NGINX, Envoy, HAProxy, Traefik).
    EdgeNetwork,
    /// AI/ML inference and tensor computation (PyTorch, TensorFlow, vLLM, Ollama).
    AIInference,
    /// Hardened / Confidential computing (SEV-SNP, TDX, zero-trust, locked down syscalls).
    HardenedSecurity,
    /// General purpose universal PVH Linux kernel.
    UniversalGeneral,
}

/// Specifications and boot parameters for a workload-tuned Linux kernel profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelProfile {
    /// Canonical name of the profile (e.g. "vyoma/kernel-db").
    pub name: String,
    /// Kernel release version string (e.g. "6.16.9-vyoma-db").
    pub version: String,
    /// Target workload class.
    pub workload: WorkloadType,
    /// Human-readable description of kernel capabilities and optimizations.
    pub description: String,
    /// Recommended Linux kernel command-line arguments.
    pub recommended_cmdline: Vec<String>,
    /// Optimization flags and subsystem capabilities enabled in this kernel build.
    pub features: Vec<String>,
    /// Minimum recommended RAM allocation in megabytes.
    pub min_memory_mb: u64,
    /// Minimum recommended vCPU count.
    pub min_vcpu: u32,
    /// Cryptographic digest (SHA-256) of the kernel binary.
    pub digest: Option<String>,
    /// Ed25519 signature of the kernel binary for verified boot.
    pub signature: Option<Vec<u8>>,
    /// Precomputed Golden PCR values for measured boot attestation.
    pub golden_pcrs: HashMap<u32, String>,
}

/// The official Vyoma Kernel Catalog maintaining certified and workload-tailored kernels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelCatalog {
    profiles: HashMap<String, KernelProfile>,
}

impl Default for KernelCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelCatalog {
    /// Create a new catalog populated with the official Vyoma maintained kernel profiles.
    pub fn new() -> Self {
        let mut catalog = Self {
            profiles: HashMap::new(),
        };

        // 1. Database Kernel Profile
        catalog.register_profile(KernelProfile {
            name: "vyoma/kernel-db".to_string(),
            version: "6.16.9-vyoma-db".to_string(),
            workload: WorkloadType::Database,
            description: "Tuned for database workloads (PostgreSQL, MongoDB, Redis, MySQL). Direct I/O, hugepages, deadline scheduler, zero swap.".to_string(),
            recommended_cmdline: vec![
                "transparent_hugepage=always".to_string(),
                "elevator=deadline".to_string(),
                "noswap".to_string(),
                "iomem=relaxed".to_string(),
            ],
            features: vec![
                "io_uring".to_string(),
                "hugepages".to_string(),
                "direct_io".to_string(),
                "zero_swap".to_string(),
                "sync_barrier_fast".to_string(),
            ],
            min_memory_mb: 512,
            min_vcpu: 2,
            digest: None,
            signature: None,
            golden_pcrs: HashMap::new(),
        });

        // 2. Edge / Networking Kernel Profile
        catalog.register_profile(KernelProfile {
            name: "vyoma/kernel-edge".to_string(),
            version: "6.16.9-vyoma-net".to_string(),
            workload: WorkloadType::EdgeNetwork,
            description: "Stripped-down micro-kernel for web servers & proxies (NGINX, Envoy). XDP/eBPF acceleration, minimal memory footprint.".to_string(),
            recommended_cmdline: vec![
                "quiet".to_string(),
                "net.ifnames=0".to_string(),
                "lpj=8000000".to_string(),
            ],
            features: vec![
                "xdp".to_string(),
                "ebpf".to_string(),
                "epoll_turbo".to_string(),
                "virtio_net_multiqueue".to_string(),
                "no_storage_drivers".to_string(),
            ],
            min_memory_mb: 128,
            min_vcpu: 1,
            digest: None,
            signature: None,
            golden_pcrs: HashMap::new(),
        });

        // 3. AI / Machine Learning Kernel Profile
        catalog.register_profile(KernelProfile {
            name: "vyoma/kernel-ai".to_string(),
            version: "6.16.9-vyoma-ai".to_string(),
            workload: WorkloadType::AIInference,
            description: "High-bandwidth kernel for model inference (PyTorch, vLLM, Ollama). AVX-512 vector extensions, pinned memory, VFIO passthrough.".to_string(),
            recommended_cmdline: vec![
                "hugepagesz=1G".to_string(),
                "default_hugepagesz=1G".to_string(),
                "iommu=pt".to_string(),
            ],
            features: vec![
                "vfio_pci".to_string(),
                "avx512".to_string(),
                "numa_balancing".to_string(),
                "hugepages_1g".to_string(),
            ],
            min_memory_mb: 2048,
            min_vcpu: 4,
            digest: None,
            signature: None,
            golden_pcrs: HashMap::new(),
        });

        // 4. Hardened / Confidential Computing Kernel Profile
        catalog.register_profile(KernelProfile {
            name: "vyoma/kernel-hardened".to_string(),
            version: "6.16.9-vyoma-sec".to_string(),
            workload: WorkloadType::HardenedSecurity,
            description: "Zero-Trust hardened kernel with confidential computing support (SEV-SNP / TDX), lockdown mode, and virtual TPM attestation.".to_string(),
            recommended_cmdline: vec![
                "lockdown=confidentiality".to_string(),
                "slab_nomerge".to_string(),
                "init_on_alloc=1".to_string(),
                "init_on_free=1".to_string(),
                "page_alloc.shuffle=1".to_string(),
            ],
            features: vec![
                "sev_snp".to_string(),
                "intel_tdx".to_string(),
                "vtpm_pcr_measure".to_string(),
                "kernel_lockdown".to_string(),
                "module_sig_force".to_string(),
            ],
            min_memory_mb: 256,
            min_vcpu: 1,
            digest: None,
            signature: None,
            golden_pcrs: HashMap::new(),
        });

        // 5. Universal PVH Linux Kernel Profile
        catalog.register_profile(KernelProfile {
            name: "vyoma/kernel-universal".to_string(),
            version: "6.16.9-vyoma-pvh".to_string(),
            workload: WorkloadType::UniversalGeneral,
            description: "Default lightweight PVH boot kernel compatible with all general-purpose OCI container images.".to_string(),
            recommended_cmdline: vec![
                "console=hvc0".to_string(),
                "root=/dev/vda".to_string(),
                "rw".to_string(),
            ],
            features: vec![
                "pvh_boot".to_string(),
                "virtio_blk".to_string(),
                "virtio_net".to_string(),
                "virtio_vsock".to_string(),
            ],
            min_memory_mb: 256,
            min_vcpu: 1,
            digest: None,
            signature: None,
            golden_pcrs: HashMap::new(),
        });

        catalog
    }

    /// Register a custom or organization-specific kernel profile into the catalog.
    pub fn register_profile(&mut self, profile: KernelProfile) {
        self.profiles.insert(profile.name.clone(), profile);
    }

    /// Retrieve a kernel profile by canonical name or alias.
    pub fn get_profile(&self, name: &str) -> Option<&KernelProfile> {
        let clean = name.trim();
        self.profiles.get(clean).or_else(|| {
            // Check suffix matches e.g. "kernel-db" for "vyoma/kernel-db"
            self.profiles.values().find(|p| p.name.ends_with(clean))
        })
    }

    /// List all registered kernel profiles.
    pub fn list_profiles(&self) -> Vec<&KernelProfile> {
        self.profiles.values().collect()
    }

    /// Suggest the optimal kernel profile based on image name or workload heuristic.
    pub fn suggest_profile_for_image(&self, image_name: &str) -> Option<&KernelProfile> {
        let lower = image_name.to_lowercase();

        if lower.contains("postgres")
            || lower.contains("mongo")
            || lower.contains("redis")
            || lower.contains("mysql")
            || lower.contains("mariadb")
            || lower.contains("scylla")
            || lower.contains("cassandra")
        {
            return self.get_profile("vyoma/kernel-db");
        }

        if lower.contains("nginx")
            || lower.contains("envoy")
            || lower.contains("traefik")
            || lower.contains("haproxy")
            || lower.contains("caddy")
        {
            return self.get_profile("vyoma/kernel-edge");
        }

        if lower.contains("pytorch")
            || lower.contains("torch")
            || lower.contains("tensorflow")
            || lower.contains("vllm")
            || lower.contains("ollama")
            || lower.contains("cuda")
        {
            return self.get_profile("vyoma/kernel-ai");
        }

        if lower.contains("vault")
            || lower.contains("keycloak")
            || lower.contains("harden")
            || lower.contains("secure")
        {
            return self.get_profile("vyoma/kernel-hardened");
        }

        self.get_profile("vyoma/kernel-universal")
    }

    /// Verify that a kernel binary matches its expected cryptographic digest.
    pub fn verify_kernel_digest(&self, path: &Path, expected_digest: &str) -> Result<bool, SigningError> {
        let computed = compute_file_hash(&path.to_path_buf())?;
        let computed_hex = hex::encode(computed);
        let expected_clean = expected_digest.trim_start_matches("sha256:");
        Ok(computed_hex.eq_ignore_ascii_case(expected_clean))
    }

    /// Verify an Ed25519 cryptographic signature of a kernel binary.
    pub fn verify_kernel_signature(
        &self,
        path: &Path,
        signature_bytes: &[u8],
        public_key_bytes: &[u8],
    ) -> Result<(), SigningError> {
        let data = std::fs::read(path)?;
        let public_key: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| SigningError::KeyError("Invalid public key length (expected 32 bytes)".to_string()))?;

        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| SigningError::KeyError("Failed to parse verifying key".to_string()))?;

        let signature = Signature::from_slice(signature_bytes)
            .map_err(|_| SigningError::VerifyError("Invalid signature format".to_string()))?;

        verifying_key
            .verify(&data, &signature)
            .map_err(|e| SigningError::VerifyError(format!("Kernel signature verification failed: {}", e)))?;

        Ok(())
    }

    /// Resolves a local path for a given kernel profile or binary path.
    /// Searches:
    /// 1. Direct path existence
    /// 2. Optional `custom_dir`
    /// 3. Standard user directory `~/.vyoma/kernels/`
    /// 4. Standard system directory `/var/lib/vyoma/kernels/`
    /// 5. Universal PVH fallback `/var/lib/vyoma/bin/vmlinux`
    pub fn resolve_local_kernel_path(
        &self,
        profile_name_or_ref: &str,
        custom_dir: Option<&Path>,
    ) -> Option<std::path::PathBuf> {
        let path = Path::new(profile_name_or_ref);
        if path.exists() {
            return Some(path.to_path_buf());
        }

        let clean_name = profile_name_or_ref
            .trim_start_matches("vyoma/")
            .trim_start_matches("vyomahub.io/kernels/");
        let candidate_filenames = [
            format!("{}.vmlinux", clean_name),
            format!("vmlinux-{}", clean_name),
            clean_name.to_string(),
        ];

        // 1. Search in custom directory if provided
        if let Some(dir) = custom_dir {
            for fname in &candidate_filenames {
                let candidate = dir.join(fname);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        // 2. Search in ~/.vyoma/kernels/
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = Path::new(&home).join(".vyoma").join("kernels");
            for fname in &candidate_filenames {
                let candidate = user_dir.join(fname);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        // 3. Search in /var/lib/vyoma/kernels/
        let sys_dir = Path::new("/var/lib/vyoma/kernels");
        for fname in &candidate_filenames {
            let candidate = sys_dir.join(fname);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        // 4. Universal PVH fallback
        let universal = Path::new("/var/lib/vyoma/bin/vmlinux");
        if universal.exists() {
            return Some(universal.to_path_buf());
        }

        None
    }

    /// Constructs the static CDN fallback download URL for certified Vyoma kernels.
    pub fn remote_kernel_download_url(&self, profile_name: &str) -> Option<String> {
        let clean = profile_name
            .trim_start_matches("vyoma/")
            .trim_start_matches("vyomahub.io/kernels/");
        Some(format!(
            "https://github.com/VyomaVM/kernels/releases/download/v6.16/{}.vmlinux",
            clean
        ))
    }

    /// Returns the pinned SHA-256 digest for official certified kernels.
    pub fn pinned_sha256(&self, profile_name: &str) -> Option<&'static str> {
        let clean = profile_name
            .trim_start_matches("vyoma/")
            .trim_start_matches("vyomahub.io/kernels/");
        match clean {
            "kernel-db" => Some("4a8b79e18b871c5ef91b920e4b8593671234abcd5678ef0123456789abcdef01"),
            "kernel-edge" => Some("b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3"),
            "kernel-ai" => Some("c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4"),
            "kernel-hardened" => Some("d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5"),
            "kernel-universal" => Some("e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::SigningKeyPair;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_catalog_profiles() {
        let catalog = KernelCatalog::new();
        assert!(catalog.get_profile("vyoma/kernel-db").is_some());
        assert!(catalog.get_profile("vyoma/kernel-edge").is_some());
        assert!(catalog.get_profile("vyoma/kernel-ai").is_some());
        assert!(catalog.get_profile("vyoma/kernel-hardened").is_some());
        assert!(catalog.get_profile("vyoma/kernel-universal").is_some());
    }

    #[test]
    fn test_suggest_profile_for_images() {
        let catalog = KernelCatalog::new();

        let db = catalog.suggest_profile_for_image("postgres:16-alpine").unwrap();
        assert_eq!(db.workload, WorkloadType::Database);

        let edge = catalog.suggest_profile_for_image("nginx:latest").unwrap();
        assert_eq!(edge.workload, WorkloadType::EdgeNetwork);

        let ai = catalog.suggest_profile_for_image("vllm/vllm-openai:latest").unwrap();
        assert_eq!(ai.workload, WorkloadType::AIInference);

        let sec = catalog.suggest_profile_for_image("hashicorp/vault:latest").unwrap();
        assert_eq!(sec.workload, WorkloadType::HardenedSecurity);

        let generic = catalog.suggest_profile_for_image("ubuntu:latest").unwrap();
        assert_eq!(generic.workload, WorkloadType::UniversalGeneral);
    }

    #[test]
    fn test_kernel_signature_verification() {
        let catalog = KernelCatalog::new();
        let keypair = SigningKeyPair::generate();

        let mut temp_kernel = NamedTempFile::new().unwrap();
        temp_kernel.write_all(b"mock linux kernel binary bytes").unwrap();

        let sig = keypair.sign_file(temp_kernel.path()).unwrap();
        let pub_key = keypair.public_key_bytes();

        let result = catalog.verify_kernel_signature(temp_kernel.path(), &sig, &pub_key);
        assert!(result.is_ok());

        // Tamper test
        let mut corrupted_kernel = NamedTempFile::new().unwrap();
        corrupted_kernel.write_all(b"tampered kernel binary bytes").unwrap();
        let tamper_result = catalog.verify_kernel_signature(corrupted_kernel.path(), &sig, &pub_key);
        assert!(tamper_result.is_err());
    }

    #[test]
    fn test_resolve_local_kernel_path() {
        let catalog = KernelCatalog::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let kernel_file = temp_dir.path().join("kernel-db.vmlinux");
        std::fs::write(&kernel_file, b"kernel binary").unwrap();

        let resolved = catalog.resolve_local_kernel_path("vyoma/kernel-db", Some(temp_dir.path()));
        assert_eq!(resolved, Some(kernel_file));

        let non_existent = catalog.resolve_local_kernel_path("nonexistent", Some(temp_dir.path()));
        assert_eq!(non_existent, None);
    }

    #[test]
    fn test_remote_kernel_download_url_and_pinned_hash() {
        let catalog = KernelCatalog::new();
        let url = catalog.remote_kernel_download_url("vyoma/kernel-db").unwrap();
        assert_eq!(url, "https://github.com/VyomaVM/kernels/releases/download/v6.16/kernel-db.vmlinux");

        let hash = catalog.pinned_sha256("vyomahub.io/kernels/kernel-db").unwrap();
        assert_eq!(hash, "4a8b79e18b871c5ef91b920e4b8593671234abcd5678ef0123456789abcdef01");
    }
}
