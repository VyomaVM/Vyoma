use anyhow::{Context, Result};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use super::types::AgentConfig;
use crate::state::AppState;

pub async fn prepare_agent(
    _state: &AppState,
    _dm_path: &str,
    vm_dir: &Path,
    _config: &vyoma_core::oci::OciImageConfig,
) -> Result<AgentConfig> {
    let initramfs_path = vm_dir.join("initramfs.cpio.gz");
    let init_script = generate_init_script();

    let agent_binary = PathBuf::from("/usr/bin/vyoma-agent-vm");
    let agent_path = if agent_binary.exists() {
        Some(&agent_binary as &Path)
    } else {
        None
    };

    vyoma_core::initramfs::create_initramfs(&init_script, agent_path, &initramfs_path)
        .context("Failed to create initramfs")?;

    info!("Agent prepared with initramfs at {:?}", initramfs_path);

    Ok(AgentConfig {
        initramfs_path: Some(initramfs_path),
        cmd: vec!["/sbin/init".to_string()],
        workdir: "/".to_string(),
        envs: vec![],
    })
}

fn generate_init_script() -> String {
    r#"#!/bin/sh
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sys /sys 2>/dev/null || true
mount -t devtmpfs dev /dev 2>/dev/null || true
ip link set lo up 2>/dev/null || true
/sbin/vyoma-agent-vm &
sleep 1
exec /sbin/init
"#.to_string()
}

pub async fn cleanup_agent(_agent_config: &AgentConfig) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_init_script() {
        let script = generate_init_script();
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("vyoma-agent-vm"));
        assert!(script.contains("mount"));
    }

    #[tokio::test]
    async fn test_prepare_agent_without_agent() {
        let temp_dir = TempDir::new().unwrap();
        let config = vyoma_core::oci::OciImageConfig::default();

        let state = Arc::new(crate::state::AppState::new_test());
        let result = prepare_agent(
            &crate::state::AppState::with_vm_service(state),
            "/dev/null",
            temp_dir.path(),
            &config,
        ).await;

        assert!(result.is_ok());
        let agent_config = result.unwrap();
        assert!(agent_config.initramfs_path.is_some());
        assert!(agent_config.initramfs_path.unwrap().exists());
    }
}