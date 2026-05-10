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