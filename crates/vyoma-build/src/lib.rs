use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, error};
use vyoma_core::oci::OciImageConfig;

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
}