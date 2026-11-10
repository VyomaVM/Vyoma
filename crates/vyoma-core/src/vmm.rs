use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
#[cfg(target_os = "linux")]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::{Command, Child, Stdio};
use std::process::ExitStatus;
#[cfg(target_os = "linux")]
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::fmt;
#[cfg(target_os = "linux")]
use std::thread;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};
#[cfg(target_os = "linux")]
use tracing::{info, error};
use tokio::sync::broadcast;
#[cfg(target_os = "linux")]
use reqwest::{Client, Method};

#[cfg(target_os = "linux")]
use crate::ch_types::{
    VmConfig, CpusConfig, MemoryConfig, PayloadConfig, DiskConfig, NetConfig, FsConfig,
    VmSnapshotConfig, VmRestoreConfig, SendMigrationData, ReceiveMigrationData,
    VmResizeDiskConfig,
};

#[cfg(target_os = "linux")]
impl fmt::Debug for VmmManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VmmManager")
         .field("socket_path", &self.socket_path)
         .field("event_monitor_path", &self.event_monitor_path)
         .field("process", &if self.process.is_some() { "Some(Child)" } else { "None" })
         .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmmTimestamp {
    pub secs: u64,
    pub nanos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VmmEvent {
    #[serde(default)]
    pub timestamp: Option<VmmTimestamp>,
    pub source: String,
    pub event: String,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

/// Hypervisor selection strategy for `vyomad` micro-VMs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HypervisorMode {
    /// Prefer Cloud Hypervisor KVM when `/dev/kvm` exists; fall back to QEMU TCG when unavailable.
    #[default]
    Auto,
    /// Require Cloud Hypervisor with `/dev/kvm`.
    CloudHypervisor,
    /// Force QEMU software TCG emulation (useful on Apple Silicon M1/M2 or non-KVM hosts).
    QemuTcg,
}

impl HypervisorMode {
    pub fn from_str_lossy(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "cloud-hypervisor" | "ch" | "kvm" => Ok(Self::CloudHypervisor),
            "qemu" | "qemu-tcg" | "tcg" => Ok(Self::QemuTcg),
            other => Err(anyhow::anyhow!(
                "Invalid hypervisor mode '{}': expected auto, cloud-hypervisor, or qemu-tcg",
                other
            )),
        }
    }
}

