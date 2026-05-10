use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::{info, warn};
use async_trait::async_trait;
use vyoma_core::layers::LayerManager;
use vyoma_core::oci::OciManager;
use vyoma_core::storage::StorageManager;

use super::types::PreparedImage;

pub async fn ensure_image_locally(image_name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home dir")?;
    let images_root = home.join(".ignite").join("images");
    std::fs::create_dir_all(&images_root)?;

    let safe_image_name = image_name.replace('/', "_").replace(':', "_");
    let image_store_path = images_root.join(&safe_image_name);
    let base_image_file = image_store_path.join("base.ext4");

    if !base_image_file.exists() {
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

        let size_mb = 2048;
        StorageManager::create_empty_file(&base_image_file, size_mb)
            .context("Create empty file failed")?;
        StorageManager::format_ext4(&base_image_file)
            .context("Format ext4 failed")?;
        StorageManager::populate_image(&base_image_file, temp_unpack_dir.path())
            .context("Populate failed")?;
    } else {
        info!("Image found locally at {:?}", base_image_file);
    }

    Ok(base_image_file)
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf>;
    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig>;
}

pub struct OciImageProvider;

#[async_trait]
impl ImageProvider for OciImageProvider {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf> {
        ensure_image_locally(image_name).await
    }

    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig> {
        extract_oci_config(image_path)
    }
}

pub struct CachedImageProvider {
    cache_dir: PathBuf,
}

impl CachedImageProvider {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("No home dir")?;
        let cache_dir = home.join(".ignite").join("images");
        std::fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    pub fn get_cached_path(&self, image_name: &str) -> Option<PathBuf> {
        let sanitized = image_name.replace(':', "_").replace('/', "_");
        let image_dir = self.cache_dir.join(&sanitized);
        let ext4_path = image_dir.join("base.ext4");
        if ext4_path.exists() {
            Some(ext4_path)
        } else {
            None
        }
    }
}

#[async_trait]
impl ImageProvider for CachedImageProvider {
    async fn fetch_image(&self, image_name: &str) -> Result<PathBuf> {
        if let Some(cached) = self.get_cached_path(image_name) {
            info!("Using cached image for {}", image_name);
            return Ok(cached);
        }
        ensure_image_locally(image_name).await
    }

    async fn get_config(&self, image_path: &PathBuf) -> Result<vyoma_core::oci::OciImageConfig> {
        extract_oci_config(image_path)
    }
}

pub async fn prepare_image(image_name: &str) -> Result<PreparedImage> {
    prepare_image_with_provider(image_name, &OciImageProvider).await
}

pub async fn prepare_image_with_provider<P: ImageProvider>(
    image_name: &str,
    provider: &P,
) -> Result<PreparedImage> {
    info!("Preparing image: {}", image_name);
    let image_path = provider.fetch_image(image_name).await?;
    let config = provider.get_config(&image_path).await?;

    Ok(PreparedImage { path: image_path, config })
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
) -> Result<std::path::PathBuf, (axum::http::StatusCode, String)> {
    ensure_image_locally(image_name)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}