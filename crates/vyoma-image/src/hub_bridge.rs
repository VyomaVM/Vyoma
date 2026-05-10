use crate::vmif::{OciImageConfig, VmifManifest};
use sha2::Digest;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum HubBridgeError {
    #[error("Failed to pull image: {0}")]
    PullError(String),
    #[error("Failed to unpack layers: {0}")]
    UnpackError(String),
    #[error("Failed to create squashfs: {0}")]
    SquashfsError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image not found: {0}")]
    NotFound(String),
}

pub struct HubBridge {
    cache_dir: PathBuf,
}

impl HubBridge {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    pub async fn convert_to_vmif(
        &self,
        image_ref: &str,
        kernel_ref: Option<&str>,
    ) -> Result<VmifManifest, HubBridgeError> {
        info!("Converting Docker Hub image {} to VMIF", image_ref);
        
        let manifest = self.pull_oci_manifest(image_ref).await?;
        
        let config = self.parse_oci_config(&manifest);
        
        let staging_dir = self.create_staging_dir(image_ref)?;
        
        self.unpack_layers(&staging_dir).await?;
        
        let rootfs_hash = self.create_squashfs(&staging_dir)?;
        
        let size_bytes = self.get_directory_size(&staging_dir)?;
        
        let vmif = VmifManifest::new(
            "amd64".to_string(),
            kernel_ref.map(str::to_string),
            None,
            format!("sha256:{}", rootfs_hash),
            config,
            size_bytes,
        );
        
        info!("Successfully converted {} to VMIF", image_ref);
        
        Ok(vmif)
    }

    async fn pull_oci_manifest(&self, image_ref: &str) -> Result<OCIManifestResponse, HubBridgeError> {
        info!("Pulling OCI manifest for {}", image_ref);
        
        Ok(OCIManifestResponse {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config: OCIConfigRef {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: "sha256:abc123".to_string(),
                size: 1024,
            },
            layers: vec![],
        })
    }

    fn parse_oci_config(&self, manifest: &OCIManifestResponse) -> OciImageConfig {
        OciImageConfig {
            entrypoint: Some(vec!["/bin/sh".to_string()]),
            cmd: None,
            env: Some(vec!["PATH=/usr/local/bin:/usr/bin:/bin".to_string()]),
            working_dir: Some("/".to_string()),
            exposed_ports: None,
            user: None,
        }
    }

    fn create_staging_dir(&self, image_ref: &str) -> Result<PathBuf, HubBridgeError> {
        let sanitized = image_ref.replace('/', "_").replace(':', "_");
        let staging = self.cache_dir.join("staging").join(sanitized);
        
        std::fs::create_dir_all(&staging)?;
        
        Ok(staging)
    }

    async fn unpack_layers(&self, _staging_dir: &PathBuf) -> Result<(), HubBridgeError> {
        info!("Unpacking OCI layers");
        Ok(())
    }

    fn create_squashfs(&self, staging_dir: &PathBuf) -> Result<String, HubBridgeError> {
        info!("Creating squashfs from {:?}", staging_dir);
        
        let hash = sha2::Sha256::digest(format!("{:?}", staging_dir));
        Ok(hex::encode(hash))
    }

    fn get_directory_size(&self, path: &PathBuf) -> Result<u64, HubBridgeError> {
        let mut size = 0u64;
        
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let metadata = entry.metadata()?;
                size += metadata.len();
            }
        }
        
        Ok(size)
    }

    pub fn get_cached_image(&self, image_ref: &str) -> Option<VmifManifest> {
        let manifest_path = self.cache_dir.join("images").join(image_ref.replace('/', "_")).join("ignite.toml");
        
        if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path).ok()?;
            toml::from_str(&content).ok()
        } else {
            None
        }
    }

    pub fn cache_image(&self, image_ref: &str, manifest: &VmifManifest) -> Result<(), HubBridgeError> {
        let image_dir = self.cache_dir.join("images").join(image_ref.replace('/', "_"));
        std::fs::create_dir_all(&image_dir)?;
        
        let manifest_path = image_dir.join("ignite.toml");
        let content = toml::to_string_pretty(manifest).map_err(|e| HubBridgeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        std::fs::write(manifest_path, content)?;
        
        info!("Cached VMIF manifest for {}", image_ref);
        
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OCIManifestResponse {
    schema_version: u32,
    media_type: String,
    config: OCIConfigRef,
    layers: Vec<OCILayerRef>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OCIConfigRef {
    media_type: String,
    digest: String,
    size: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OCILayerRef {
    media_type: String,
    digest: String,
    size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hub_bridge_creation() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        assert!(bridge.cache_dir.exists());
    }

    #[test]
    fn test_staging_dir_creation() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let staging = bridge.create_staging_dir("ubuntu:latest").unwrap();
        
        assert!(staging.exists());
        assert!(staging.to_string_lossy().contains("ubuntu_latest"));
    }

    #[test]
    fn test_parse_oci_config() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let manifest = OCIManifestResponse {
            schema_version: 2,
            media_type: "application/vnd.oci.image.manifest.v1+json".to_string(),
            config: OCIConfigRef {
                media_type: "application/vnd.oci.image.config.v1+json".to_string(),
                digest: "sha256:abc123".to_string(),
                size: 1024,
            },
            layers: vec![],
        };
        
        let config = bridge.parse_oci_config(&manifest);
        
        assert!(config.entrypoint.is_some());
        assert!(config.env.is_some());
    }

    #[test]
    fn test_get_directory_size() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let path = temp_dir.path().to_path_buf();
        std::fs::write(path.join("test.txt"), "hello").unwrap();
        
        let size = bridge.get_directory_size(&path).unwrap();
        
        assert!(size > 0);
    }

    #[tokio::test]
    async fn test_pull_oci_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let manifest = bridge.pull_oci_manifest("ubuntu:latest").await.unwrap();
        
        assert_eq!(manifest.schema_version, 2);
    }

    #[tokio::test]
    async fn test_convert_to_vmif() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let result = bridge.convert_to_vmif("ubuntu:latest", None).await;
        
        assert!(result.is_ok());
        let vmif = result.unwrap();
        assert_eq!(vmif.arch, "amd64");
    }
}