/// Platform-independent Virtual Machine Monitor backend abstraction.
/// Implemented natively on Linux via [`VmmManager`] (Cloud Hypervisor)
/// and [`crate::qemu::QemuTcgManager`] (software TCG fallback).
#[async_trait]
pub trait VmmBackend: Send + Sync {
    fn socket_path(&self) -> &str;
    fn event_monitor_path(&self) -> Option<&str>;
    fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent>;
    fn subscribe_logs(&self) -> broadcast::Receiver<String>;
    fn pid(&self) -> Option<u32>;
    fn try_wait(&mut self) -> Result<Option<ExitStatus>>;
    fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()>;
    async fn check_alive(&self) -> bool;
    async fn set_boot_source(&mut self, kernel_path: &str, boot_args: &str, initramfs: Option<&str>) -> Result<()>;
    async fn add_drive(&mut self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()>;
    async fn set_machine_config(&mut self, vcpu_count: u32, mem_size_mib: u32) -> Result<()>;
    async fn add_network_interface(&mut self, iface_id: &str, host_dev_name: &str, guest_mac: Option<&str>) -> Result<()>;
    async fn add_file_system(&mut self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()>;
    async fn add_vsock(&mut self, cid: u32, socket_path: &str) -> Result<()>;
    async fn enable_serial(&mut self, file_path: &str) -> Result<()>;
    async fn set_tpm(&mut self, socket_path: &str) -> Result<()>;
    async fn start_instance(&self) -> Result<()>;
    async fn pause_instance(&self) -> Result<()>;
    async fn resume_instance(&self) -> Result<()>;
    async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()>;
    async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()>;
    async fn send_migration(&self, target_url: &str, bandwidth_mbps: Option<u32>) -> Result<()>;
    async fn receive_migration(&self, receiver_url: &str) -> Result<()>;
    async fn cancel_migration(&self) -> Result<()>;
    async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()>;
    fn kill(&mut self) -> Result<()>;
}

/// Manages a Cloud Hypervisor process and its API interaction.
#[cfg(target_os = "linux")]
pub struct VmmManager {
    socket_path: String,
    event_monitor_path: Option<String>,
    event_sender: broadcast::Sender<VmmEvent>,
    process: Option<Child>,
    log_sender: broadcast::Sender<String>,
    config: VmConfig,
    client: Client,
}

#[cfg(target_os = "linux")]
impl VmmManager {
    pub fn new(socket_path: &str) -> Self {
        let (tx, _) = broadcast::channel(100);
        let (event_tx, _) = broadcast::channel(100);
        
        let client = Client::builder()
            .unix_socket(socket_path)
            .build()
            .unwrap_or_else(|_| Client::new()); // Fallback, but unix_socket should work
            
        Self {
            socket_path: socket_path.to_string(),
            event_monitor_path: Some(format!("{}.events", socket_path)),
            event_sender: event_tx,
            process: None,
            log_sender: tx,
            config: VmConfig::default(),
            client,
        }
    }
    
    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    pub fn event_monitor_path(&self) -> Option<&str> {
        self.event_monitor_path.as_deref()
    }

    pub fn set_event_monitor_path(&mut self, path: Option<String>) {
        self.event_monitor_path = path;
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent> {
        self.event_sender.subscribe()
    }

    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.id())
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        if let Some(child) = self.process.as_mut() {
            child.try_wait().map_err(|e| anyhow!("Failed to wait on child: {}", e))
        } else {
            Ok(None)
        }
    }

    pub fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()> {
        info!("Starting Cloud Hypervisor at {} (Socket: {}, EventMonitor: {:?}, NetNS: {:?}, Rootless: {})", 
            binary_path, self.socket_path, self.event_monitor_path, netns, rootless);
        
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        if let Some(ref ep) = self.event_monitor_path {
            if Path::new(ep).exists() {
                let _ = std::fs::remove_file(ep);
            }
        }

        let mut cmd = if rootless {
            let mut c = Command::new("unshare");
            c.arg("-r").arg("-n").arg(binary_path);
            c
        } else if let Some(ns) = netns {
            let mut c = Command::new("ip");
            c.arg("netns").arg("exec").arg(ns).arg(binary_path);
            c
        } else {
            Command::new(binary_path)
        };

        cmd.arg("--api-socket").arg(&self.socket_path);

        if let Some(ref ep) = self.event_monitor_path {
            cmd.arg("--event-monitor").arg(format!("path={}", ep));
        }

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn cloud-hypervisor: {}", e))?;

        let stdout = child.stdout.take().ok_or(anyhow!("Failed to capture stdout"))?;
        let stderr = child.stderr.take().ok_or(anyhow!("Failed to capture stderr"))?;
        let tx_out = self.log_sender.clone();
        let tx_err = self.log_sender.clone();

        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    tracing::info!("[CH STDOUT] {}", l);
                    let _ = tx_out.send(format!("[STDOUT] {}", l));
                }
            }
        });

        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    tracing::error!("[CH STDERR] {}", l);
                    let _ = tx_err.send(format!("[STDERR] {}", l));
                }
            }
        });

        if let Some(event_path) = self.event_monitor_path.clone() {
            let tx_event = self.event_sender.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    stream_vmm_events(&event_path, tx_event).await;
                });
            } else {
                thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                        rt.block_on(stream_vmm_events(&event_path, tx_event));
                    }
                });
            }
        }

        self.process = Some(child);
        self.wait_for_socket(Duration::from_secs(5))?;
        
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
        Err(anyhow!("Timed out waiting for Cloud Hypervisor socket"))
    }

    pub async fn check_alive(&self) -> bool {
        self.api_request::<()>("/api/v1/vmm.ping", Method::GET, None).await.is_ok()
    }

    async fn api_request<T: Serialize>(&self, endpoint: &str, method: Method, body: Option<&T>) -> Result<()> {
        let url = format!("http://localhost{}", endpoint);
        info!("VMM api_request: socket_path={}, endpoint={}, method={}", self.socket_path, endpoint, method);
        
        if !Path::new(&self.socket_path).exists() {
            return Err(anyhow!("Cloud Hypervisor socket not found at {} - it may have crashed", self.socket_path));
        }

        let mut req = self.client.request(method.clone(), &url);
        
        if let Some(b) = body {
            req = req.json(b);
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            error!("API {} failed. status: {}, body: {}", endpoint, status, err_text);
            return Err(anyhow!("API request {} failed with {}: {}", endpoint, status, err_text));
        }

        Ok(())
    }

    pub async fn set_boot_source(&mut self, kernel_path: &str, boot_args: &str, initramfs: Option<&str>) -> Result<()> {
        self.config.payload = Some(PayloadConfig {
            kernel: Some(kernel_path.to_string()),
            cmdline: Some(boot_args.to_string()),
            initramfs: initramfs.map(|s| s.to_string()),
        });
        Ok(())
    }

    pub async fn add_drive(&mut self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()> {
        let is_qcow2 = host_path.ends_with(".qcow2");
        let disk = DiskConfig {
            path: host_path.to_string(),
            readonly: if is_qcow2 {
                Some(false) // QCOW2 overlays must be writable for guest state mutations
            } else if is_root {
                Some(true) // Traditional Device Mapper / loop base rootfs is readonly
            } else {
                None
            },
            direct: None,
            vhost_user: None,
            id: Some(drive_id.to_string()),
            queue_size: None,
            num_queues: None,
            image_type: if is_qcow2 {
                Some("Qcow2".to_string())
            } else {
                Some("Raw".to_string())
            },
            backing_files: if is_qcow2 {
                Some(true)
            } else {
                None
            },
        };
        
        if let Some(disks) = &mut self.config.disks {
            disks.push(disk);
        } else {
            self.config.disks = Some(vec![disk]);
        }
        Ok(())
    }
    
    pub async fn set_machine_config(&mut self, vcpu_count: u32, mem_size_mib: u32) -> Result<()> {
        self.config.cpus = Some(CpusConfig {
            boot_vcpus: vcpu_count,
            max_vcpus: Some(vcpu_count),
        });
        
        self.config.memory = Some(MemoryConfig {
            size: (mem_size_mib as u64) * 1024 * 1024,
            shared: Some(true), // often required for virtiofs or vhost-user
            hugepages: None,
        });
        Ok(())
    }

    pub async fn add_network_interface(&mut self, iface_id: &str, host_dev_name: &str, guest_mac: Option<&str>) -> Result<()> {
        let net = NetConfig {
            tap: Some(host_dev_name.to_string()),
            mac: guest_mac.map(|s| s.to_string()),
            ip: None,
            mask: None,
            vhost_user: None,
        };
        
        if let Some(nets) = &mut self.config.net {
            nets.push(net);
        } else {
            self.config.net = Some(vec![net]);
        }
        Ok(())
    }

    pub async fn add_file_system(&mut self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()> {
        let fs = FsConfig {
            tag: tag.to_string(),
            socket: socket_path.to_string(),
            num_queues: Some(1),
            queue_size: Some(1024),
        };
        
        if let Some(fss) = &mut self.config.fs {
            fss.push(fs);
        } else {
            self.config.fs = Some(vec![fs]);
        }
        Ok(())
    }

    pub async fn add_vsock(&mut self, cid: u32, socket_path: &str) -> Result<()> {
        let vsock = crate::ch_types::VsockConfig {
            cid,
            socket: socket_path.to_string(),
        };
        self.config.vsock = Some(vsock);
        Ok(())
    }

    pub async fn enable_serial(&mut self, file_path: &str) -> Result<()> {
        let serial = crate::ch_types::ConsoleConfig {
            file: Some(file_path.to_string()),
            mode: "File".to_string(),
        };
        self.config.serial = Some(serial);
        Ok(())
    }

    pub async fn set_firmware(&mut self, firmware_path: &str, secure_boot: bool, uefi_vars: Option<&str>) -> Result<()> {
        let firmware = crate::ch_types::FirmwareConfig {
            firmware_path: firmware_path.to_string(),
            secure_boot,
            uefi_vars: uefi_vars.map(|s| s.to_string()),
        };
        self.config.firmware = Some(firmware);
        info!("Configured firmware: {} (secure_boot: {})", firmware_path, secure_boot);
        Ok(())
    }

    pub async fn set_tpm(&mut self, socket_path: &str) -> Result<()> {
        let tpm = crate::ch_types::TpmConfig {
            socket_path: socket_path.to_string(),
            tpm_version: "2.0".to_string(),
        };
        self.config.tpm = Some(tpm);
        info!("Configured TPM at {}", socket_path);
        Ok(())
    }

    pub async fn start_instance(&self) -> Result<()> {
        // Step 1: Create VM with config
        info!("Sending VmConfig to Cloud Hypervisor: {:?}", self.config);
        self.api_request("/api/v1/vm.create", Method::PUT, Some(&self.config)).await?;
        
        // Step 2: Boot VM
        self.api_request::<()>("/api/v1/vm.boot", Method::PUT, None).await?;
        Ok(())
    }

    pub async fn pause_instance(&self) -> Result<()> {
        self.api_request::<()>("/api/v1/vm.pause", Method::PUT, None).await
    }

    pub async fn resume_instance(&self) -> Result<()> {
        self.api_request::<()>("/api/v1/vm.resume", Method::PUT, None).await
    }

    pub async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        let config = VmSnapshotConfig {
            destination_url: format!("file://{}", snapshot_path),
        };
        self.api_request("/api/v1/vm.snapshot", Method::PUT, Some(&config)).await
    }

    pub async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        let config = VmRestoreConfig {
            source_url: format!("file://{}", snapshot_path),
        };
        self.api_request("/api/v1/vm.restore", Method::PUT, Some(&config)).await
    }

    pub async fn send_migration(&self, target_url: &str, bandwidth_mbps: Option<u32>) -> Result<()> {
        let config = SendMigrationData {
            destination_url: target_url.to_string(),
            local: None,
            bandwidth: bandwidth_mbps,
            ..Default::default()
        };
        self.api_request("/api/v1/vm.send-migration", Method::PUT, Some(&config)).await
    }

    pub async fn receive_migration(&self, receiver_url: &str) -> Result<()> {
        let config = ReceiveMigrationData {
            receiver_url: receiver_url.to_string(),
            tls_dir: None,
        };
        self.api_request("/api/v1/vm.receive-migration", Method::PUT, Some(&config)).await
    }

    pub async fn cancel_migration(&self) -> Result<()> {
        info!("Cancelling migration for VM on socket {}", self.socket_path);
        self.api_request::<()>("/api/v1/vm.cancel-migration", Method::PUT, None).await
    }

    pub async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()> {
        info!("Resizing disk {} to {} bytes via Cloud Hypervisor API", disk_id, desired_size);
        let config = VmResizeDiskConfig {
            id: disk_id.to_string(),
            desired_size,
        };
        self.api_request::<VmResizeDiskConfig>("/api/v1/vm.resize-disk", Method::PUT, Some(&config)).await
    }

    pub async fn enable_sev_snp(&mut self, policy: Option<&str>, guest_key_root: Option<&str>) -> Result<()> {
        let sev_snp = crate::ch_types::SevSnpConfig {
            enabled: true,
            policy: policy.map(|s| s.to_string()),
            certificate_path: None,
            guest_key_root_hash: guest_key_root.map(|s| s.to_string()),
            host_data: None,
        };
        self.config.sev_snp = Some(sev_snp);
        info!("Enabled SEV-SNP with policy: {:?}", policy);
        Ok(())
    }

    pub async fn enable_tdx(&mut self, measurement_uuid: Option<&str>) -> Result<()> {
        let tdx = crate::ch_types::TdxConfig {
            enabled: true,
            measurement_uuid: measurement_uuid.map(|s| s.to_string()),
        };
        self.config.tdx = Some(tdx);
        info!("Enabled TDX with UUID: {:?}", measurement_uuid);
        Ok(())
    }

    pub fn is_sev_snp_enabled(&self) -> bool {
        self.config.sev_snp.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    pub fn is_tdx_enabled(&self) -> bool {
        self.config.tdx.as_ref().map(|c| c.enabled).unwrap_or(false)
    }

    pub fn kill(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            info!("Killing Cloud Hypervisor process");
            let _ = child.kill();
            let _ = child.wait();
        }
        if Path::new(&self.socket_path).exists() {
           let _ = std::fs::remove_file(&self.socket_path);
        }
        Ok(())
    }
    
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }
}

