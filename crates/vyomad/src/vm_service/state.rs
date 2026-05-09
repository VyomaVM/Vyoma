use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::{info, error};

use crate::state::{AppState, VmInstance, wal::WalEntry};

pub async fn save_vm_state(
    state: &AppState,
    instance: VmInstance,
    vm_id: String,
) -> Result<()> {
    instance.save_state().context("Failed to save state")?;

    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(instance)));
    }

    if let Err(e) = state.wal.append(&WalEntry::vm_create(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }
    if let Err(e) = state.wal.append(&WalEntry::vm_start(vm_id.clone())) {
        error!("Failed to write WAL entry: {}", e);
    }

    Ok(())
}

pub async fn emit_vm_start_event(
    state: &AppState,
    vm_id: String,
    labels: std::collections::HashMap<String, String>,
) {
    let _ = state.events_tx.send(serde_json::json!({
        "type": "vm_start",
        "id": vm_id,
        "name": labels.get("vyoma.service").unwrap_or(&vm_id)
    }).to_string());
}

pub async fn load_vm_state(
    _state: &AppState,
    vm_id: &str,
) -> Result<Option<VmInstance>> {
    let home = dirs::home_dir().context("No home dir")?;
    let state_file = home.join(".ignite").join("vms").join(vm_id).join("state.json");
    
    if !state_file.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&state_file).context("Failed to read state file")?;
    let _state: crate::state::VmState = serde_json::from_str(&content)
        .context("Failed to parse state")?;
    
    info!("Loaded state for VM {}", vm_id);
    Ok(None)
}