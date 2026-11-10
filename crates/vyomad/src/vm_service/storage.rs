use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

use super::types::PreparedStorage;
use crate::state::AppState;

pub async fn prepare_storage(
    state: &AppState,
    rootfs_sqfs_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    info!("Preparing native QCOW2 storage for VM {}", vm_id);
    prepare_qcow2_storage(state, rootfs_sqfs_path, vm_dir, vm_id).await
}

pub async fn prepare_qcow2_storage(
    _state: &AppState,
    rootfs_base_path: &Path,
    vm_dir: &Path,
    vm_id: &str,
) -> Result<PreparedStorage> {
    info!("Preparing native QCOW2 backing storage for VM {}", vm_id);
    let overlay_path = vm_dir.join("root.qcow2");

    let backing_fmt = if rootfs_base_path.extension().and_then(|e| e.to_str()) == Some("qcow2") {
        "qcow2"
    } else {
        "raw"
    };

    vyoma_storage::Qcow2Manager::create_overlay_with_format(
        &overlay_path,
        rootfs_base_path,
        backing_fmt,
        None,
    )
    .context("Failed to create pure-Rust QCOW2 overlay")?;

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

pub async fn cleanup_storage(storage: &PreparedStorage) -> Result<()> {
    info!("Cleaning up storage for {}", storage.dm_name);
    if let Some(ref path) = storage.qcow2_path {
        if std::path::Path::new(path).exists() {
            info!("Removing QCOW2 overlay file: {}", path);
            let _ = std::fs::remove_file(path);
        }
    }
    if !storage.cow_file_path.is_empty() && std::path::Path::new(&storage.cow_file_path).exists() {
        info!("Removing overlay file: {}", storage.cow_file_path);
        let _ = std::fs::remove_file(&storage.cow_file_path);
    }

    Ok(())
}