#[derive(Default)]
struct JsonStreamParser {
    buffer: String,
    brace_depth: i32,
    in_string: bool,
    escape: bool,
}

impl JsonStreamParser {
    fn feed(&mut self, chunk: &str) -> Vec<VmmEvent> {
        let mut events = Vec::new();
        for c in chunk.chars() {
            if self.brace_depth == 0 && c.is_whitespace() {
                continue;
            }
            self.buffer.push(c);
            if self.escape {
                self.escape = false;
            } else if c == '\\' && self.in_string {
                self.escape = true;
            } else if c == '"' {
                self.in_string = !self.in_string;
            } else if !self.in_string {
                if c == '{' {
                    self.brace_depth += 1;
                } else if c == '}' {
                    self.brace_depth -= 1;
                    if self.brace_depth == 0 {
                        let trimmed = self.buffer.trim();
                        if !trimmed.is_empty() {
                            if let Ok(evt) = serde_json::from_str::<VmmEvent>(trimmed) {
                                events.push(evt);
                            }
                        }
                        self.buffer.clear();
                    } else if self.brace_depth < 0 {
                        self.brace_depth = 0;
                        self.buffer.clear();
                    }
                }
            }
        }
        events
    }
}

#[cfg(target_os = "linux")]
async fn stream_vmm_events(event_path: &str, tx_event: broadcast::Sender<VmmEvent>) {
    use tokio::io::AsyncReadExt;

    let path = Path::new(event_path);
    for _ in 0..60 {
        if path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let mut file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return,
    };

    let mut parser = JsonStreamParser::default();
    let mut buf = [0u8; 4096];

    loop {
        match file.read(&mut buf).await {
            Ok(0) => {
                if tx_event.receiver_count() == 0 && !path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buf[..n]);
                for evt in parser.feed(&chunk) {
                    tracing::info!("[CH EVENT] {}: {}", evt.source, evt.event);
                    let _ = tx_event.send(evt);
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl VmmBackend for VmmManager {
    fn socket_path(&self) -> &str {
        self.socket_path()
    }

    fn event_monitor_path(&self) -> Option<&str> {
        self.event_monitor_path()
    }

    fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent> {
        self.subscribe_events()
    }

    fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.subscribe_logs()
    }

    fn pid(&self) -> Option<u32> {
        self.pid()
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.try_wait()
    }

    fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()> {
        self.start_daemon(binary_path, netns, rootless)
    }

    async fn check_alive(&self) -> bool {
        self.check_alive().await
    }

    async fn set_boot_source(&mut self, kernel_path: &str, boot_args: &str, initramfs: Option<&str>) -> Result<()> {
        self.set_boot_source(kernel_path, boot_args, initramfs).await
    }

    async fn add_drive(&mut self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()> {
        self.add_drive(drive_id, host_path, is_root).await
    }

    async fn set_machine_config(&mut self, vcpu_count: u32, mem_size_mib: u32) -> Result<()> {
        self.set_machine_config(vcpu_count, mem_size_mib).await
    }

    async fn add_network_interface(&mut self, iface_id: &str, host_dev_name: &str, guest_mac: Option<&str>) -> Result<()> {
        self.add_network_interface(iface_id, host_dev_name, guest_mac).await
    }

    async fn add_file_system(&mut self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()> {
        self.add_file_system(fs_id, socket_path, tag).await
    }

    async fn add_vsock(&mut self, cid: u32, socket_path: &str) -> Result<()> {
        self.add_vsock(cid, socket_path).await
    }

    async fn enable_serial(&mut self, file_path: &str) -> Result<()> {
        self.enable_serial(file_path).await
    }

    async fn set_tpm(&mut self, socket_path: &str) -> Result<()> {
        self.set_tpm(socket_path).await
    }

    async fn start_instance(&self) -> Result<()> {
        self.start_instance().await
    }

    async fn pause_instance(&self) -> Result<()> {
        self.pause_instance().await
    }

    async fn resume_instance(&self) -> Result<()> {
        self.resume_instance().await
    }

    async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.create_snapshot(snapshot_path, mem_file_path).await
    }

    async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.load_snapshot(snapshot_path, mem_file_path).await
    }

    async fn send_migration(&self, target_url: &str, bandwidth_mbps: Option<u32>) -> Result<()> {
        self.send_migration(target_url, bandwidth_mbps).await
    }

    async fn receive_migration(&self, receiver_url: &str) -> Result<()> {
        self.receive_migration(receiver_url).await
    }

    async fn cancel_migration(&self) -> Result<()> {
        self.cancel_migration().await
    }

    async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()> {
        self.resize_disk(disk_id, desired_size).await
    }

    fn kill(&mut self) -> Result<()> {
        self.kill()
    }
}

#[cfg(target_os = "linux")]
impl Drop for VmmManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

/// Polymorphic VMM handle used by `vyomad` so VMs can run natively via Cloud Hypervisor
/// or transparently fall back to QEMU TCG when `/dev/kvm` is unavailable.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub enum AnyVmm {
    CloudHypervisor(VmmManager),
    QemuTcg(crate::qemu::QemuTcgManager),
}

#[cfg(target_os = "linux")]
impl From<VmmManager> for AnyVmm {
    fn from(v: VmmManager) -> Self {
        Self::CloudHypervisor(v)
    }
}

#[cfg(target_os = "linux")]
impl From<crate::qemu::QemuTcgManager> for AnyVmm {
    fn from(q: crate::qemu::QemuTcgManager) -> Self {
        Self::QemuTcg(q)
    }
}

#[cfg(target_os = "linux")]
impl AnyVmm {
    fn as_backend(&self) -> &dyn VmmBackend {
        match self {
            Self::CloudHypervisor(v) => v,
            Self::QemuTcg(q) => q,
        }
    }

    fn as_backend_mut(&mut self) -> &mut dyn VmmBackend {
        match self {
            Self::CloudHypervisor(v) => v,
            Self::QemuTcg(q) => q,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.as_backend().pid()
    }

    pub fn kill(&mut self) -> Result<()> {
        self.as_backend_mut().kill()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        self.as_backend_mut().try_wait()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent> {
        self.as_backend().subscribe_events()
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.as_backend().subscribe_logs()
    }

    pub async fn pause_instance(&self) -> Result<()> {
        self.as_backend().pause_instance().await
    }

    pub async fn resume_instance(&self) -> Result<()> {
        self.as_backend().resume_instance().await
    }

    pub async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.as_backend().create_snapshot(snapshot_path, mem_file_path).await
    }

    pub async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.as_backend().load_snapshot(snapshot_path, mem_file_path).await
    }

    pub async fn cancel_migration(&self) -> Result<()> {
        self.as_backend().cancel_migration().await
    }

    pub async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()> {
        self.as_backend().resize_disk(disk_id, desired_size).await
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl VmmBackend for AnyVmm {
    fn socket_path(&self) -> &str { self.as_backend().socket_path() }
    fn event_monitor_path(&self) -> Option<&str> { self.as_backend().event_monitor_path() }
    fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent> { self.as_backend().subscribe_events() }
    fn subscribe_logs(&self) -> broadcast::Receiver<String> { self.as_backend().subscribe_logs() }
    fn pid(&self) -> Option<u32> { self.as_backend().pid() }
    fn try_wait(&mut self) -> Result<Option<ExitStatus>> { self.as_backend_mut().try_wait() }
    fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()> {
        self.as_backend_mut().start_daemon(binary_path, netns, rootless)
    }
    async fn check_alive(&self) -> bool { self.as_backend().check_alive().await }
    async fn set_boot_source(&mut self, kernel_path: &str, boot_args: &str, initramfs: Option<&str>) -> Result<()> {
        self.as_backend_mut().set_boot_source(kernel_path, boot_args, initramfs).await
    }
    async fn add_drive(&mut self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()> {
        self.as_backend_mut().add_drive(drive_id, host_path, is_root).await
    }
    async fn set_machine_config(&mut self, vcpu_count: u32, mem_size_mib: u32) -> Result<()> {
        self.as_backend_mut().set_machine_config(vcpu_count, mem_size_mib).await
    }
    async fn add_network_interface(&mut self, iface_id: &str, host_dev_name: &str, guest_mac: Option<&str>) -> Result<()> {
        self.as_backend_mut().add_network_interface(iface_id, host_dev_name, guest_mac).await
    }
    async fn add_file_system(&mut self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()> {
        self.as_backend_mut().add_file_system(fs_id, socket_path, tag).await
    }
    async fn add_vsock(&mut self, cid: u32, socket_path: &str) -> Result<()> {
        self.as_backend_mut().add_vsock(cid, socket_path).await
    }
    async fn enable_serial(&mut self, file_path: &str) -> Result<()> {
        self.as_backend_mut().enable_serial(file_path).await
    }
    async fn set_tpm(&mut self, socket_path: &str) -> Result<()> {
        self.as_backend_mut().set_tpm(socket_path).await
    }
    async fn start_instance(&self) -> Result<()> { self.as_backend().start_instance().await }
    async fn pause_instance(&self) -> Result<()> { self.as_backend().pause_instance().await }
    async fn resume_instance(&self) -> Result<()> { self.as_backend().resume_instance().await }
    async fn create_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.as_backend().create_snapshot(snapshot_path, mem_file_path).await
    }
    async fn load_snapshot(&self, snapshot_path: &str, mem_file_path: &str) -> Result<()> {
        self.as_backend().load_snapshot(snapshot_path, mem_file_path).await
    }
    async fn send_migration(&self, target_url: &str, bandwidth_mbps: Option<u32>) -> Result<()> {
        self.as_backend().send_migration(target_url, bandwidth_mbps).await
    }
    async fn receive_migration(&self, receiver_url: &str) -> Result<()> {
        self.as_backend().receive_migration(receiver_url).await
    }
    async fn cancel_migration(&self) -> Result<()> { self.as_backend().cancel_migration().await }
    async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()> {
        self.as_backend().resize_disk(disk_id, desired_size).await
    }
    fn kill(&mut self) -> Result<()> { self.as_backend_mut().kill() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmm_event_deserialization() {
        let json_str = r#"{
            "timestamp": {
                "secs": 12,
                "nanos": 345678
            },
            "source": "vmm",
            "event": "started",
            "properties": null
        }"#;

        let event: VmmEvent = serde_json::from_str(json_str).expect("Failed to parse event");
        assert_eq!(event.source, "vmm");
        assert_eq!(event.event, "started");
        assert_eq!(event.timestamp, Some(VmmTimestamp { secs: 12, nanos: 345678 }));
    }

    #[test]
    fn test_vmm_event_migration_cancelled() {
        let json_str = r#"{
            "source": "vm",
            "event": "migration-cancelled"
        }"#;

        let event: VmmEvent = serde_json::from_str(json_str).expect("Failed to parse cancellation event");
        assert_eq!(event.source, "vm");
        assert_eq!(event.event, "migration-cancelled");
        assert!(event.timestamp.is_none());
    }

    #[test]
    fn test_resize_disk_config_serialization() {
        use crate::ch_types::VmResizeDiskConfig;
        let config = VmResizeDiskConfig {
            id: "drive-root".to_string(),
            desired_size: 10 * 1024 * 1024 * 1024,
        };
        let json = serde_json::to_string(&config).expect("serialization failed");
        assert!(json.contains("\"id\":\"drive-root\""));
        assert!(json.contains("\"desired_size\":10737418240"));
    }

    #[test]
    fn test_json_stream_parser_multiline_and_chunked() {
        let mut parser = JsonStreamParser::default();
        let part1 = "{\n  \"source\": \"vmm\",\n  \"event\": \"star";
        let part2 = "ted\"\n}\n{\n  \"source\": \"vm\",\n  \"event\": \"resumed\"\n}\n";

        assert!(parser.feed(part1).is_empty());
        let events = parser.feed(part2);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].source, "vmm");
        assert_eq!(events[0].event, "started");
        assert_eq!(events[1].source, "vm");
        assert_eq!(events[1].event, "resumed");
    }
}


