#[tokio::test]
async fn test_docker_hub_pull_manifest() {
    // This integration test requires network access.
    // It verifies we can talk to Docker Hub anonymously.
    
    use vyoma_core::oci::OciManager;

    let mut manager = OciManager::new();
    let image = "docker.io/library/alpine:latest";
    
    println!("Attempting to pull manifest for {}", image);
    match manager.pull_manifest(image).await {
        Ok(manifest) => {
            println!("Successfully pulled manifest!");
            assert!(manifest.contains("schemaVersion"));
        }
        Err(e) => {
            panic!("Failed to pull manifest: {}", e);
        }
    }
}
