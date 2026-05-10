use crate::converter::{VmifConverter, SquashfsCompression};
use crate::vmif::{OciImageConfig, VmifManifest, VmifImage};
use sha2::Digest;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

#[derive(Error, Debug)]
pub enum HubBridgeError {
    #[error("Failed to pull image: {0}")]
    PullError(String),
    #[error("Failed to unpack layers: {0}")]
    UnpackError(String),
    #[error("Failed to create squashfs: {0}")]
    SquashfsError(String),
    #[error("OCI layer unpack failed: {0}")]
    LayerUnpackFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image not found: {0}")]
    NotFound(String),
    #[error("Manifest conversion error: {0}")]
    ConversionError(String),
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
    ) -> Result<VmifImage, HubBridgeError> {
        info!("Converting Docker Hub image {} to VMIF", image_ref);
        
        let mut oci = OciManagerHub::new();
        let manifest_json = self.pull_oci_manifest(&mut oci, image_ref).await?;
        
        let layers = self.parse_layers(&manifest_json)?;
        let config = self.extract_oci_config(&mut oci, image_ref, &manifest_json).await?;
        
        let staging_dir = self.create_staging_dir(image_ref)?;
        
        self.unpack_layers(&mut oci, image_ref, &layers, &staging_dir).await?;
        
        let image_dir = self.get_image_dir(image_ref);
        std::fs::create_dir_all(&image_dir)?;
        
        let converter = VmifConverter::new();
        let vmif_image = converter.convert_directory_to_vmif(
            &staging_dir,
            &image_dir,
            image_ref,
            "amd64",
            config,
            kernel_ref.map(str::to_string),
            None,
            SquashfsCompression::default(),
        ).map_err(|e| HubBridgeError::ConversionError(e.to_string()))?;
        
        std::fs::remove_dir_all(&staging_dir).ok();
        
        info!("Successfully converted {} to VMIF", image_ref);
        
        Ok(vmif_image)
    }

    pub async fn pull_and_convert(
        &self,
        image_ref: &str,
        kernel_ref: Option<&str>,
    ) -> Result<VmifImage, HubBridgeError> {
        self.convert_to_vmif(image_ref, kernel_ref).await
    }

    async fn pull_oci_manifest(
        &self,
        oci: &mut OciManagerHub,
        image_ref: &str,
    ) -> Result<String, HubBridgeError> {
        info!("Pulling OCI manifest for {}", image_ref);
        oci.pull_manifest(image_ref)
            .await
            .map_err(|e| HubBridgeError::PullError(e.to_string()))
    }

    fn parse_layers(&self, manifest_json: &str) -> Result<Vec<String>, HubBridgeError> {
        #[derive(serde::Deserialize)]
        struct ManifestV2 {
            layers: Vec<LayerDesc>,
        }
        #[derive(serde::Deserialize)]
        struct LayerDesc {
            digest: String,
        }
        
        let v2: ManifestV2 = serde_json::from_str(manifest_json)
            .map_err(|e| HubBridgeError::PullError(format!("Failed to parse manifest: {}", e)))?;
        
        Ok(v2.layers.into_iter().map(|l| l.digest).collect())
    }

    async fn extract_oci_config(
        &self,
        oci: &mut OciManagerHub,
        image_ref: &str,
        manifest_json: &str,
    ) -> Result<OciImageConfig, HubBridgeError> {
        #[derive(serde::Deserialize)]
        struct ManifestV2 {
            config: ConfigDesc,
        }
        #[derive(serde::Deserialize)]
        struct ConfigDesc {
            digest: String,
        }
        
        let v2: ManifestV2 = serde_json::from_str(manifest_json)
            .map_err(|e| HubBridgeError::PullError(format!("Failed to parse manifest: {}", e)))?;
        
        let config_blob = oci.pull_config_blob(image_ref, &v2.config.digest)
            .await
            .map_err(|e| HubBridgeError::PullError(e.to_string()))?;
        
        Ok(OciImageConfig {
            entrypoint: config_blob.config.entrypoint,
            cmd: config_blob.config.cmd,
            env: config_blob.config.env,
            working_dir: config_blob.config.working_dir,
            exposed_ports: config_blob.config.exposed_ports,
            user: config_blob.config.user,
        })
    }

    fn create_staging_dir(&self, image_ref: &str) -> Result<PathBuf, HubBridgeError> {
        let sanitized = image_ref.replace('/', "_").replace(':', "_");
        let staging = self.cache_dir.join("staging").join(sanitized);
        std::fs::create_dir_all(&staging)?;
        Ok(staging)
    }

    async fn unpack_layers(
        &self,
        oci: &mut OciManagerHub,
        image_ref: &str,
        layers: &[String],
        staging_dir: &Path,
    ) -> Result<(), HubBridgeError> {
        info!("Unpacking {} OCI layers", layers.len());
        
        for (i, digest) in layers.iter().enumerate() {
            info!("Unpacking layer {} ({})", i + 1, digest);
            let layer_data = oci.pull_layer(image_ref, digest)
                .await
                .map_err(|e| HubBridgeError::PullError(e.to_string()))?;
            
            self.unpack_layer(&layer_data, staging_dir)
                .map_err(|e| HubBridgeError::LayerUnpackFailed(e.to_string()))?;
        }
        
        Ok(())
    }

    fn unpack_layer(&self, data: &[u8], dest: &Path) -> Result<(), HubBridgeError> {
        let mut archive = tar::Archive::new(data);
        archive.unpack(dest)
            .map_err(|e| HubBridgeError::UnpackError(e.to_string()))?;
        Ok(())
    }

    fn get_image_dir(&self, image_ref: &str) -> PathBuf {
        self.cache_dir.join("images").join(image_ref.replace('/', "_").replace(':', "_"))
    }

    pub fn get_cached_image(&self, image_ref: &str) -> Option<VmifManifest> {
        let image_dir = self.get_image_dir(image_ref);
        let manifest_path = image_dir.join("vyoma.toml");
        
        if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path).ok()?;
            toml::from_str(&content).ok()
        } else {
            None
        }
    }

    pub fn cache_image(&self, image_ref: &str, manifest: &VmifManifest) -> Result<(), HubBridgeError> {
        let image_dir = self.get_image_dir(image_ref);
        std::fs::create_dir_all(&image_dir)?;
        
        let manifest_path = image_dir.join("vyoma.toml");
        let content = toml::to_string_pretty(manifest)
            .map_err(|e| HubBridgeError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        
        std::fs::write(manifest_path, content)?;
        info!("Cached VMIF manifest for {}", image_ref);
        Ok(())
    }
}

