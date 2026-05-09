use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tracing::{info, error};

pub struct VtpmManager {
    socket_path: String,
    state_dir: String,
    process: Option<Child>,
}

impl VtpmManager {
    pub fn new(vm_id: &str, base_dir: &Path) -> Result<Self> {
        let state_dir = base_dir.join(vm_id).join("tpm");
        std::fs::create_dir_all(&state_dir)?;

        let socket_path = state_dir.join("swtpm.sock").to_string_lossy().to_string();

        Ok(Self {
            socket_path,
            state_dir: state_dir.to_string_lossy().to_string(),
            process: None,
        })
    }

    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    pub fn state_dir(&self) -> &str {
        &self.state_dir
    }

    pub fn start(&mut self) -> Result<()> {
        if self.process.is_some() {
            return Ok(());
        }

        info!("Starting swtpm for vTPM at {}", self.socket_path);

        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let swtpm_binary = self.find_swtpm()?;
        let mut child = Command::new(&swtpm_binary)
            .arg("socket")
            .arg("--tpmstate")
            .arg(format!("dir={}", self.state_dir))
            .arg("--ctrl")
            .arg(format!("type=unixio,path={}", self.socket_path))
            .arg("--tpm2")
            .arg("--")
            .arg("--tpm2")
            .arg("backend")
            .arg("--type")
            .arg("dir")
            .arg("--filename")
            .arg(&self.state_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn swtpm: {}", e))?;

        self.process = Some(child);
        self.wait_for_socket(Duration::from_secs(5))?;

        info!("vTPM started successfully");
        Ok(())
    }

    fn find_swtpm(&self) -> Result<String> {
        let possible_paths = vec![
            "/usr/bin/swtpm",
            "/usr/local/bin/swtpm",
            "swtpm",
        ];

        for path in possible_paths {
            if let Ok(output) = Command::new(path).arg("--version").output() {
                if output.status.success() {
                    return Ok(path.to_string());
                }
            }
        }

        Err(anyhow!("swtpm not found. Please install swtpm package."))
    }

    fn wait_for_socket(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Path::new(&self.socket_path).exists() {
                std::thread::sleep(Duration::from_millis(100));
                if Path::new(&self.socket_path).exists() {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(anyhow!("Timed out waiting for vTPM socket"))
    }

    pub fn is_running(&self) -> bool {
        self.process.is_some()
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            info!("Stopping vTPM");
            let _ = child.kill();
            let _ = child.wait();
        }

        if Path::new(&self.socket_path).exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        Ok(())
    }

    pub fn get_tpm_info(&self) -> Result<TpmInfo> {
        if !Path::new(&self.socket_path).exists() {
            return Err(anyhow!("vTPM socket not found"));
        }

        Ok(TpmInfo {
            socket_path: self.socket_path.clone(),
            state_dir: self.state_dir.clone(),
            tpm_version: "2.0".to_string(),
        })
    }
}

impl Drop for VtpmManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug, Clone)]
pub struct TpmInfo {
    pub socket_path: String,
    pub state_dir: String,
    pub tpm_version: String,
}

pub struct PcrPolicy {
    pub pcrs: std::collections::HashMap<u32, String>,
}

impl PcrPolicy {
    pub fn new() -> Self {
        Self {
            pcrs: std::collections::HashMap::new(),
        }
    }

    pub fn with_pcr(mut self, pcr_index: u32, expected_hash: String) -> Self {
        self.pcrs.insert(pcr_index, expected_hash);
        self
    }

    pub fn standard_pcrs() -> Self {
        let mut pcrs = std::collections::HashMap::new();
        pcrs.insert(0, "firmware".to_string());
        pcrs.insert(1, "firmware_config".to_string());
        pcrs.insert(4, "boot_manager".to_string());
        pcrs.insert(5, "boot_manager_config".to_string());
        pcrs.insert(7, "secure_boot_state".to_string());
        pcrs.insert(9, "kernel".to_string());
        pcrs.insert(10, "initrd".to_string());
        pcrs.insert(14, "rootfs".to_string());
        Self { pcrs }
    }

    pub fn verify_measurement(&self, pcr_index: u32, actual_hash: &str) -> bool {
        if let Some(expected) = self.pcrs.get(&pcr_index) {
            expected == actual_hash
        } else {
            true
        }
    }
}

impl Default for PcrPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcr_policy_default() {
        let policy = PcrPolicy::new();
        assert!(policy.pcrs.is_empty());
    }

    #[test]
    fn test_pcr_policy_with_pcr() {
        let policy = PcrPolicy::new().with_pcr(9, "abc123".to_string());
        assert_eq!(policy.pcrs.get(&9), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_pcr_policy_verify() {
        let policy = PcrPolicy::new().with_pcr(9, "expected_hash".to_string());
        assert!(policy.verify_measurement(9, "expected_hash"));
        assert!(!policy.verify_measurement(9, "wrong_hash"));
    }

    #[test]
    fn test_pcr_policy_standard() {
        let policy = PcrPolicy::standard_pcrs();
        assert!(policy.pcrs.contains_key(&0));
        assert!(policy.pcrs.contains_key(&7));
        assert!(policy.pcrs.contains_key(&9));
    }
}