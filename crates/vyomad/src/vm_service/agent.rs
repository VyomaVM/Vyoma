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
    let init_script = generate_init_script(_config);

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

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/' || c == ':') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn generate_init_script(config: &vyoma_core::oci::OciImageConfig) -> String {
    let mut script = String::new();

    script.push_str("#!/bin/sh\n");
    script.push_str("mount -t proc proc /proc 2>/dev/null || true\n");
    script.push_str("mount -t sysfs sys /sys 2>/dev/null || true\n");
    script.push_str("mount -t devtmpfs dev /dev 2>/dev/null || true\n");
    script.push_str("ip link set lo up 2>/dev/null || true\n");

    if let Some(envs) = &config.env {
        for env in envs {
            if let Some((key, value)) = env.split_once('=') {
                script.push_str(&format!("export {}={}\n", shell_escape(key), shell_escape(value)));
            }
        }
    }

    if let Some(workdir) = &config.working_dir {
        script.push_str(&format!("cd {}\n", shell_escape(workdir)));
    }

    let full_cmd = config.full_command();

    // Start the agent in the background before executing the workload.
    // The agent is forked (&) so it continues running after exec replaces
    // this shell. The orphaned agent process gets reparented to the VM's init.
    script.push_str("/sbin/vyoma-agent-vm &\n");

    if !full_cmd.is_empty() {
        let cmd_args: Vec<String> = full_cmd.iter().map(|s| shell_escape(s)).collect();
        script.push_str(&format!("exec {}\n", cmd_args.join(" ")));
    } else {
        script.push_str("exec /bin/sh\n");
    }

    script
}

pub async fn cleanup_agent(_agent_config: &AgentConfig) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_init_script_default() {
        let config = vyoma_core::oci::OciImageConfig::default();
        let script = generate_init_script(&config);
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("vyoma-agent-vm"));
        assert!(script.contains("mount"));
    }

    #[test]
    fn test_generate_init_script_with_config() {
        let mut config = vyoma_core::oci::OciImageConfig::default();
        config.entrypoint = Some(vec!["/bin/nginx".to_string()]);
        config.cmd = Some(vec!["-g".to_string(), "daemon off;".to_string()]);
        config.env = Some(vec!["NGINX_HOST=localhost".to_string(), "NGINX_PORT=80".to_string()]);
        config.working_dir = Some("/usr/share/nginx/html".to_string());

        let script = generate_init_script(&config);
        assert!(script.contains("export NGINX_HOST=localhost"));
        assert!(script.contains("export NGINX_PORT=80"));
        assert!(script.contains("cd /usr/share/nginx/html"));
        assert!(script.contains("exec /bin/nginx -g 'daemon off;'"));
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("simple"), "simple");
        assert_eq!(shell_escape("with-dash"), "with-dash");
        assert_eq!(shell_escape("with_underscore"), "with_underscore");
        assert_eq!(shell_escape("with'quote"), "'with'\\''quote'");
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