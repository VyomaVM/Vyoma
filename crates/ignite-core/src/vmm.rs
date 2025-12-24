use anyhow::{anyhow, Result};
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::process::{Command, Child};
use std::time::Duration;
use std::fmt;
use tracing::info;

impl fmt::Debug for VmmManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VmmManager")
         .field("socket_path", &self.socket_path)
         .field("process", &if self.process.is_some() { "Some(Child)" } else { "None" })
         .finish()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BootSource {
    pub kernel_image_path: String,
    pub boot_args: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Drive {
    pub drive_id: String,
    pub path_on_host: String,
    pub is_root_device: bool,
    pub is_read_only: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MachineConfig {
    pub vcpu_count: u32,
    pub mem_size_mib: u32,
    // Add other fields like smt, track_dirty_pages if needed
}

/// Manages a Firecracker process and its API interaction.
pub struct VmmManager {
    socket_path: String,
    process: Option<Child>,
}

impl VmmManager {
    pub fn new(socket_path: &str) -> Self {
        Self {
            socket_path: socket_path.to_string(),
            process: None,
        }
    }

    /// Spawns the Firecracker process in a background thread/process.
    pub fn start_daemon(&mut self, binary_path: &str) -> Result<()> {
        info!("Starting Firecracker daemon at {} using socket {}", binary_path, self.socket_path);
        
        // Ensure socket doesn't exist
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let child = Command::new(binary_path)
            .arg("--api-sock")
            .arg(&self.socket_path)
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn firecracker: {}", e))?;

        self.process = Some(child);
        
        // Wait for socket to appear
        self.wait_for_socket(Duration::from_secs(2))?;
        
        Ok(())
    }

    fn wait_for_socket(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if Path::new(&self.socket_path).exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(anyhow!("Timed out waiting for Firecracker socket"))
    }

    /// Sends a configuration request to the Firecracker API via curl.
    async fn api_request<T: Serialize>(&self, endpoint: &str, method: &str, body: Option<&T>) -> Result<()> {
        let url = format!("http://localhost{}", endpoint);
        
        let mut cmd = Command::new("curl");
        cmd.arg("--unix-socket").arg(&self.socket_path)
           .arg("-X").arg(method)
           .arg("--silent")
           .arg("--show-error")
           .arg("--fail"); // Fail on HTTP errors

        if let Some(b) = body {
            let json = serde_json::to_string(b)?;
            cmd.arg("-H").arg("Content-Type: application/json");
            cmd.arg("-H").arg("Accept: application/json");
            cmd.arg("-d").arg(json);
        }
        
        cmd.arg(&url);

        let output = cmd.output().map_err(|e| anyhow!("Failed to execute curl: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("API request {} failed: {}", endpoint, stderr));
        }

        Ok(())
    }

    /// Configures the boot source (kernel).
    pub async fn set_boot_source(&self, kernel_path: &str, boot_args: &str) -> Result<()> {
        let config = BootSource {
            kernel_image_path: kernel_path.to_string(),
            boot_args: boot_args.to_string(),
        };
        self.api_request("/boot-source", "PUT", Some(&config)).await
    }

    /// Adds a drive.
    pub async fn add_drive(&self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()> {
        let drive = Drive {
            drive_id: drive_id.to_string(),
            path_on_host: host_path.to_string(),
            is_root_device: is_root,
            is_read_only: false,
        };
        // URL for drives is /drives/<drive_id>
        let endpoint = format!("/drives/{}", drive_id);
        self.api_request(&endpoint, "PUT", Some(&drive)).await
    }
    
    /// Sets machine configuration (vCPUs, RAM).
    pub async fn set_machine_config(&self, vcpu_count: u32, mem_size_mib: u32) -> Result<()> {
        let config = MachineConfig {
            vcpu_count,
            mem_size_mib,
        };
        self.api_request("/machine-config", "PUT", Some(&config)).await
    }

    /// Starts the specific Instance.
    pub async fn start_instance(&self) -> Result<()> {
        #[derive(Serialize)]
        struct Action {
            action_type: String,
        }
        let action = Action { action_type: "InstanceStart".to_string() };
        self.api_request("/actions", "PUT", Some(&action)).await
    }

    /// Adds a network interface.
    pub async fn add_network_interface(&self, iface_id: &str, host_dev_name: &str, guest_mac: Option<&str>) -> Result<()> {
        #[derive(Serialize)]
        struct NetworkInterface {
            iface_id: String,
            host_dev_name: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            guest_mac: Option<String>,
        }
        
        let net = NetworkInterface {
            iface_id: iface_id.to_string(),
            host_dev_name: host_dev_name.to_string(),
            guest_mac: guest_mac.map(|s| s.to_string()),
        };
        let endpoint = format!("/network-interfaces/{}", iface_id);
        self.api_request(&endpoint, "PUT", Some(&net)).await
    }

    /// Pauses the VM.
    pub async fn pause_instance(&self) -> Result<()> {
        #[derive(Serialize)]
        struct StateChange {
            state: String,
        }
        let change = StateChange { state: "Paused".to_string() };
        self.api_request("/vm", "PATCH", Some(&change)).await
    }

    /// Resumes the VM.
    pub async fn resume_instance(&self) -> Result<()> {
        #[derive(Serialize)]
        struct StateChange {
            state: String,
        }
        let change = StateChange { state: "Resumed".to_string() };
        self.api_request("/vm", "PATCH", Some(&change)).await
    }

    /// Creates a snapshot of the current VM state.
    /// The VM must be paused first.
    pub async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        #[derive(Serialize)]
        struct SnapshotConfig {
            snapshot_path: String,
            mem_file_path: String,
            snapshot_type: String, // Full or Diff
        }
        
        let config = SnapshotConfig {
            snapshot_path: snapshot_path.to_string(),
            mem_file_path: mem_file_path.to_string(),
            snapshot_type: "Full".to_string(),
        };
        
        self.api_request("/snapshot/create", "PUT", Some(&config)).await
    }

    /// Loads a snapshot from disk.
    /// This should be called before starting the instance (and instead of booting a kernel).
    pub async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        #[derive(Serialize)]
        struct LoadConfig {
            snapshot_path: String,
            mem_file_path: String,
            // enable_diff_snapshots: bool, // Optional
            // resume_vm: bool, // Optional
        }
        
        let config = LoadConfig {
            snapshot_path: snapshot_path.to_string(),
            mem_file_path: mem_file_path.to_string(),
        };
        
        self.api_request("/snapshot/load", "PUT", Some(&config)).await
    }

    pub fn kill(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            info!("Killing Firecracker process");
            child.kill()?;
            child.wait()?;
        }
        // Cleanup socket
        if Path::new(&self.socket_path).exists() {
           let _ = std::fs::remove_file(&self.socket_path);
        }
        Ok(())
    }
}

impl Drop for VmmManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
