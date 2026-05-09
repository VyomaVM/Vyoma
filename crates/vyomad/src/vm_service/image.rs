use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::api::handlers::ensure_image_locally;
use crate::vm_service::types::PreparedImage;

pub async fn prepare_image(image_name: &str) -> Result<PreparedImage> {
    info!("Preparing image: {}", image_name);
    let image_path = ensure_image_locally(image_name)
        .await
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let config = extract_oci_config(&image_path)?;

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