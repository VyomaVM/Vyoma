use anyhow::Result;
use ignite_core::oci::OciManager;
use ignite_core::layers::LayerManager;
use tempfile::tempdir;
use serde_json::Value;

#[tokio::test]
async fn test_layer_pull_and_unpack() -> Result<()> {
    // 1. Setup
    let mut oci = OciManager::new();
    let image = "docker.io/library/alpine:latest";
    
    // 2. Get Manifest
    println!("Pulling manifest...");
    let manifest_json = oci.pull_manifest(image).await?;
    let manifest: Value = serde_json::from_str(&manifest_json)?;
    
    // 3. Find first layer digest
    // Note: This logic assumes V2 manifest structure from our OCI integration
    let layers = manifest["layers"].as_array().expect("Manifest should have layers");
    let first_layer = layers.first().expect("Should have at least one layer");
    let digest = first_layer["digest"].as_str().expect("Layer should have digest");
    
    println!("Pulling layer: {}", digest);
    
    // 4. Download Layer
    let layer_data = oci.pull_layer(image, digest).await?;
    assert!(!layer_data.is_empty(), "Layer data should not be empty");
    println!("Downloaded {} bytes", layer_data.len());
    
    // 5. Unpack
    let dir = tempdir()?;
    println!("Unpacking to {:?}", dir.path());
    LayerManager::unpack_layer(&layer_data, dir.path())?;
    
    // 6. Verify contents (Alpine usually has /bin or /etc)
    
    // One of these should likely exist in the first layer of Alpine
    // Actually, usually layers are additive. The first layer is often the Base Image.
    // Let's just check if *any* file exists.
    let count = std::fs::read_dir(dir.path())?.count();
    println!("Found {} entries in unpacked directory", count);
    assert!(count > 0, "Unpacked directory should not be empty");

    Ok(())
}
