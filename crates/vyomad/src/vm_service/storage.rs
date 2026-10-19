use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, error};
use vyoma_storage::{LoopManager, DmManager, LoopDevice, DmDevice, StorageManager as NativeStorageManager};

use super::types::PreparedStorage;
use crate::state::AppState;

pub struct StorageContext {
    pub loop_mgr: LoopManager,
    pub dm_mgr: DmManager,
    pub base_loop: Option<LoopDevice>,
    pub cow_loop: Option<LoopDevice>,
    pub dm_device: Option<DmDevice>,
}

impl StorageContext {
    pub fn new() -> Result<Self> {
        Ok(Self {
            loop_mgr: LoopManager::new()?,
            dm_mgr: DmManager::new()?,
            base_loop: None,
            cow_loop: None,
            dm_device: None,
        })
    }
}

pub async fn prepare_storage(
    state: &AppState,
    rootfs_sqfs_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    info!("Preparing storage for VM {} (preferring native QCOW2)", vm_id);
    match prepare_qcow2_storage(state, rootfs_sqfs_path, vm_dir, vm_id).await {
        Ok(storage) => Ok(storage),
        Err(e) => {
            tracing::warn!("QCOW2 storage creation failed ({}), falling back to Device Mapper", e);
            prepare_privileged_storage(state, rootfs_sqfs_path, vm_dir, vm_id).await
        }
    }
}

pub async fn prepare_qcow2_storage(
    _state: &AppState,
    rootfs_base_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    info!("Preparing native QCOW2 backing storage for VM {}", vm_id);
    let overlay_path = vm_dir.join("root.qcow2");
    
    let backing_path_str = rootfs_base_path.to_str().ok_or_else(|| {
        anyhow::anyhow!("Invalid base image path: {:?}", rootfs_base_path)
    })?;

    let backing_fmt = if rootfs_base_path.extension().and_then(|e| e.to_str()) == Some("qcow2") {
        "qcow2"
    } else {
        "raw"
    };

    let output = std::process::Command::new("qemu-img")
        .arg("create")
        .arg("-f")
        .arg("qcow2")
        .arg("-b")
        .arg(backing_path_str)
        .arg("-F")
        .arg(backing_fmt)
        .arg(&overlay_path)
        .arg("10G")
        .output()
        .context("Failed to run qemu-img for QCOW2 overlay creation")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("qemu-img create failed: {}", stderr);
    }

    let overlay_path_str = overlay_path.to_string_lossy().to_string();
    info!("Native QCOW2 overlay prepared at {}", overlay_path_str);

    Ok(PreparedStorage {
        dm_device_path: overlay_path_str.clone(),
        loop_devices: Vec::new(),
        cow_file_path: overlay_path_str.clone(),
        dm_name: format!("qcow2-{}", vm_id),
        is_qcow2: true,
        qcow2_path: Some(overlay_path_str),
    })
}

async fn prepare_privileged_storage(
    state: &AppState,
    rootfs_sqfs_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    let cow_file = vm_dir.join("diff.cow");
    let size_mb = 2048;

    LoopManager::create_cow_file(&cow_file, size_mb as u64)
        .context("Failed to create COW file")?;

    let loop_mgr = LoopManager::new().context("Failed to create LoopManager")?;
    let dm_mgr = DmManager::new().context("Failed to create DmManager")?;

    info!("Attaching squashfs rootfs to loop device: {:?}", rootfs_sqfs_path);
    let base_loop = loop_mgr.attach_readonly(rootfs_sqfs_path)
        .context("Failed to attach squashfs loop device")?;

    info!("Attaching COW file to loop device");
    let cow_loop = loop_mgr.attach(&cow_file)
        .context("Failed to attach COW loop device")?;

    let dm_name = format!("vyoma-{}", vm_id);
    info!("Creating Device Mapper snapshot with squashfs origin: {}", dm_name);
    
    let dm_device = dm_mgr.create_snapshot(&dm_name, base_loop.path(), cow_loop.path())
        .context("Failed to create DM snapshot")?;

    let base_loop_path = base_loop.path().to_string_lossy().to_string();
    let cow_loop_path = cow_loop.path().to_string_lossy().to_string();

    info!(
        "VMIF storage prepared: dm={}, base_loop={} (squashfs), cow_loop={}",
        dm_device.path().display(),
        base_loop_path,
        cow_loop_path
    );

    Ok(PreparedStorage {
        dm_device_path: dm_device.path().to_string_lossy().to_string(),
        loop_devices: vec![base_loop_path, cow_loop_path],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        dm_name,
        is_qcow2: false,
        qcow2_path: None,
    })
}

pub async fn cleanup_storage(storage: &PreparedStorage) -> Result<()> {
    if storage.is_qcow2 {
        info!("Cleaning up native QCOW2 storage for {}", storage.dm_name);
        if let Some(ref path) = storage.qcow2_path {
            if std::path::Path::new(path).exists() {
                info!("Removing QCOW2 overlay file: {}", path);
                let _ = std::fs::remove_file(path);
            }
        }
        return Ok(());
    }

    let loop_mgr = match LoopManager::new() {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create LoopManager for cleanup: {}", e);
            return Ok(());
        }
    };

    for dev in &storage.loop_devices {
        info!("Detaching loop device: {}", dev);
        let loop_dev = LoopDevice::new(
            std::path::PathBuf::from(dev),
            None,
        );
        if let Err(e) = loop_mgr.detach(&loop_dev) {
            error!("Failed to detach loop {}: {}", dev, e);
        }
    }

    info!("Removing DM device: {}", storage.dm_name);
    let dm_mgr = match DmManager::new() {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create DmManager for cleanup: {}", e);
            return Ok(());
        }
    };
    if let Err(e) = dm_mgr.remove_snapshot(&storage.dm_name) {
        error!("Failed to remove DM {}: {}", storage.dm_name, e);
    }

    if std::path::Path::new(&storage.cow_file_path).exists() {
        info!("Removing COW file: {}", storage.cow_file_path);
        let _ = std::fs::remove_file(&storage.cow_file_path);
    }

    Ok(())
}

pub async fn cleanup_storage_full(
    base_loop: Option<LoopDevice>,
    cow_loop: Option<LoopDevice>,
    dm_device: Option<DmDevice>,
    dm_name: &str,
) -> Result<()> {
    let loop_mgr = match LoopManager::new() {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create LoopManager: {}", e);
            return Ok(());
        }
    };

    if let Some(loop_dev) = base_loop {
        info!("Detaching base loop device: {:?}", loop_dev.path());
        if let Err(e) = loop_mgr.detach(&loop_dev) {
            error!("Failed to detach base loop: {}", e);
        }
    }

    if let Some(loop_dev) = cow_loop {
        info!("Detaching COW loop device: {:?}", loop_dev.path());
        if let Err(e) = loop_mgr.detach(&loop_dev) {
            error!("Failed to detach COW loop: {}", e);
        }
    }

    if let Some(dm) = dm_device {
        let dm_mgr = match DmManager::new() {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to create DmManager: {}", e);
                return Ok(());
            }
        };
        info!("Removing DM device: {}", dm.name());
        if let Err(e) = dm_mgr.remove_snapshot(dm.name()) {
            error!("Failed to remove DM device: {}", e);
        }
    }

    Ok(())
}