use anyhow::{anyhow, Result};
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::process::{Command, Child, Stdio};
use std::time::Duration;
use std::fmt;
use std::thread;
use std::io::{BufRead, BufReader};
use tracing::info;
use tokio::sync::broadcast;

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
    log_sender: broadcast::Sender<String>,
}

impl VmmManager {
    pub fn new(socket_path: &str) -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            socket_path: socket_path.to_string(),
            process: None,
            log_sender: tx,
        }
    }
    
    pub fn get_pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.id())
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        if let Some(child) = self.process.as_mut() {
            child.try_wait().map_err(|e| anyhow!("Failed to wait on child: {}", e))
        } else {
            Ok(None)
        }
    }

    /// Spawns the Firecracker process in a background thread/process.
    /// Optionally runs inside a network namespace.
    pub fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()> {
        info!("Starting Firecracker at {} (Socket: {}, NetNS: {:?}, Rootless: {})", binary_path, self.socket_path, netns, rootless);
        
        // Ensure socket doesn't exist
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let mut child = if rootless {
             // Rootless: Run in new User(+Root map) and Net namespace
             Command::new("unshare")
                .arg("-r") // map current user to root inside NS
                .arg("-n") // new network namespace
                .arg(binary_path)
                .arg("--api-sock")
                .arg(&self.socket_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| anyhow!("Failed to spawn firecracker with unshare: {}", e))?
        } else if let Some(ns) = netns {
             Command::new("sudo") // sudo is needed for ip netns exec usually
                .arg("ip")
                .arg("netns")
                .arg("exec")
                .arg(ns)
                .arg(binary_path)
                .arg("--api-sock")
                .arg(&self.socket_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| anyhow!("Failed to spawn firecracker in netns: {}", e))?
        } else {
             Command::new(binary_path)
                .arg("--api-sock")
                .arg(&self.socket_path)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| anyhow!("Failed to spawn firecracker: {}", e))?
        };

        // Capture logs
        let stdout = child.stdout.take().ok_or(anyhow!("Failed to capture stdout"))?;
        let stderr = child.stderr.take().ok_or(anyhow!("Failed to capture stderr"))?;
        let tx_out = self.log_sender.clone();
        let tx_err = self.log_sender.clone();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx_out.send(format!("[STDOUT] {}", l));
                }
            }
        });

        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                 if let Ok(l) = line {
                    let _ = tx_err.send(format!("[STDERR] {}", l));
                }
            }
        });

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

    /// Checks if the Firecracker process is responsive via the API socket.
    pub async fn check_alive(&self) -> bool {
        // Just try to get machine config or version info.
        // GET /machine-config should work if configured, or just GET / (FC returns info usually).
        // Let's try GET /
        // curl --unix-socket ... http://localhost/
        // Actually FC might return 404 for /, but that means it IS alive.
        // We just check if curl connects.
        
        let mut cmd = Command::new("curl");
        cmd.arg("--unix-socket").arg(&self.socket_path)
           .arg("--head") // Just HEAD request
           .arg("--silent")
           .arg("--fail") // Fail on server errors? well 404 is fine.
           .arg("http://localhost/");
           
        // If curl returns 0, it connected and got 200-299.
        // If it returns exit code 7 (Failed to connect), it's dead.
        // If it returns 22 (HTTP Error) but connected, it's alive.
        
        match cmd.status() {
            Ok(status) => {
                // If it's exit code 7 (CURLE_COULDNT_CONNECT), then it is dead.
                // Otherwise (even if 404/400), the socket is there.
                if let Some(code) = status.code() {
                     return code != 7;
                }
                false // No exit code?
            },
            Err(_) => false,
        }
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

    /// Adds a VirtioFS file system.
    pub async fn add_file_system(&self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()> {
        #[derive(Serialize)]
        struct FileSystemConfig {
            device_id: String,
            socket_path: String,
            tag: String,
        }
        
        let config = FileSystemConfig {
            device_id: fs_id.to_string(),
            socket_path: socket_path.to_string(),
            tag: tag.to_string(),
        };
        // Note: Endpoint might be /filesystems/ID or just /filesystems?
        // Firecracker docs usually say PUT /filesystems/<id>
        // But let's try /filesystems/id
        let endpoint = format!("/filesystems/{}", fs_id);
        self.api_request(&endpoint, "PUT", Some(&config)).await
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
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }
}

impl Drop for VmmManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
