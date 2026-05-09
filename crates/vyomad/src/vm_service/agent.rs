use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::Write;
use tracing::{info, error};

use super::types::AgentConfig;
use crate::state::AppState;

pub async fn prepare_agent(
    _state: &AppState,
    _dm_path: &str,
    vm_dir: &Path,
    _config: &vyoma_core::oci::OciImageConfig,
) -> Result<AgentConfig> {
    let initramfs_path = generate_initramfs(vm_dir)?;
    
    let temp_init_path = vm_dir.join("vyoma-init.sh");
    std::fs::write(&temp_init_path, "#!/bin/sh\nset -e\n")?;

    info!(
        "Agent prepared with initramfs at {:?} and init script at {:?}",
        initramfs_path, temp_init_path
    );

    Ok(AgentConfig {
        initramfs_path: Some(initramfs_path),
        init_script_path: temp_init_path,
        cmd: vec!["/sbin/init".to_string()],
        workdir: "/".to_string(),
        envs: vec![],
    })
}

fn generate_initramfs(vm_dir: &Path) -> Result<PathBuf> {
    let temp_dir = vm_dir.join("initramfs_temp");
    std::fs::create_dir_all(&temp_dir)?;
    
    let init_script = generate_init_script();
    let init_path = temp_dir.join("init");
    std::fs::write(&init_path, &init_script)?;
    std::fs::set_permissions(&init_path, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;

    let sbin_dir = temp_dir.join("sbin");
    std::fs::create_dir_all(&sbin_dir)?;
    
    let agent_binary = PathBuf::from("/usr/bin/vyoma-agent-vm");
    if agent_binary.exists() {
        std::fs::copy(&agent_binary, sbin_dir.join("vyoma-agent-vm"))
            .context("Failed to copy agent binary")?;
    }

    let dev_dir = temp_dir.join("dev");
    std::fs::create_dir_all(&dev_dir)?;
    
    let device_entries = [
        ("null", 'c', 1, 3),
        ("zero", 'c', 1, 5),
        ("console", 'c', 5, 1),
    ];
    
    for (name, dev_type, major, minor) in device_entries {
        let dev_path = dev_dir.join(name);
        let output = std::process::Command::new("mknod")
            .arg(&dev_path)
            .arg(dev_type.to_string())
            .arg(major.to_string())
            .arg(minor.to_string())
            .output()
            .context("Failed to run mknod")?;
        
        if !output.status.success() {
            error!("Failed to create device {}", name);
        }
    }

    let initramfs_path = vm_dir.join("initramfs.cpio");
    
    let cpio_output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cd '{}' && find . -type f -o -type d | cpio -o -H newc 2>/dev/null > '{}'",
            temp_dir.display(),
            initramfs_path.display()
        ))
        .output()
        .context("Failed to create cpio archive")?;
    
    if !cpio_output.status.success() {
        let stderr = String::from_utf8_lossy(&cpio_output.stderr);
        error!("cpio stderr: {}", stderr);
    }

    std::fs::remove_dir_all(&temp_dir).ok();

    info!("Generated initramfs: {} bytes", std::fs::metadata(&initramfs_path)?.len());
    Ok(initramfs_path)
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
}
