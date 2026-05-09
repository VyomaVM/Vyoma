use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn, error};

use super::types::AgentConfig;

pub async fn prepare_agent(
    dm_path: &str,
    vm_dir: &Path,
    config: &vyoma_core::oci::OciImageConfig,
    state: &crate::state::AppState,
) -> Result<AgentConfig> {
    let mut init_script = String::new();
    init_script.push_str("#!/bin/sh\n");
    init_script.push_str("set -e\n");
    init_script.push_str("mount -t proc proc /proc || true\n");
    init_script.push_str("mount -t sysfs sys /sys || true\n");
    init_script.push_str("mount -t devtmpfs dev /dev || true\n");
    init_script.push_str("/sbin/vyoma-agent &\n\n");

    let mut envs = Vec::new();
    if let Some(e) = &config.env {
        envs = e.clone();
    }
    for e in &envs {
        init_script.push_str(&format!("export \"{}\"\n", e.replace('"', "\\\"")));
    }

    let mut workdir = "/".to_string();
    if let Some(wd) = &config.working_dir {
        if !wd.is_empty() {
            workdir = wd.clone();
        }
    }
    init_script.push_str(&format!("mkdir -p {}\n", workdir));
    init_script.push_str(&format!("cd {}\n", workdir));

    let mut oci_cmd = vec!["/bin/sh".to_string()];
    if let Some(cmd) = &config.cmd {
        oci_cmd = cmd.clone();
    }
    let cmd_str = oci_cmd.into_iter()
        .map(|s| format!("\"{}\"", s.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ");
    init_script.push_str(&format!("exec {}\n", cmd_str));

    let temp_init_path = vm_dir.join("vyoma-init.sh");
    std::fs::write(&temp_init_path, &init_script)
        .context("Failed to write init script")?;

    if state.rootless {
        return Ok(AgentConfig {
            initramfs_path: None,
            init_script_path: temp_init_path,
            cmd: oci_cmd,
            workdir,
            envs,
        });
    }

    let agent_path = std::env::current_exe()
        .map(|p| p.parent().unwrap().join("vyoma-agent"))
        .unwrap_or_else(|_| PathBuf::from("/usr/bin/vyoma-agent"));

    let agent_path = if !agent_path.exists() {
        PathBuf::from("target/x86_64-unknown-linux-musl/release/vyoma-agent")
    } else {
        agent_path
    };

    let mount_point = vm_dir.join("mnt");
    std::fs::create_dir_all(&mount_point).context("Failed to create mount point")?;

    let mount_status = Command::new("ip")
        .args(&["mount", dm_path, &mount_point.to_string_lossy()])
        .status();

    if mount_status.map(|s| s.success()).unwrap_or(false) {
        let sbin_dir = mount_point.join("sbin");
        std::fs::create_dir_all(&sbin_dir).unwrap_or_default();

        let target_init = sbin_dir.join("vyoma-init");
        let _ = Command::new("cp")
            .arg(&temp_init_path)
            .arg(&target_init)
            .status();
        let _ = Command::new("chmod")
            .args(&["+x", &target_init.to_string_lossy()])
            .status();

        let target_agent = sbin_dir.join("vyoma-agent");
        let _ = Command::new("cp")
            .arg(&agent_path)
            .arg(&target_agent)
            .status();
        let _ = Command::new("chmod")
            .args(&["+x", &target_agent.to_string_lossy()])
            .status();

        let _ = Command::new("umount")
            .arg(&mount_point.to_string_lossy())
            .status();
        let _ = std::fs::remove_dir(&mount_point);

        info!("Injected /sbin/vyoma-init and /sbin/vyoma-agent via mount");
    } else {
        warn!("Failed to mount {} to inject init script", dm_path);
    }

    Ok(AgentConfig {
        initramfs_path: None,
        init_script_path: temp_init_path,
        cmd: oci_cmd,
        workdir,
        envs,
    })
}