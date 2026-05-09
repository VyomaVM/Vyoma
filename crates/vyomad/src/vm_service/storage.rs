use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, error};
use vyoma_core::storage::StorageManager;

use super::types::PreparedStorage;
use crate::state::AppState;

pub async fn prepare_storage(
    state: &AppState,
    base_image_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    info!("Preparing storage for VM {}", vm_id);

    if state.rootless {
        prepare_rootless_storage(base_image_path, vm_dir, vm_id)
    } else {
        prepare_privileged_storage(state, base_image_path, vm_dir, vm_id).await
    }
}

fn prepare_rootless_storage(
    base_image_path: &Path,
    vm_dir: &Path,
    _vm_id: &str,
) -> Result<PreparedStorage> {
    let vm_disk = vm_dir.join("disk.ext4");
    info!("Rootless: Copying base image to {:?}", vm_disk);
    
    std::fs::copy(base_image_path, &vm_disk)
        .context("Rootless copy failed")?;

    Ok(PreparedStorage {
        dm_device_path: vm_disk.to_string_lossy().to_string(),
        loop_devices: Vec::new(),
        cow_file_path: vm_disk.to_string_lossy().to_string(),
        dm_name: "rootless".to_string(),
    })
}

async fn prepare_privileged_storage(
    state: &AppState,
    base_image_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    let cow_file = vm_dir.join("diff.cow");
    let size_mb = 2048;

    StorageManager::create_cow_file(&cow_file, size_mb)
        .context("Failed to create COW file")?;

    let base_loop = StorageManager::setup_loop_device(base_image_path)
        .context("Failed to setup base loop device")?;
    let cow_loop = StorageManager::setup_loop_device(&cow_file)
        .context("Failed to setup COW loop device")?;

    let dm_name = format!("ign-{}", vm_id);
    let size_sectors = size_mb * 1024 * 1024 / 512;
    let dm_path = StorageManager::create_dm_snapshot(&dm_name, &base_loop, &cow_loop, size_sectors)
        .context("Failed to create DM snapshot")?;

    Ok(PreparedStorage {
        dm_device_path: dm_path,
        loop_devices: vec![base_loop, cow_loop],
        cow_file_path: cow_file.to_string_lossy().to_string(),
        dm_name,
    })
}

pub async fn cleanup_storage(storage: &PreparedStorage) -> Result<()> {
    for dev in &storage.loop_devices {
        if let Err(e) = StorageManager::detach_loop_device(dev) {
            error!("Failed to detach loop {}: {}", dev, e);
        }
    }

    if let Err(e) = StorageManager::remove_dm_device(&storage.dm_name) {
        error!("Failed to remove DM {}: {}", storage.dm_name, e);
    }

    if std::path::Path::new(&storage.cow_file_path).exists() {
        let _ = std::fs::remove_file(&storage.cow_file_path);
    }

    Ok(())
}