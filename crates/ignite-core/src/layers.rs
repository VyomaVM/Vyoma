use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use tar::Archive;
use std::path::Path;
use tracing::info;

pub struct LayerManager;

impl LayerManager {
    /// Extracts a GZIP compressed tarball (layer content) to a specific directory.
    pub fn unpack_layer(layer_data: &[u8], target_dir: &Path) -> Result<()> {
        info!("Unpacking layer to {:?}", target_dir);
        
        let decoder = GzDecoder::new(layer_data);
        let mut archive = Archive::new(decoder);
        
        // We might need to handle whiteout files (.wh.) for OverlayFS semantics later if we do "Flattening" manually.
        // For now, we trust standard tar unpacking.
        // Note: Safe unpacking is critical. archive.unpack() attempts to prevent path traversal.
        
        archive.unpack(target_dir).map_err(|e| anyhow!("Failed to unpack layer: {}", e))
    }
}
