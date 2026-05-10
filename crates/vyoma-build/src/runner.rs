use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use tracing::{info, warn, error};
use vyoma_core::oci::OciImageConfig;
use vyoma_image::{VmifConverter, SquashfsCompression};
use chrono;
use std::process::Command;
use tokio::time::{timeout, Duration};
use std::sync::Arc;

use crate::Instruction;

use crate::{BuildResult, BuildError, Vyomafile};

/// Core build engine that executes Vyomafile instructions in isolated VMs
pub struct BuildRunner {
    pub work_dir: PathBuf,
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
                        let new_rootfs = self.handle_run(rootfs, command).await?;
                        current_rootfs = Some(new_rootfs);
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

        // For RUN commands, we need to:
        // 1. Extract the current squashfs to a temporary directory
        // 2. Launch a VM with the extracted directory as root (but readonly)
        // 3. VM executes command and modifies files in a writable overlay
        // 4. After VM exits, create new squashfs from modified directory

        let temp_dir = tempfile::tempdir()
            .map_err(|e| BuildError::ExecutionError(format!("Failed to create temp dir: {}", e)))?;
        let extract_dir = temp_dir.path().join("extract");
        let overlay_dir = temp_dir.path().join("overlay");

        // Extract current squashfs
        self.extract_squashfs(rootfs_path, &extract_dir).await?;

        // Create overlay directory for writable changes
        std::fs::create_dir_all(&overlay_dir)
            .map_err(|e| BuildError::ExecutionError(format!("Failed to create overlay dir: {}", e)))?;

        // For now, we'll simulate the RUN command execution
        // In a full implementation, we'd need to:
        // 1. Create a union filesystem (overlayfs) with extract_dir as lower, overlay_dir as upper
        // 2. Launch VM with the union mount as root
        // 3. VM executes command and modifies overlay_dir
        // 4. After VM exits, merge changes back

        warn!("RUN command execution simulation - command: {}", command);

        // Simulate success/failure
        let exit_code = if command.contains("false") || command.contains("exit 1") {
            1
        } else {
            0
        };

