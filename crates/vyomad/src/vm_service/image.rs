use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use async_trait::async_trait;
use vyoma_core::layers::LayerManager;
use vyoma_core::oci::OciManager;
use vyoma_image::{VmifConverter, VmifManifest, OciImageConfig as VyomaOciConfig, SquashfsCompression};

use super::types::PreparedImage;

pub async fn ensure_image_locally(image_name: &str, data_dir: &str) -> Result<PathBuf> {
    let images_root = PathBuf::from(data_dir).join(".vyoma").join("images");
    std::fs::create_dir_all(&images_root)?;

    let safe_image_name = image_name.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let manifest_path = image_store_path.join("vyoma.toml");
    let rootfs_ext4_path = image_store_path.join("rootfs.ext4");
    let rootfs_sqfs_path = image_store_path.join("rootfs.sqfs");

    if !rootfs_ext4_path.exists() && !rootfs_sqfs_path.exists() {
        info!("Image {} not found locally. Pulling...", image_name);
        std::fs::create_dir_all(&image_store_path)?;

        let mut oci = OciManager::new();
        let manifest_json = oci
            .pull_manifest(image_name)
            .await
            .context("Pull manifest failed")?;

        let layers = oci
            .parse_layers(&manifest_json)
            .context("Parse layers failed")?;

        let mut oci_config: Option<VyomaOciConfig> = None;
        if let Ok(config_digest) = oci.parse_config_digest(&manifest_json) {
            info!("Fetching OCI config blob: {}", config_digest);
            if let Ok(config) = oci.pull_config_blob(image_name, &config_digest).await {
                let config_path = image_store_path.join("vyoma-config.json");
                if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                    if let Err(e) = std::fs::write(&config_path, json_str) {
                        warn!("Failed to write vyoma-config.json: {}", e);
                    } else {
                        info!("Saved OCI configuration to {:?}", config_path);
                    }
                }
                oci_config = Some(VyomaOciConfig {
                    entrypoint: config.entrypoint,
                    cmd: config.cmd,
                    env: config.env,
                    working_dir: config.working_dir,
                    exposed_ports: config.exposed_ports,
                    user: config.user,
                });
            }
        }

        let temp_unpack_dir = tempfile::tempdir().context("Failed to create temp dir")?;

        for digest in layers {
            let layer_data = oci.pull_layer(image_name, &digest)
                .await
                .context(format!("Failed layer {}", digest))?;
            LayerManager::unpack_layer(&layer_data, temp_unpack_dir.path())
                .context("Unpack failed")?;
        }

        // Generate native ext4 rootfs directly from the unpacked container directory
        let _ = std::process::Command::new("mke2fs")
            .args(&["-d", &temp_unpack_dir.path().to_string_lossy(), "-t", "ext4", "-F", &rootfs_ext4_path.to_string_lossy(), "200M"])
            .output();

        let converter = VmifConverter::new();
        let config = oci_config.unwrap_or_else(|| VyomaOciConfig::default());
        let target_arch = vyoma_core::oci::host_oci_arch();

        let _vmif_image = converter.convert_directory_to_vmif(
            temp_unpack_dir.path(),
            &image_store_path,
            image_name,
            target_arch,
            config,
            None,
            None,
            SquashfsCompression::default(),
        ).context("VMIF conversion failed")?;

        info!("Image {} converted to VMIF successfully", image_name);
    }

    if rootfs_ext4_path.exists() {
        info!("Native ext4 rootfs found at {:?}", rootfs_ext4_path);
        Ok(rootfs_ext4_path)
    } else if rootfs_sqfs_path.exists() {
        info!("Converting rootfs.sqfs to rootfs.ext4 for native kernel ext4 driver...");
        if let Ok(temp_dir) = tempfile::tempdir() {
            let _ = std::process::Command::new("unsquashfs")
                .args(&["-d", &temp_dir.path().to_string_lossy(), &rootfs_sqfs_path.to_string_lossy()])
                .output();
            let _ = std::process::Command::new("mke2fs")
                .args(&["-d", &temp_dir.path().to_string_lossy(), "-t", "ext4", "-F", &rootfs_ext4_path.to_string_lossy(), "200M"])
                .output();
        }
        if rootfs_ext4_path.exists() {
            Ok(rootfs_ext4_path)
        } else {
            Ok(rootfs_sqfs_path)
        }
    } else {
        Ok(rootfs_sqfs_path)
    }
}

pub async fn load_vmif_manifest(image_name: &str, data_dir: &str) -> Result<VmifManifest> {
    let images_root = PathBuf::from(data_dir).join(".vyoma").join("images");
    
    let safe_image_name = image_name.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let manifest_path = image_store_path.join("vyoma.toml");
    
    VmifConverter::load_manifest(&manifest_path)
        .context("Failed to load VMIF manifest")
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf>;
    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig>;
    async fn get_vmif_manifest(&self, image_name: &str) -> Result<Option<vyoma_image::VmifManifest>>;
}

