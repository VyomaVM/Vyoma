use crate::signing::{SigningKeyPair, SignedManifest};
use crate::vmif::{OciImageConfig, VmifManifest, VmifImage};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum ConverterError {
    #[error("Failed to create squashfs: {0}")]
    SquashfsError(String),
    #[error("mksquashfs not found in PATH")]
    MksquashfsNotFound,
    #[error("Squashfs creation failed: {0}")]
    SquashfsFailed(i32),
    #[error("Failed to compute rootfs hash: {0}")]
    HashError(String),
    #[error("Signing error: {0}")]
    SigningError(#[from] crate::signing::SigningError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML error: {0}")]
    Toml(String),
    #[error("VMIF error: {0}")]
    VmifError(#[from] crate::vmif::VmifError),
}

pub struct VmifConverter {
    signing_key: Option<SigningKeyPair>,
}

impl VmifConverter {
    pub fn new() -> Self {
        Self { signing_key: None }
    }

    pub fn with_signing_key(signing_key: SigningKeyPair) -> Self {
        Self {
            signing_key: Some(signing_key),
        }
    }

    pub fn create_squashfs(
        source_dir: &Path,
        dest_file: &Path,
        compression: SquashfsCompression,
    ) -> Result<(), ConverterError> {
        if !source_dir.exists() {
            return Err(ConverterError::SquashfsError(format!(
                "Source directory does not exist: {:?}",
                source_dir
            )));
        }

        if which_mksquashfs().is_none() {
            return Err(ConverterError::MksquashfsNotFound);
        }

        let comp_flag = match compression {
            SquashfsCompression::Zstd(level) => {
                vec!["-comp".to_string(), "zstd".to_string(), "-Xcompression-level".to_string(), level.to_string()]
            }
            SquashfsCompression::Gzip => {
                vec!["-comp".to_string(), "gzip".to_string()]
            }
            SquashfsCompression::Xz => {
                vec!["-comp".to_string(), "xz".to_string()]
            }
            SquashfsCompression::Lz4 => {
                vec!["-comp".to_string(), "lz4".to_string()]
            }
        };

        let mut args = vec![
            source_dir.to_string_lossy().to_string(),
            dest_file.to_string_lossy().to_string(),
        ];
        args.extend(comp_flag);
        args.push("-noappend".to_string());

        let output = Command::new("mksquashfs")
            .args(&args)
            .output()
            .map_err(|e| ConverterError::SquashfsError(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConverterError::SquashfsFailed(output.status.code().unwrap_or(-1)));
        }

        info!("Created squashfs at {:?}", dest_file);
        Ok(())
    }

    pub fn compute_squashfs_hash(squashfs_path: &Path) -> Result<String, ConverterError> {
        let data = std::fs::read(squashfs_path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash = hasher.finalize();
        Ok(hex::encode(hash))
    }

    pub fn convert_directory_to_vmif(
        &self,
        source_dir: &Path,
        dest_dir: &Path,
        image_name: &str,
        arch: &str,
        oci_config: OciImageConfig,
        kernel_ref: Option<String>,
        initrd_ref: Option<String>,
        compression: SquashfsCompression,
    ) -> Result<VmifImage, ConverterError> {
        std::fs::create_dir_all(dest_dir)?;
        let rootfs_sqfs_path = dest_dir.join("rootfs.sqfs");

        Self::create_squashfs(source_dir, &rootfs_sqfs_path, compression)?;
        let rootfs_hash = Self::compute_squashfs_hash(&rootfs_sqfs_path)?;
        let size_bytes = std::fs::metadata(&rootfs_sqfs_path)?.len();

        let manifest = VmifManifest::new(
            arch.to_string(),
            kernel_ref,
            initrd_ref,
            format!("sha256:{}", rootfs_hash),
            oci_config,
            size_bytes,
        );

        let manifest_path = dest_dir.join("vyoma.toml");
        self.write_manifest(&manifest, &manifest_path)?;

        let signed_manifest = self.sign_manifest(&manifest, &manifest_path)?;

        let mut vmif_image = VmifImage::new(manifest, rootfs_sqfs_path);
        if signed_manifest.is_some() {
            let sig_path = dest_dir.join("vyoma.toml.sig");
            if let Some(ref signed) = signed_manifest {
                signed.save_to_file(&sig_path)?;
            }
        }

        info!(
            "Converted {} to VMIF at {:?}",
            image_name,
            dest_dir
        );

        Ok(vmif_image)
    }

    fn sign_manifest(
        &self,
        manifest: &VmifManifest,
        _manifest_path: &Path,
    ) -> Result<Option<SignedManifest>, ConverterError> {
        if let Some(ref keypair) = self.signing_key {
            let signed = keypair.sign_manifest(manifest)?;
            info!("Manifest signed successfully");
            Ok(Some(signed))
        } else {
            Ok(None)
        }
    }

    fn write_manifest(
        &self,
        manifest: &VmifManifest,
        manifest_path: &Path,
    ) -> Result<(), ConverterError> {
        let content = toml::to_string_pretty(manifest)
            .map_err(|e| ConverterError::Toml(e.to_string()))?;
        std::fs::write(manifest_path, content)?;
        info!("Wrote manifest to {:?}", manifest_path);
        Ok(())
    }

    pub fn load_manifest(manifest_path: &Path) -> Result<VmifManifest, ConverterError> {
        let content = std::fs::read_to_string(manifest_path)?;
        let manifest: VmifManifest = toml::from_str(&content)
            .map_err(|e| ConverterError::Toml(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn load_signed_manifest(sig_path: &Path) -> Result<SignedManifest, ConverterError> {
        SignedManifest::load_from_file(&sig_path.to_path_buf()).map_err(ConverterError::from)
    }

    pub fn verify_image(dest_dir: &Path) -> Result<VmifImage, ConverterError> {
        let rootfs_sqfs_path = dest_dir.join("rootfs.sqfs");
        let manifest_path = dest_dir.join("vyoma.toml");

        if !rootfs_sqfs_path.exists() {
            return Err(ConverterError::SquashfsError(format!(
                "rootfs.sqfs not found at {:?}",
                rootfs_sqfs_path
            )));
        }

        if !manifest_path.exists() {
            return Err(ConverterError::SquashfsError(format!(
                "vyoma.toml not found at {:?}",
                manifest_path
            )));
        }

        let manifest = Self::load_manifest(&manifest_path)?;

        let expected_hash = manifest.rootfs.trim_start_matches("sha256:");
        let actual_hash = Self::compute_squashfs_hash(&rootfs_sqfs_path)?;

        if expected_hash != actual_hash {
            return Err(ConverterError::HashError(format!(
                "Rootfs hash mismatch: expected {}, got {}",
                expected_hash, actual_hash
            )));
        }

        let sig_path = dest_dir.join("vyoma.toml.sig");
        let mut vmif_image = VmifImage::new(manifest, rootfs_sqfs_path);

        if sig_path.exists() {
            if let Ok(signed) = Self::load_signed_manifest(&sig_path) {
                if signed.manifest == vmif_image.manifest {
                    info!("Signed manifest verified successfully");
                }
            } else {
                warn!("Failed to load signed manifest");
            }
        }

        Ok(vmif_image)
    }
}

impl Default for VmifConverter {
    fn default() -> Self {
        Self::new()
    }
}

pub enum SquashfsCompression {
    Zstd(u32),
    Gzip,
    Xz,
    Lz4,
}

impl Default for SquashfsCompression {
    fn default() -> Self {
        SquashfsCompression::Zstd(9)
    }
}

fn which_mksquashfs() -> Option<PathBuf> {
    std::env::var("PATH")
        .ok()
        .and_then(|paths| {
            paths.split(':').find_map(|p| {
                let path = PathBuf::from(p).join("mksquashfs");
                if path.exists() {
                    Some(path)
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            if PathBuf::from("/usr/bin/mksquashfs").exists() {
                Some(PathBuf::from("/usr/bin/mksquashfs"))
            } else if PathBuf::from("/sbin/mksquashfs").exists() {
                Some(PathBuf::from("/sbin/mksquashfs"))
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_vmif_converter_creation() {
        let converter = VmifConverter::new();
        assert!(converter.signing_key.is_none());
    }

    #[test]
    fn test_vmif_converter_with_signing_key() {
        let keypair = SigningKeyPair::generate();
        let converter = VmifConverter::with_signing_key(keypair);
        assert!(converter.signing_key.is_some());
    }

    #[test]
    fn test_squashfs_compression_default() {
        let compression = SquashfsCompression::default();
        match compression {
            SquashfsCompression::Zstd(level) => assert_eq!(level, 9),
            _ => panic!("Expected Zstd compression"),
        }
    }

    #[test]
    fn test_which_mksquashfs() {
        let result = which_mksquashfs();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_create_squashfs() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();

        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();
        std::fs::write(source_dir.join("test2.txt"), "test content").unwrap();

        let dest_file = temp_dir.path().join("rootfs.sqfs");

        let result = VmifConverter::create_squashfs(
            &source_dir,
            &dest_file,
            SquashfsCompression::default(),
        );

        assert!(result.is_ok());
        assert!(dest_file.exists());

        let metadata = std::fs::metadata(&dest_file).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_compute_squashfs_hash() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_file = temp_dir.path().join("rootfs.sqfs");
        VmifConverter::create_squashfs(&source_dir, &dest_file, SquashfsCompression::default()).unwrap();

        let hash = VmifConverter::compute_squashfs_hash(&dest_file).unwrap();
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn test_convert_directory_to_vmif() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_dir = temp_dir.path().join("image");

        let config = OciImageConfig::default();
        let converter = VmifConverter::new();

        let result = converter.convert_directory_to_vmif(
            &source_dir,
            &dest_dir,
            "test-image",
            "amd64",
            config,
            None,
            None,
            SquashfsCompression::default(),
        );

        assert!(result.is_ok());
        let vmif_image = result.unwrap();
        assert!(vmif_image.rootfs_path.exists());
        assert_eq!(vmif_image.manifest.arch, "amd64");
    }

    #[test]
    fn test_convert_directory_with_signing() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_dir = temp_dir.path().join("image");

        let keypair = SigningKeyPair::generate();
        let converter = VmifConverter::with_signing_key(keypair);
        let config = OciImageConfig::default();

        let result = converter.convert_directory_to_vmif(
            &source_dir,
            &dest_dir,
            "test-image",
            "amd64",
            config,
            None,
            None,
            SquashfsCompression::default(),
        );

        assert!(result.is_ok());
        let sig_path = dest_dir.join("vyoma.toml.sig");
        assert!(sig_path.exists());
    }

    #[test]
    fn test_load_and_verify_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_dir = temp_dir.path().join("image");

        let converter = VmifConverter::new();
        let config = OciImageConfig::default();

        converter
            .convert_directory_to_vmif(
                &source_dir,
                &dest_dir,
                "test-image",
                "amd64",
                config,
                Some("kernel:v1".to_string()),
                None,
                SquashfsCompression::default(),
            )
            .unwrap();

        let result = VmifConverter::verify_image(&dest_dir);
        assert!(result.is_ok());
        let vmif_image = result.unwrap();
        assert_eq!(vmif_image.manifest.kernel, Some("kernel:v1".to_string()));
    }

    #[test]
    fn test_verify_image_fails_without_rootfs() {
        let temp_dir = TempDir::new().unwrap();
        let dest_dir = temp_dir.path().join("image");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let result = VmifConverter::verify_image(&dest_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_image_fails_with_tampered_rootfs() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_dir = temp_dir.path().join("image");

        let converter = VmifConverter::new();
        let config = OciImageConfig::default();

        converter
            .convert_directory_to_vmif(
                &source_dir,
                &dest_dir,
                "test-image",
                "amd64",
                config,
                None,
                None,
                SquashfsCompression::default(),
            )
            .unwrap();

        std::fs::write(dest_dir.join("rootfs.sqfs"), "tampered content").unwrap();

        let result = VmifConverter::verify_image(&dest_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_with_labels() {
        let temp_dir = TempDir::new().unwrap();
        let source_dir = temp_dir.path().join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(source_dir.join("test.txt"), "hello world").unwrap();

        let dest_dir = temp_dir.path().join("image");
        std::fs::create_dir_all(&dest_dir).unwrap();

        let mut config = OciImageConfig::default();
        let mut labels = std::collections::HashMap::new();
        labels.insert("version".to_string(), "1.0".to_string());
        labels.insert("maintainer".to_string(), "test@example.com".to_string());

        let mut manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:temporary".to_string(),
            config.clone(),
            0,
        );
        manifest = manifest.with_labels(labels);

        let dest_file = dest_dir.join("rootfs.sqfs");
        VmifConverter::create_squashfs(&source_dir, &dest_file, SquashfsCompression::default()).unwrap();
        let hash = VmifConverter::compute_squashfs_hash(&dest_file).unwrap();
        manifest.rootfs = format!("sha256:{}", hash);
        manifest.size_bytes = std::fs::metadata(&dest_file).unwrap().len();

        let manifest_path = dest_dir.join("vyoma.toml");
        let content = toml::to_string_pretty(&manifest).unwrap();
        std::fs::write(&manifest_path, content).unwrap();

        let loaded = VmifConverter::load_manifest(&manifest_path).unwrap();
        assert_eq!(loaded.labels.get("version"), Some(&"1.0".to_string()));
        assert_eq!(loaded.labels.get("maintainer"), Some(&"test@example.com".to_string()));
    }
}
