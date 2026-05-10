use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn};
use vyoma_core::oci::OciImageConfig;
use vyoma_image::{VmifConverter, SquashfsCompression};

use crate::{BuildResult, BuildError, Vyomafile};

/// Core build engine that executes Vyomafile instructions in isolated VMs
pub struct BuildRunner {
    work_dir: PathBuf,
    temp_dir: PathBuf,
}

impl BuildRunner {
    pub fn new(work_dir: PathBuf) -> Self {
        let temp_dir = work_dir.join("temp");
        Self { work_dir, temp_dir }
    }

    /// Execute a complete build from Vyomafile
    pub async fn build(
        &self,
        vyomafile_path: &Path,
        context_dir: &Path,
        image_name: &str,
    ) -> Result<BuildResult, BuildError> {
        info!("Starting VM-isolated build for {}", image_name);

        // Parse Vyomafile
        let vyomafile = Vyomafile::parse(vyomafile_path)
            .map_err(|e| BuildError::ParseError(e.to_string()))?;

        // Initialize build state
        let mut current_rootfs: Option<PathBuf> = None;
        let mut current_config = OciImageConfig::default();

        // Process each instruction
        for instruction in &vyomafile.instructions {
            match instruction {
                Instruction::From { image } => {
                    info!("Processing FROM {}", image);
                    current_rootfs = Some(self.handle_from(image).await?);
                }
                Instruction::Run { command } => {
                    info!("Processing RUN {}", command);
                    if let Some(ref rootfs) = current_rootfs {
                        current_rootfs = Some(self.handle_run(rootfs, command).await?);
                    } else {
                        return Err(BuildError::ExecutionError(
                            "RUN instruction without FROM".to_string()
                        ));
                    }
                }
                Instruction::Copy { src, dst } => {
                    info!("Processing COPY {} -> {}", src, dst);
                    if let Some(ref rootfs) = current_rootfs {
                        self.handle_copy(rootfs, context_dir, src, dst).await?;
                    } else {
                        return Err(BuildError::ExecutionError(
                            "COPY instruction without FROM".to_string()
                        ));
                    }
                }
                Instruction::Cmd { args } => {
                    info!("Processing CMD {:?}", args);
                    current_config.cmd = args.clone();
                }
                Instruction::Entrypoint { args } => {
                    info!("Processing ENTRYPOINT {:?}", args);
                    current_config.entrypoint = Some(args.clone());
                }
                Instruction::Env { key, value } => {
                    info!("Processing ENV {}={}", key, value);
                    current_config.env.push(format!("{}={}", key, value));
                }
                Instruction::Workdir { path } => {
                    info!("Processing WORKDIR {}", path);
                    current_config.working_dir = path.clone();
                }
            }
        }

        // Finalize the image
        if let Some(final_rootfs) = current_rootfs {
            self.finalize_image(&final_rootfs, image_name, &current_config).await
        } else {
            Err(BuildError::ExecutionError(
                "No FROM instruction found".to_string()
            ))
        }
    }

    async fn handle_from(&self, image: &str) -> Result<PathBuf, BuildError> {
        // For now, we'll assume the image is already available locally
        // In a real implementation, this would call ensure_image_locally
        let image_path = self.work_dir.join("images").join(image.replace('/', "_").replace(':', "_"));
        let rootfs_path = image_path.join("rootfs.sqfs");

        if !rootfs_path.exists() {
            return Err(BuildError::ExecutionError(
                format!("Base image {} not found", image)
            ));
        }

        Ok(rootfs_path)
    }

    async fn handle_run(&self, rootfs_path: &Path, command: &str) -> Result<PathBuf, BuildError> {
        // TODO: Implement VM execution for RUN commands
        warn!("RUN command '{}' not yet implemented - using mock implementation", command);

        // For now, return the same rootfs path (no-op)
        Ok(rootfs_path.to_path_buf())
    }

    async fn handle_copy(&self, rootfs_path: &Path, context_dir: &Path, src: &str, dst: &str) -> Result<(), BuildError> {
        // TODO: Implement file injection using debugfs
        warn!("COPY {} -> {} not yet implemented - using mock implementation", src, dst);

        // For now, just log the operation
        let src_path = context_dir.join(src);
        if !src_path.exists() {
            return Err(BuildError::InjectionError(
                format!("Source path {} does not exist", src)
            ));
        }

        Ok(())
    }

    async fn finalize_image(
        &self,
        rootfs_path: &Path,
        image_name: &str,
        config: &OciImageConfig,
    ) -> Result<BuildResult, BuildError> {
        info!("Finalizing image {}", image_name);

        // Create output directory
        let output_dir = self.work_dir.join("builds").join(image_name.replace('/', "_").replace(':', "_"));
        std::fs::create_dir_all(&output_dir)?;

        // Copy the final rootfs
        let final_rootfs = output_dir.join("rootfs.sqfs");
        std::fs::copy(rootfs_path, &final_rootfs)?;

        // Create manifest
        let converter = VmifConverter::new();
        let manifest_path = output_dir.join("vyoma.toml");

        let manifest = vyoma_image::VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            format!("sha256:{}", "placeholder"), // TODO: compute actual hash
            config.clone(),
            std::fs::metadata(&final_rootfs)?.len(),
        );

        let content = toml::to_string_pretty(&manifest)
            .map_err(|e| BuildError::ExecutionError(e.to_string()))?;
        std::fs::write(&manifest_path, content)?;

        Ok(BuildResult {
            image_name: image_name.to_string(),
            rootfs_path: final_rootfs,
            manifest_path,
            config: config.clone(),
        })
    }
}

impl Default for BuildRunner {
    fn default() -> Self {
        Self::new(PathBuf::from("/tmp/vyoma-build"))
    }
}