pub struct OciImageProvider {
    pub data_dir: String,
}

#[async_trait]
impl ImageProvider for OciImageProvider {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf> {
        ensure_image_locally(image_name, &self.data_dir).await
    }

    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig> {
        extract_oci_config(image_path)
    }

    async fn get_vmif_manifest(&self, image_name: &str) -> Result<Option<vyoma_image::VmifManifest>> {
        match load_vmif_manifest(image_name, &self.data_dir).await {
            Ok(m) => Ok(Some(m)),
            Err(_) => Ok(None),
        }
    }
}

pub struct CachedImageProvider {
    cache_dir: PathBuf,
    data_dir: String,
}

impl CachedImageProvider {
    pub fn new(data_dir: &str) -> Result<Self> {
        let cache_dir = PathBuf::from(data_dir).join(".vyoma").join("images");
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir, data_dir: data_dir.to_string() })
    }

    pub fn get_cached_path(&self, image_name: &str) -> Option<PathBuf> {
        let sanitized = image_name.replace(':', "_").replace('/', "_");
        let image_dir = self.cache_dir.join(&sanitized);
        let sqfs_path = image_dir.join("rootfs.sqfs");
        let manifest_path = image_dir.join("vyoma.toml");
        if sqfs_path.exists() && manifest_path.exists() {
            Some(sqfs_path)
        } else {
            None
        }
    }
}

#[async_trait]
impl ImageProvider for CachedImageProvider {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf> {
        if let Some(cached) = self.get_cached_path(image_name) {
            info!("Using cached VMIF image for {}", image_name);
            return Ok(cached);
        }
        ensure_image_locally(image_name, &self.data_dir).await
    }

    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig> {
        extract_oci_config(image_path)
    }

    async fn get_vmif_manifest(&self, image_name: &str) -> Result<Option<vyoma_image::VmifManifest>> {
        match load_vmif_manifest(image_name, &self.data_dir).await {
            Ok(m) => Ok(Some(m)),
            Err(_) => Ok(None),
        }
    }
}

pub async fn prepare_image(image_name: &str, data_dir: &str) -> Result<PreparedImage> {
    prepare_image_with_provider(image_name, &OciImageProvider { data_dir: data_dir.to_string() }).await
}

pub fn resolve_kernel(
    requested_kernel: Option<&str>,
    manifest: &Option<vyoma_image::VmifManifest>,
    data_dir: &str,
) -> PathBuf {
    let catalog = vyoma_image::KernelCatalog::new();
    let data_path = Path::new(data_dir);

    // 1. Explicit request override takes precedence
    if let Some(req_k) = requested_kernel {
        let trimmed = req_k.trim();
        if !trimmed.is_empty() {
            if let Some(resolved) = catalog.resolve_local_kernel_path(trimmed, Some(data_path)) {
                return resolved;
            }
            if let Some(resolved) = resolve_kernel_by_tag(trimmed, data_dir) {
                return resolved;
            }
            let direct_path = PathBuf::from(trimmed);
            if direct_path.exists() {
                return direct_path;
            }
            warn!("Explicitly requested kernel '{}' not found locally, falling back to manifest/defaults", trimmed);
        }
    }

    // 2. Resolve from manifest (top-level, composite, or suggested)
    if let Some(manifest) = manifest {
        if let Some(kernel_ref) = &manifest.kernel {
            if let Some(resolved) = catalog.resolve_local_kernel_path(kernel_ref, Some(data_path)) {
                return resolved;
            }
            if let Some(resolved) = resolve_kernel_by_tag(kernel_ref, data_dir) {
                return resolved;
            }
        }
        if let Some(composite) = &manifest.composite_spec {
            if let Some(resolved) = catalog.resolve_local_kernel_path(&composite.kernel_ref, Some(data_path)) {
                return resolved;
            }
            if let Some(resolved) = resolve_kernel_by_tag(&composite.kernel_ref, data_dir) {
                return resolved;
            }
        }
        if let Some(software) = &manifest.software_spec {
            if let Some(ref suggested) = software.suggested_kernel {
                if let Some(resolved) = catalog.resolve_local_kernel_path(suggested, Some(data_path)) {
                    return resolved;
                }
                if let Some(resolved) = resolve_kernel_by_tag(suggested, data_dir) {
                    return resolved;
                }
            }
        }
    }

    // 3. Fallback to default bundled/system kernel
    get_default_kernel_path(data_dir)
}

