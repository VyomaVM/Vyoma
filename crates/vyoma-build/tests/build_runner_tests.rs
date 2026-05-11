use vyoma_build::runner::BuildRunner;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_build_runner_creation() {
    let temp_dir = TempDir::new().unwrap();
    let runner = BuildRunner::new(temp_dir.path().to_path_buf());
    
    assert!(!runner.measured);
    assert!(runner.signing_key_path.is_none());
}

#[tokio::test]
async fn test_build_runner_with_measured() {
    let temp_dir = TempDir::new().unwrap();
    let runner = BuildRunner::new(temp_dir.path().to_path_buf())
        .with_measured(true, Some("/tmp/test-key".to_string()));
    
    assert!(runner.measured);
    assert_eq!(runner.signing_key_path, Some("/tmp/test-key".to_string()));
}

#[tokio::test]
async fn test_build_runner_measured_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let runner = BuildRunner::new(temp_dir.path().to_path_buf())
        .with_measured(false, None);
    
    assert!(!runner.measured);
    assert!(runner.signing_key_path.is_none());
}