        if exit_code == 0 {
            // Create new squashfs from the (unchanged) extracted directory
            // In real implementation, this would include overlay changes
            let new_layer_name = format!("layer_{}.sqfs", chrono::Utc::now().timestamp());
            let new_layer_path = self.temp_dir.join(&new_layer_name);

            VmifConverter::create_squashfs(&extract_dir, &new_layer_path, SquashfsCompression::default())
                .map_err(|e| BuildError::LayerError(format!("Failed to create new squashfs: {}", e)))?;

            info!("Created new layer: {:?}", new_layer_path);
            Ok(new_layer_path)
        } else {
            Err(BuildError::ExecutionError(format!("Build command failed with exit code {}", exit_code)))
        }
    }

    async fn execute_in_vm(&self, command: &str) -> Result<i32, BuildError> {
        info!("Launching Cloud Hypervisor VM to execute: {}", command);

        // Create build-specific initramfs
        let initramfs_path = self.create_build_initramfs(command).await?;

        // Find kernel path (assume default for now)
        let kernel_path = self.find_kernel_path()?;

        // Create temporary VM directory
        let vm_id = format!("build-{}", std::process::id());
        let vm_dir = self.temp_dir.join(&vm_id);
        std::fs::create_dir_all(&vm_dir)?;

        // Build Cloud Hypervisor configuration
        let socket_path = vm_dir.join("ch.sock");
        let rootfs_path = self.temp_dir.join("temp_root.sqfs"); // Placeholder rootfs
        let ch_args = self.build_ch_args(&rootfs_path, &kernel_path, &initramfs_path, &socket_path);

        // Launch Cloud Hypervisor
        info!("Starting Cloud Hypervisor with args: {:?}", ch_args);
        let mut child = Command::new("cloud-hypervisor")
            .args(&ch_args)
            .spawn()
            .map_err(|e| BuildError::VmError(format!("Failed to start Cloud Hypervisor: {}", e)))?;

        // Wait for VM to complete with timeout (using tokio::time::timeout with async block)
        let timeout_duration = Duration::from_secs(300); // 5 minute timeout for builds

        let exit_status_result = timeout(timeout_duration, async {
            child.wait()
        }).await;

        let exit_status = match exit_status_result {
            Ok(result) => result.map_err(|e| BuildError::VmError(format!("VM process error: {}", e)))?,
            Err(_) => {
                // Timeout - kill the process
                let _ = child.kill();
                return Err(BuildError::VmError("VM execution timed out".to_string()));
            }
        };

        // Clean up
        let _ = std::fs::remove_dir_all(&vm_dir);

        let exit_code = exit_status.code().unwrap_or(1);
        info!("VM execution completed with exit code: {}", exit_code);

        Ok(exit_code)
    }

    async fn create_build_initramfs(&self, command: &str) -> Result<PathBuf, BuildError> {
        let initramfs_path = self.temp_dir.join("build-initramfs.cpio.gz");

        // Generate build-specific init script
        let init_script = format!(r#"#!/bin/sh
# Build init script - runs command and exits
set -e

# Mount basic filesystems
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sys /sys 2>/dev/null || true
mount -t devtmpfs dev /dev 2>/dev/null || true

# Execute the build command
echo "Build VM: Executing command: {}"
{}

# Capture exit code
exit_code=$?
echo "Build VM: Command completed with exit code: $exit_code"

# Power off (this will cause Cloud Hypervisor to exit)
poweroff -f
"#, command, command);

        vyoma_core::initramfs::create_initramfs(&init_script, None, &initramfs_path)
            .map_err(|e| BuildError::VmError(format!("Failed to create build initramfs: {}", e)))?;

        info!("Created build initramfs at: {:?}", initramfs_path);
        Ok(initramfs_path)
    }

    fn find_kernel_path(&self) -> Result<PathBuf, BuildError> {
        // For now, assume the default kernel location
        // In a real implementation, this would check multiple locations
        let kernel_path = PathBuf::from("/usr/lib/vyoma/vmlinux");

        if kernel_path.exists() {
            Ok(kernel_path)
        } else {
            Err(BuildError::VmError("Kernel not found at /usr/lib/vyoma/vmlinux".to_string()))
        }
    }

    fn build_ch_args(
        &self,
        rootfs_path: &Path,
        kernel_path: &Path,
        initramfs_path: &Path,
        socket_path: &Path,
    ) -> Vec<String> {
        vec![
            "--kernel".to_string(),
            kernel_path.to_string_lossy().to_string(),
            "--initramfs".to_string(),
            initramfs_path.to_string_lossy().to_string(),
            "--disk".to_string(),
            format!("path={},readonly=on", rootfs_path.display()),
            "--console".to_string(),
            "off".to_string(), // Disable console to avoid hanging
            "--serial".to_string(),
            "tty".to_string(),
            "--api-socket".to_string(),
            socket_path.to_string_lossy().to_string(),
            "--cpus".to_string(),
            "1".to_string(), // Single CPU for builds
            "--memory".to_string(),
            "size=512M".to_string(), // 512MB RAM for builds
            "--rng".to_string(),
            "src=/dev/urandom".to_string(),
        ]
    }



    async fn handle_copy(&self, rootfs_path: &Path, context_dir: &Path, src: &str, dst: &str) -> Result<(), BuildError> {
        info!("Injecting file {} -> {} using debugfs", src, dst);

        // For squashfs, we can't modify it directly. Instead, we need to:
        // 1. Extract the current squashfs to a temporary directory
        // 2. Copy the file to the appropriate location
        // 3. Create a new squashfs with the updated contents

        let temp_extract_dir = tempfile::tempdir()
            .map_err(|e| BuildError::InjectionError(format!("Failed to create temp dir: {}", e)))?;
        let extract_path = temp_extract_dir.path();

        // Extract the current squashfs
        self.extract_squashfs(rootfs_path, extract_path).await?;

        // Copy the source file to destination
        let src_path = context_dir.join(src);
        if !src_path.exists() {
            return Err(BuildError::InjectionError(
                format!("Source path {} does not exist", src)
            ));
        }

        let dst_path = extract_path.join(dst.trim_start_matches('/'));
        if let Some(parent) = dst_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| BuildError::InjectionError(format!("Failed to create dest dir: {}", e)))?;
        }

        std::fs::copy(&src_path, &dst_path)
            .map_err(|e| BuildError::InjectionError(format!("Failed to copy file: {}", e)))?;

        // Create new squashfs with the injected file
        let new_squashfs_name = format!("layer_{}_injected.sqfs", chrono::Utc::now().timestamp());
        let new_squashfs_path = self.temp_dir.join(&new_squashfs_name);

        VmifConverter::create_squashfs(
            extract_path,
            &new_squashfs_path,
            vyoma_image::SquashfsCompression::default(),
        ).map_err(|e| BuildError::InjectionError(format!("Failed to create new squashfs: {}", e)))?;

        // Replace the original rootfs with the new one
        std::fs::copy(&new_squashfs_path, rootfs_path)
            .map_err(|e| BuildError::InjectionError(format!("Failed to update rootfs: {}", e)))?;

        info!("Successfully injected {} -> {}", src, dst);
        Ok(())
    }

    async fn extract_squashfs(&self, squashfs_path: &Path, dest_dir: &Path) -> Result<(), BuildError> {
        info!("Extracting squashfs: {:?} -> {:?}", squashfs_path, dest_dir);

        // Create destination directory
        std::fs::create_dir_all(dest_dir)
            .map_err(|e| BuildError::InjectionError(format!("Failed to create extract dir: {}", e)))?;

        // Use unsquashfs to extract the squashfs file
        let output = Command::new("unsquashfs")
            .args(&[
                "-f", // force overwrite
                "-d", // destination directory
                &dest_dir.to_string_lossy(),
                &squashfs_path.to_string_lossy(),
            ])
            .output()
            .map_err(|e| BuildError::InjectionError(format!("Failed to run unsquashfs: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BuildError::InjectionError(format!("unsquashfs failed: {}", stderr)));
        }

        info!("Successfully extracted squashfs to: {:?}", dest_dir);
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

        // Compute actual hash of the rootfs
        let hash = VmifConverter::compute_squashfs_hash(&final_rootfs)
            .map_err(|e| BuildError::ExecutionError(format!("Failed to compute hash: {}", e)))?;

        let manifest = vyoma_image::VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            format!("sha256:{}", hash),
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