pub fn resolve_kernel_from_manifest(manifest: &Option<vyoma_image::VmifManifest>, data_dir: &str) -> Option<PathBuf> {
    let kernel_ref = manifest.as_ref()?.kernel.as_ref()?;
    
    if kernel_ref.starts_with("sha256:") {
        let hash = kernel_ref.trim_start_matches("sha256:");
        resolve_kernel_by_hash(hash, data_dir)
    } else if kernel_ref.starts_with("kernels/") {
        resolve_kernel_by_tag(kernel_ref, data_dir)
    } else {
        resolve_kernel_by_tag(kernel_ref, data_dir)
    }
}

pub fn enforce_image_verification_policy(
    image_name: &str,
    manifest: &Option<vyoma_image::VmifManifest>,
) -> Result<()> {
    if let Some(manifest) = manifest {
        // Enforce verification status check: Flagged images are rejected
        if let Some(ref report) = manifest.verification_report {
            if report.status == vyoma_image::VerificationStatus::Flagged {
                anyhow::bail!(
                    "Security violation: Image '{}' is FLAGGED by security policy or verification checks and cannot be run.",
                    image_name
                );
            }
            if report.cve_critical_count > 10 {
                anyhow::bail!(
                    "Security violation: Image '{}' contains {} critical CVEs and cannot be run.",
                    image_name, report.cve_critical_count
                );
            }
        }
    }
    Ok(())
}

pub fn verify_rootfs_verity(rootfs_path: &Path, expected_roothash: &str) -> Result<()> {
    if !rootfs_path.exists() {
        return Err(anyhow::anyhow!("Rootfs path does not exist: {:?}", rootfs_path));
    }
    let data = std::fs::read(rootfs_path)
        .with_context(|| format!("Failed to read rootfs at {:?}", rootfs_path))?;
    let res = vyoma_image::verity::generate_verity_from_bytes(&data, None, 4096)
        .context("Failed to compute dm-verity tree for rootfs")?;
    vyoma_core::attest::verify_dm_verity_roothash(&res.spec.root_hash, expected_roothash)
        .context("Rootfs dm-verity root hash mismatch")?;
    Ok(())
}

fn resolve_kernel_by_hash(hash: &str, data_dir: &str) -> Option<PathBuf> {
    let kernel_store = std::path::Path::new(data_dir).join("kernels");
    let kernel_path = kernel_store.join(hash);
    if kernel_path.exists() {
        Some(kernel_path)
    } else {
        warn!("Kernel with hash {} not found in kernel store", hash);
        None
    }
}

fn resolve_kernel_by_tag(tag: &str, data_dir: &str) -> Option<PathBuf> {
    let kernel_store = std::path::Path::new(data_dir).join("kernels");
    
    if let Ok(entries) = std::fs::read_dir(&kernel_store) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name == tag || file_name == format!("{}.vmlinuz", tag) {
                return Some(entry.path());
            }
        }
    }
    
    warn!("Kernel with tag {} not found in kernel store", tag);
    None
}

pub fn get_default_kernel_path(data_dir: &str) -> PathBuf {
    std::path::Path::new(data_dir).join("bin/vmlinux")
}

pub async fn prepare_image_with_provider<P: ImageProvider>(
    image_name: &str,
    provider: &P,
) -> Result<PreparedImage> {
    info!("Preparing VMIF image: {}", image_name);
    let image_path = provider.fetch_image(image_name).await?;
    let config = provider.get_config(&image_path).await?;
    let manifest = provider.get_vmif_manifest(image_name).await?;

    // Enforce security verification policy
    enforce_image_verification_policy(image_name, &manifest)?;

    // Extract verity root hash from verity_spec, software_spec, or verification_report
    let verity_hash = manifest.as_ref().and_then(|m| {
        m.verity_spec.as_ref().map(|v| v.root_hash.as_str())
            .or_else(|| m.software_spec.as_ref().and_then(|s| s.verity_root_hash.as_deref()))
            .or_else(|| m.verification_report.as_ref().and_then(|r| r.verity_root_hash.as_deref()))
    });

    if let Some(expected_roothash) = verity_hash {
        info!("Enforcing dm-verity root hash verification for {}", image_name);
        verify_rootfs_verity(&image_path, expected_roothash)?;
        info!("dm-verity root hash verified for {}", image_name);
    }

    Ok(PreparedImage {
        rootfs_sqfs_path: image_path,
        manifest,
        config,
        kernel_path: None,
    })
}

pub fn extract_oci_config(image_path: &std::path::Path) -> Result<vyoma_core::oci::OciImageConfig> {
    let config_path = image_path.parent().unwrap().join("vyoma-config.json");

    if config_path.exists() {
        let config_str = std::fs::read_to_string(&config_path).context("Failed to read config")?;
        let config: vyoma_core::oci::OciImageConfig = serde_json::from_str(&config_str)
            .context("Failed to parse OCI config")?;
        Ok(config)
    } else {
        warn!("No OCI config found at {:?}, using defaults", config_path);
        Ok(vyoma_core::oci::OciImageConfig::default())
    }
}

