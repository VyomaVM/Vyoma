use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn, error};
use vyoma_core::oci::OciImageConfig;
use vyoma_image::{VmifConverter, SquashfsCompression};
use vyoma_storage::{LoopManager, DmManager};

use crate::Instruction;

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
        let mut current_config = OciImageConfig {
            entrypoint: None,
            cmd: None,
            env: Some(Vec::new()),
            working_dir: None,
            exposed_ports: None,
            user: None,
        };

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
                    current_config.cmd = Some(args.clone());
                }
                Instruction::Entrypoint { args } => {
                    info!("Processing ENTRYPOINT {:?}", args);
                    current_config.entrypoint = Some(args.clone());
                }
                Instruction::Env { key, value } => {
                    info!("Processing ENV {}={}", key, value);
                    if let Some(ref mut env_vars) = current_config.env {
                        env_vars.push(format!("{}={}", key, value));
                    } else {
                        current_config.env = Some(vec![format!("{}={}", key, value)]);
                    }
                }
                Instruction::Workdir { path } => {
                    info!("Processing WORKDIR {}", path);
                    current_config.working_dir = Some(path.clone());
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
        info!("Executing RUN command in isolated VM: {}", command);

        // Create a temporary directory for this build step
        let build_temp_dir = tempfile::tempdir()
            .map_err(|e| BuildError::ExecutionError(format!("Failed to create temp dir: {}", e)))?;
        let temp_path = build_temp_dir.path();

        // Create COW file for the overlay
        let cow_file = temp_path.join("build.cow");
        let cow_size_mb = 1024; // 1GB should be enough for most build operations

        LoopManager::create_cow_file(&cow_file, cow_size_mb as u64)
            .map_err(|e| BuildError::ExecutionError(format!("Failed to create COW file: {}", e)))?;

        // Set up loop devices and DM snapshot
        let loop_mgr = LoopManager::new()
            .map_err(|e| BuildError::VmError(format!("Failed to create LoopManager: {}", e)))?;
        let dm_mgr = DmManager::new()
            .map_err(|e| BuildError::VmError(format!("Failed to create DmManager: {}", e)))?;

        // Attach base image to loop device
        let base_loop = loop_mgr.attach(rootfs_path)
            .map_err(|e| BuildError::VmError(format!("Failed to attach base loop: {}", e)))?;

        // Attach COW file to loop device
        let cow_loop = loop_mgr.attach(&cow_file)
            .map_err(|e| BuildError::VmError(format!("Failed to attach COW loop: {}", e)))?;

        // Create DM snapshot
        let dm_name = format!("build-{}", std::process::id());
        let dm_device = dm_mgr.create_snapshot(&dm_name, base_loop.path(), cow_loop.path())
            .map_err(|e| BuildError::VmError(format!("Failed to create DM snapshot: {}", e)))?;

        // Launch VM to execute the command
        let exit_code = self.execute_in_vm(&dm_device, command).await?;

        // Clean up VM resources
        if let Err(e) = dm_mgr.remove_snapshot(&dm_name) {
            error!("Failed to remove DM snapshot {}: {}", dm_name, e);
        }
        if let Err(e) = loop_mgr.detach(&cow_loop) {
            error!("Failed to detach COW loop: {}", e);
        }
        if let Err(e) = loop_mgr.detach(&base_loop) {
            error!("Failed to detach base loop: {}", e);
        }

        if exit_code == 0 {
            // Commit the changes - convert COW file to new squashfs
            info!("Command succeeded, committing changes to new layer");
            self.commit_cow_to_squashfs(&cow_file, temp_path).await
        } else {
            // Command failed, discard changes
            warn!("Command failed with exit code {}, discarding changes", exit_code);
            Err(BuildError::ExecutionError(format!("Build command failed with exit code {}", exit_code)))
        }
    }

    async fn execute_in_vm(&self, dm_device: &vyoma_storage::DmDevice, command: &str) -> Result<i32, BuildError> {
        // TODO: Implement actual VM launch with Cloud Hypervisor
        // For now, simulate success/failure based on command content
        warn!("VM execution not yet implemented - simulating based on command: {}", command);

        // Simulate some commands succeeding, others failing
        if command.contains("false") || command.contains("exit 1") {
            Ok(1) // Simulate failure
        } else {
            Ok(0) // Simulate success
        }
    }

    async fn commit_cow_to_squashfs(&self, cow_file: &Path, temp_dir: &Path) -> Result<PathBuf, BuildError> {
        // TODO: Properly convert COW file back to squashfs
        // For now, create a new squashfs file as placeholder
        let new_layer_name = format!("layer_{}.sqfs", chrono::Utc::now().timestamp());
        let new_layer_path = self.temp_dir.join(&new_layer_name);

        // Copy the COW file as a temporary stand-in
        std::fs::copy(cow_file, &new_layer_path)
            .map_err(|e| BuildError::LayerError(format!("Failed to create new layer: {}", e)))?;

        info!("Created new layer: {:?}", new_layer_path);
        Ok(new_layer_path)
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

        // Convert config types
        let image_config = vyoma_image::OciImageConfig {
            entrypoint: config.entrypoint.clone(),
            cmd: config.cmd.clone(),
            env: config.env.clone(),
            working_dir: config.working_dir.clone(),
            exposed_ports: config.exposed_ports.clone(),
            user: config.user.clone(),
        };

        let manifest = vyoma_image::VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            format!("sha256:{}", "placeholder"), // TODO: compute actual hash
            image_config,
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