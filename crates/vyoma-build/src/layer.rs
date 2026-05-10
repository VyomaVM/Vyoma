use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::info;

/// Represents a layer in the build process
#[derive(Debug, Clone)]
pub struct Layer {
    pub path: PathBuf,
    pub size: u64,
}

/// Operations for managing build layers
pub struct LayerManager;

impl LayerManager {
    /// Create a new layer from a base image
    pub fn create_from_base(base_path: &Path, work_dir: &Path) -> Result<Layer> {
        info!("Creating layer from base: {:?}", base_path);

        // For now, just copy the base file
        let layer_name = format!("layer_{}", chrono::Utc::now().timestamp());
        let layer_path = work_dir.join(format!("{}.sqfs", layer_name));
        std::fs::copy(base_path, &layer_path)?;

        let size = std::fs::metadata(&layer_path)?.len();

        Ok(Layer {
            path: layer_path,
            size,
        })
    }

    /// Commit changes to create a new layer
    pub fn commit_changes(current_layer: &Layer, work_dir: &Path) -> Result<Layer> {
        info!("Committing changes to new layer");

        // For now, just create a copy with a new name
        let new_layer_name = format!("layer_{}", chrono::Utc::now().timestamp());
        let new_layer_path = work_dir.join(format!("{}.sqfs", new_layer_name));
        std::fs::copy(&current_layer.path, &new_layer_path)?;

        let size = std::fs::metadata(&new_layer_path)?.len();

        Ok(Layer {
            path: new_layer_path,
            size,
        })
    }

    /// Clean up temporary layer files
    pub fn cleanup(layer: &Layer) -> Result<()> {
        if layer.path.exists() {
            std::fs::remove_file(&layer.path)?;
            info!("Cleaned up layer: {:?}", layer.path);
        }
        Ok(())
    }
}