struct OciManagerHub {
    client: reqwest::Client,
}

impl OciManagerHub {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    async fn pull_manifest(&mut self, image: &str) -> anyhow::Result<String> {
        let (registry, repository, tag) = self.parse_image_ref(image);
        let proto = if registry.contains("localhost") { "http" } else { "https" };
        let manifest_url = format!("{}://{}/v2/{}/manifests/{}", proto, registry, repository, tag);
        
        info!("Fetching manifest from: {}", manifest_url);
        
        let mut req = self.client.get(&manifest_url)
            .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json");
        
        let resp = req.send().await?;
        
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch manifest: {}", resp.status()));
        }
        
        let body = resp.text().await?;
        Ok(body)
    }

    async fn pull_layer(&mut self, image: &str, digest: &str) -> anyhow::Result<Vec<u8>> {
        let (registry, repository, _) = self.parse_image_ref(image);
        let proto = if registry.contains("localhost") { "http" } else { "https" };
        let layer_url = format!("{}://{}/v2/{}/blobs/{}", proto, registry, repository, digest);
        
        info!("Fetching layer blob: {}", digest);
        
        let resp = self.client.get(&layer_url).send().await?;
        
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to fetch layer {}: {}", digest, resp.status()));
        }
        
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    async fn pull_config_blob(&mut self, image: &str, digest: &str) -> anyhow::Result<OciConfigBlob> {
        let bytes = self.pull_layer(image, digest).await?;
        let full_config: OciConfigBlob = serde_json::from_slice(&bytes)?;
        Ok(full_config)
    }

    fn parse_image_ref(&self, image: &str) -> (String, String, String) {
        let (rest, tag) = if let Some((r, t)) = image.rsplit_once(':') {
            (r, t)
        } else {
            (image, "latest")
        };

        let (registry, repository) = if let Some((reg, repo)) = rest.split_once('/') {
            if reg.contains('.') || (reg.contains(':') && !reg.contains("docker.io")) || reg == "localhost" {
                (reg, repo)
            } else {
                ("registry-1.docker.io", rest)
            }
        } else {
            ("registry-1.docker.io", rest)
        };

        let final_repo = if registry == "registry-1.docker.io" && !repository.contains('/') {
            format!("library/{}", repository)
        } else {
            repository.to_string()
        };

        let final_reg = if registry == "docker.io" { "registry-1.docker.io" } else { registry };

        (final_reg.to_string(), final_repo, tag.to_string())
    }
}

#[derive(serde::Deserialize, Debug)]
struct OciConfigBlob {
    architecture: String,
    os: String,
    config: OciImageConfig,
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
    fn test_get_image_dir() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let image_dir = bridge.get_image_dir("ubuntu:latest");
        assert!(image_dir.to_string_lossy().contains("ubuntu_latest"));
    }

    #[test]
    fn test_image_dir_sanitization() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let image_dir = bridge.get_image_dir("my.registry.com:5000/ubuntu:latest");
        assert!(image_dir.to_string_lossy().contains("my.registry.com_5000_ubuntu_latest"));
    }

    #[test]
    fn test_cache_image_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let bridge = HubBridge::new(temp_dir.path().to_path_buf());
        
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            Some("kernel:v1".to_string()),
            None,
            "sha256:test123".to_string(),
            OciImageConfig::default(),
            1024,
        );
        
        bridge.cache_image("test:latest", &manifest).unwrap();
        
        let loaded = bridge.get_cached_image("test:latest");
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.arch, "amd64");
        assert_eq!(loaded.rootfs, "sha256:test123");
    }

    #[test]
    fn test_oci_manager_parse_image_ref() {
        let oci = OciManagerHub::new();
        
        let (reg, repo, tag) = oci.parse_image_ref("ubuntu:latest");
        assert_eq!(reg, "registry-1.docker.io");
        assert_eq!(repo, "library/ubuntu");
        assert_eq!(tag, "latest");
        
        let (reg, repo, tag) = oci.parse_image_ref("my.registry.com:5000/ubuntu:v1");
        assert_eq!(reg, "my.registry.com:5000");
        assert_eq!(repo, "ubuntu");
        assert_eq!(tag, "v1");
        
        let (reg, repo, tag) = oci.parse_image_ref("localhost:5000/test:latest");
        assert_eq!(reg, "localhost:5000");
        assert_eq!(repo, "test");
        assert_eq!(tag, "latest");
    }
}
