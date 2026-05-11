//! Image build service for Vyoma
//!
//! Handles building container images from Vyomafile specifications.
//! This module provides the infrastructure for building images.
//!
//! # Follow-up Status
//!
//! The actual build logic is still in handlers.rs. This module provides
//! the structure. The build_image in handlers.rs needs to be refactored
//! to use this module after run_vm is proven stable.

use std::path::PathBuf;
use anyhow::{Context, Result};

pub struct BuildResult {
    pub build_id: String,
    pub image_path: PathBuf,
}

pub fn create_build_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn get_images_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home dir")?;
    Ok(home.join(".vyoma").join("images"))
}

pub fn get_image_path(build_id: &str) -> Result<PathBuf> {
    let images_root = get_images_root()?;
    let image_dir = images_root.join(build_id);
    Ok(image_dir.join("base.ext4"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_build_id() {
        let id = create_build_id();
        assert!(!id.is_empty());
    }

    #[test]
    fn test_get_images_root() {
        let root = get_images_root();
        assert!(root.is_ok());
    }
}