pub async fn ensure_image_locally_handler(
    image_name: &str,
    data_dir: &str,
) -> Result<std::path::PathBuf, (axum::http::StatusCode, String)> {
    ensure_image_locally(image_name, data_dir)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vyoma_image::{VerificationReport, VerificationStatus, VmifManifest};
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_enforce_image_verification_policy() {
        // None manifest: allowed
        assert!(enforce_image_verification_policy("test:v1", &None).is_ok());

        // Certified manifest: allowed
        let mut manifest = VmifManifest::default();
        manifest.verification_report = Some(VerificationReport {
            report_version: 1,
            artifact_digest: "sha256:abc".to_string(),
            status: VerificationStatus::Certified,
            issuer: "vyoma-hub".to_string(),
            signed_at: "2026-11-22T00:00:00Z".to_string(),
            expires_at: None,
            static_scan_passed: true,
            cve_critical_count: 0,
            detonation_passed: true,
            golden_pcrs: std::collections::HashMap::new(),
            verity_root_hash: None,
            seal_signature: None,
        });
        assert!(enforce_image_verification_policy("test:v1", &Some(manifest.clone())).is_ok());

        // Flagged report: blocked
        manifest.verification_report = Some(VerificationReport {
            report_version: 1,
            artifact_digest: "sha256:bad".to_string(),
            status: VerificationStatus::Flagged,
            issuer: "vyoma-hub".to_string(),
            signed_at: "2026-11-22T00:00:00Z".to_string(),
            expires_at: None,
            static_scan_passed: false,
            cve_critical_count: 5,
            detonation_passed: false,
            golden_pcrs: std::collections::HashMap::new(),
            verity_root_hash: None,
            seal_signature: None,
        });
        let err = enforce_image_verification_policy("test:v1", &Some(manifest.clone())).unwrap_err();
        assert!(err.to_string().contains("is FLAGGED by security policy"));

        // Critical CVE threshold (>10): blocked
        manifest.verification_report = Some(VerificationReport {
            report_version: 1,
            artifact_digest: "sha256:cve".to_string(),
            status: VerificationStatus::Community,
            issuer: "community".to_string(),
            signed_at: "2026-11-22T00:00:00Z".to_string(),
            expires_at: None,
            static_scan_passed: true,
            cve_critical_count: 15,
            detonation_passed: true,
            golden_pcrs: std::collections::HashMap::new(),
            verity_root_hash: None,
            seal_signature: None,
        });
        let err2 = enforce_image_verification_policy("test:v1", &Some(manifest)).unwrap_err();
        assert!(err2.to_string().contains("contains 15 critical CVEs"));
    }

    #[test]
    fn test_resolve_kernel_priority() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_str().unwrap();

        // 1. Fallback default
        let resolved_default = resolve_kernel(None, &None, data_dir);
        assert_eq!(resolved_default, PathBuf::from(data_dir).join("bin/vmlinux"));

        // 2. Manifest kernel
        let mut manifest = VmifManifest::default();
        manifest.kernel = Some("kernels/microvm-v6.6".to_string());
        // Since kernel file doesn't exist locally, falls back cleanly
        let resolved_fallback = resolve_kernel(None, &Some(manifest.clone()), data_dir);
        assert_eq!(resolved_fallback, PathBuf::from(data_dir).join("bin/vmlinux"));

        // 3. Request kernel with existing file
        let mut kernel_file = NamedTempFile::new().unwrap();
        writeln!(kernel_file, "mock kernel binary").unwrap();
        let kernel_path_str = kernel_file.path().to_str().unwrap();

        let resolved_req = resolve_kernel(Some(kernel_path_str), &Some(manifest), data_dir);
        assert_eq!(resolved_req, PathBuf::from(kernel_path_str));
    }

    #[test]
    fn test_verify_rootfs_verity() {
        let mut temp_rootfs = NamedTempFile::new().unwrap();
        let payload = vec![0x42u8; 8192]; // 2 data blocks
        temp_rootfs.write_all(&payload).unwrap();
        temp_rootfs.flush().unwrap();

        let res = vyoma_image::verity::generate_verity_from_bytes(&payload, None, 4096).unwrap();

        // Matching hash succeeds
        assert!(verify_rootfs_verity(temp_rootfs.path(), &res.spec.root_hash).is_ok());

        // Tampered hash fails
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_rootfs_verity(temp_rootfs.path(), wrong_hash).is_err());
    }
}