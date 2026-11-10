use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::Serialize;
use std::fmt;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::net::UnixStream;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::vmm::{VmmBackend, VmmEvent, VmmTimestamp};

#[derive(Debug, Clone)]
struct QemuDriveSpec {
    id: String,
    path: String,
    is_root: bool,
}

#[derive(Debug, Clone)]
struct QemuFsSpec {
    #[allow(dead_code)]
    id: String,
    socket: String,
    tag: String,
}

#[derive(Debug, Clone)]
struct QemuNetSpec {
    #[allow(dead_code)]
    id: String,
    tap: String,
    mac: Option<String>,
}

/// Manages a QEMU TCG process via QMP over a Unix domain socket.
/// Used as a software emulation fallback on hosts without hardware `/dev/kvm`
/// (such as Apple Silicon M1/M2 Linux VMs or nested cloud containers).
pub struct QemuTcgManager {
    socket_path: String,
    process: Option<Child>,
    event_sender: broadcast::Sender<VmmEvent>,
    log_sender: broadcast::Sender<String>,
    vcpus: u32,
    memory_mib: u32,
    kernel_path: Option<String>,
    cmdline: Option<String>,
    initramfs_path: Option<String>,
    drives: Vec<QemuDriveSpec>,
    nets: Vec<QemuNetSpec>,
    filesystems: Vec<QemuFsSpec>,
    vsock: Option<(u32, String)>,
    serial_path: Option<String>,
    tpm_socket: Option<String>,
    binary_path: String,
    arch: String,
}

impl fmt::Debug for QemuTcgManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QemuTcgManager")
            .field("socket_path", &self.socket_path)
            .field("arch", &self.arch)
            .field("binary_path", &self.binary_path)
            .field("pid", &self.process.as_ref().map(|p| p.id()))
            .finish()
    }
}

impl QemuTcgManager {
    pub fn new(socket_path: &str) -> Self {
        let (log_tx, _) = broadcast::channel(100);
        let (event_tx, _) = broadcast::channel(100);
        let arch = std::env::consts::ARCH.to_string();
        let default_bin = qemu_binary_name_for_arch(&arch).to_string();
        Self {
            socket_path: socket_path.to_string(),
            process: None,
            event_sender: event_tx,
            log_sender: log_tx,
            vcpus: 1,
            memory_mib: 512,
            kernel_path: None,
            cmdline: None,
            initramfs_path: None,
            drives: Vec::new(),
            nets: Vec::new(),
            filesystems: Vec::new(),
            vsock: None,
            serial_path: None,
            tpm_socket: None,
            binary_path: default_bin,
            arch,
        }
    }

    pub fn with_arch(mut self, arch: impl Into<String>) -> Self {
        self.arch = arch.into();
        self
    }

    /// Build QEMU TCG CLI arguments for the configured VM hardware and devices.
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "-nographic".to_string(),
            "-nodefaults".to_string(),
            "-no-reboot".to_string(),
        ];

        match self.arch.as_str() {
            "aarch64" | "arm64" => {
                args.push("-machine".to_string());
                args.push("virt,accel=tcg".to_string());
                args.push("-cpu".to_string());
                args.push("cortex-a57".to_string());
            }
            _ => {
                args.push("-machine".to_string());
                args.push("q35,accel=tcg".to_string());
                args.push("-cpu".to_string());
                args.push("max".to_string());
            }
        }

        args.push("-smp".to_string());
        args.push(self.vcpus.max(1).to_string());
        args.push("-m".to_string());
        args.push(format!("{}M", self.memory_mib.max(64)));

        // QMP control channel
        args.push("-chardev".to_string());
        args.push(format!(
            "socket,id=qmp,path={},server=on,wait=off",
            self.socket_path
        ));
        args.push("-mon".to_string());
        args.push("chardev=qmp,mode=control".to_string());

        if let Some(ref kernel) = self.kernel_path {
            args.push("-kernel".to_string());
            args.push(kernel.clone());
        }
        if let Some(ref cmdline) = self.cmdline {
            args.push("-append".to_string());
            args.push(cmdline.clone());
        }
        if let Some(ref initrd) = self.initramfs_path {
            args.push("-initrd".to_string());
            args.push(initrd.clone());
        }

        for drive in &self.drives {
            let fmt = if drive.path.ends_with(".qcow2") {
                "qcow2"
            } else {
                "raw"
            };
            let readonly = if drive.path.ends_with(".qcow2") {
                "off"
            } else if drive.is_root {
                "on"
            } else {
                "off"
            };
            args.push("-drive".to_string());
            args.push(format!(
                "file={},if=virtio,format={},readonly={},id={}",
                drive.path, fmt, readonly, drive.id
            ));
        }

        if self.nets.is_empty() {
            args.push("-netdev".to_string());
            args.push("user,id=net0".to_string());
            args.push("-device".to_string());
            args.push("virtio-net-pci,netdev=net0".to_string());
        } else {
            for (idx, net) in self.nets.iter().enumerate() {
                let netdev_id = format!("net{}", idx);
                args.push("-netdev".to_string());
                args.push(format!(
                    "tap,id={},ifname={},script=no,downscript=no",
                    netdev_id, net.tap
                ));
                args.push("-device".to_string());
                let mut dev = format!("virtio-net-pci,netdev={}", netdev_id);
                if let Some(ref mac) = net.mac {
                    dev.push_str(&format!(",mac={}", mac));
                }
                args.push(dev);
            }
        }

        if !self.filesystems.is_empty() {
            args.push("-object".to_string());
            args.push(format!(
                "memory-backend-memfd,id=mem,size={}M,share=on",
                self.memory_mib.max(64)
            ));
            args.push("-numa".to_string());
            args.push("node,memdev=mem".to_string());

            for (idx, fs) in self.filesystems.iter().enumerate() {
                let char_id = format!("charfs{}", idx);
                args.push("-chardev".to_string());
                args.push(format!("socket,id={},path={}", char_id, fs.socket));
                args.push("-device".to_string());
                args.push(format!("vhost-user-fs-pci,chardev={},tag={}", char_id, fs.tag));
            }
        }

        if let Some((cid, _)) = &self.vsock {
            if Path::new("/dev/vhost-vsock").exists() {
                args.push("-device".to_string());
                args.push(format!("vhost-vsock-pci,guest-cid={}", cid));
            }
        }

        if let Some(ref serial) = self.serial_path {
            args.push("-serial".to_string());
            args.push(format!("file:{}", serial));
        } else {
            args.push("-serial".to_string());
            args.push("stdio".to_string());
        }

        if let Some(ref tpm_sock) = self.tpm_socket {
            args.push("-chardev".to_string());
            args.push(format!("socket,id=chrtpm,path={}", tpm_sock));
            args.push("-tpmdev".to_string());
            args.push("emulator,id=tpm0,chardev=chrtpm".to_string());
            args.push("-device".to_string());
            args.push("tpm-tis,tpmdev=tpm0".to_string());
        }

        args
    }

    async fn qmp_command<T: Serialize>(&self, cmd: &str, args: Option<&T>) -> Result<()> {
        if !Path::new(&self.socket_path).exists() {
            return Err(anyhow!(
                "QEMU QMP socket not found at {} - VM may not be running",
                self.socket_path
            ));
        }

        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| anyhow!("Failed to connect to QEMU QMP socket: {}", e))?;
        let (reader, mut writer) = stream.into_split();
        let mut lines = AsyncBufReader::new(reader).lines();

        // Read QMP greeting
        let _ = lines.next_line().await?;

        // Negotiate capabilities
        writer
            .write_all(b"{\"execute\":\"qmp_capabilities\"}\n")
            .await?;
        let _ = lines.next_line().await?;

        let payload = if let Some(a) = args {
            serde_json::json!({ "execute": cmd, "arguments": a })
        } else {
            serde_json::json!({ "execute": cmd })
        };
        let mut serialized = serde_json::to_string(&payload)?;
        serialized.push('\n');
        writer.write_all(serialized.as_bytes()).await?;
        let resp = lines.next_line().await?.unwrap_or_default();
        if resp.contains("\"error\"") {
            return Err(anyhow!("QMP command '{}' failed: {}", cmd, resp));
        }
        Ok(())
    }

    fn emit_event(&self, event: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let _ = self.event_sender.send(VmmEvent {
            timestamp: Some(VmmTimestamp {
                secs: now.as_secs(),
                nanos: now.subsec_nanos(),
            }),
            source: "qemu-tcg".to_string(),
            event: event.to_string(),
            properties: None,
        });
    }
}

/// Canonical QEMU system emulator binary name for a host architecture.
pub fn qemu_binary_name_for_arch(arch: &str) -> &'static str {
    match arch {
        "aarch64" | "arm64" => "qemu-system-aarch64",
        _ => "qemu-system-x86_64",
    }
}

#[async_trait]
impl VmmBackend for QemuTcgManager {
    fn socket_path(&self) -> &str {
        &self.socket_path
    }

    fn event_monitor_path(&self) -> Option<&str> {
        None
    }

    fn subscribe_events(&self) -> broadcast::Receiver<VmmEvent> {
        self.event_sender.subscribe()
    }

    fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_sender.subscribe()
    }

    fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(|p| p.id())
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
        if let Some(child) = self.process.as_mut() {
            child
                .try_wait()
                .map_err(|e| anyhow!("Failed to check QEMU process status: {}", e))
        } else {
            Ok(None)
        }
    }

    fn start_daemon(&mut self, binary_path: &str, netns: Option<&str>, rootless: bool) -> Result<()> {
        if !binary_path.trim().is_empty() {
            self.binary_path = binary_path.to_string();
        }
        let _ = (netns, rootless);
        info!(
            "Configured QEMU TCG fallback backend (binary={}, qmp_socket={})",
            self.binary_path, self.socket_path
        );
        Ok(())
    }

    async fn check_alive(&self) -> bool {
        self.qmp_command::<()>("query-status", None).await.is_ok()
    }

    async fn set_boot_source(
        &mut self,
        kernel_path: &str,
        boot_args: &str,
        initramfs: Option<&str>,
    ) -> Result<()> {
        self.kernel_path = Some(kernel_path.to_string());
        self.cmdline = Some(boot_args.to_string());
        self.initramfs_path = initramfs.map(|s| s.to_string());
        Ok(())
    }

    async fn add_drive(&mut self, drive_id: &str, host_path: &str, is_root: bool) -> Result<()> {
        self.drives.push(QemuDriveSpec {
            id: drive_id.to_string(),
            path: host_path.to_string(),
            is_root,
        });
        Ok(())
    }

    async fn set_machine_config(&mut self, vcpu_count: u32, mem_size_mib: u32) -> Result<()> {
        self.vcpus = vcpu_count;
        self.memory_mib = mem_size_mib;
        Ok(())
    }

    async fn add_network_interface(
        &mut self,
        iface_id: &str,
        host_dev_name: &str,
        guest_mac: Option<&str>,
    ) -> Result<()> {
        self.nets.push(QemuNetSpec {
            id: iface_id.to_string(),
            tap: host_dev_name.to_string(),
            mac: guest_mac.map(|s| s.to_string()),
        });
        Ok(())
    }

    async fn add_file_system(&mut self, fs_id: &str, socket_path: &str, tag: &str) -> Result<()> {
        self.filesystems.push(QemuFsSpec {
            id: fs_id.to_string(),
            socket: socket_path.to_string(),
            tag: tag.to_string(),
        });
        Ok(())
    }

    async fn add_vsock(&mut self, cid: u32, socket_path: &str) -> Result<()> {
        self.vsock = Some((cid, socket_path.to_string()));
        Ok(())
    }

    async fn enable_serial(&mut self, file_path: &str) -> Result<()> {
        self.serial_path = Some(file_path.to_string());
        Ok(())
    }

    async fn set_tpm(&mut self, socket_path: &str) -> Result<()> {
        self.tpm_socket = Some(socket_path.to_string());
        Ok(())
    }

    async fn start_instance(&self) -> Result<()> {
        // QEMU receives its full machine spec at process launch time; spawn via start_with_process
        // Wait: `VmmBackend::start_instance` takes `&self`, so if already spawned or not yet spawned
        // we report cleanly unless launched via `spawn_configured()`. Let's spawn if socket missing.
        if Path::new(&self.socket_path).exists() {
            return Ok(());
        }
        Err(anyhow!(
            "QemuTcgManager must be spawned via spawn_configured(&mut self) before start_instance"
        ))
    }

    async fn pause_instance(&self) -> Result<()> {
        self.qmp_command::<()>("stop", None).await?;
        self.emit_event("paused");
        Ok(())
    }

    async fn resume_instance(&self) -> Result<()> {
        self.qmp_command::<()>("cont", None).await?;
        self.emit_event("resumed");
        Ok(())
    }

    async fn create_snapshot(&self, _snapshot_path: &str, _mem_file_path: &str) -> Result<()> {
        Err(anyhow!(
            "Memory snapshots require Cloud Hypervisor KVM mode; QEMU TCG supports disk overlays"
        ))
    }

    async fn load_snapshot(&self, _snapshot_path: &str, _mem_file_path: &str) -> Result<()> {
        Err(anyhow!(
            "Snapshot restore requires Cloud Hypervisor KVM mode"
        ))
    }

    async fn send_migration(&self, _target_url: &str, _bandwidth_mbps: Option<u32>) -> Result<()> {
        Err(anyhow!(
            "Live migration requires Cloud Hypervisor KVM mode"
        ))
    }

    async fn receive_migration(&self, _receiver_url: &str) -> Result<()> {
        Err(anyhow!(
            "Live migration requires Cloud Hypervisor KVM mode"
        ))
    }

    async fn cancel_migration(&self) -> Result<()> {
        self.qmp_command::<()>("migrate_cancel", None).await
    }

    async fn resize_disk(&self, disk_id: &str, desired_size: u64) -> Result<()> {
        #[derive(Serialize)]
        struct BlockResizeArgs<'a> {
            device: &'a str,
            size: u64,
        }
        self.qmp_command(
            "block_resize",
            Some(&BlockResizeArgs {
                device: disk_id,
                size: desired_size,
            }),
        )
        .await
    }

    fn kill(&mut self) -> Result<()> {
        if let Some(mut child) = self.process.take() {
            info!("Stopping QEMU TCG process");
            let _ = child.kill();
            let _ = child.wait();
        }
        if Path::new(&self.socket_path).exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
        self.emit_event("shutdown");
        Ok(())
    }
}

impl QemuTcgManager {
    /// Spawns the configured QEMU TCG process and waits for the QMP socket to become ready.
    pub fn spawn_configured(&mut self) -> Result<()> {
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let args = self.build_args();
        info!("Spawning QEMU TCG: {} {}", self.binary_path, args.join(" "));

        let mut child = Command::new(&self.binary_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn {}: {}", self.binary_path, e))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture QEMU stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture QEMU stderr"))?;
        let tx_out = self.log_sender.clone();
        let tx_err = self.log_sender.clone();

        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().flatten() {
                info!("[QEMU STDOUT] {}", line);
                let _ = tx_out.send(format!("[STDOUT] {}", line));
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().flatten() {
                error!("[QEMU STDERR] {}", line);
                let _ = tx_err.send(format!("[STDERR] {}", line));
            }
        });

        self.process = Some(child);
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if Path::new(&self.socket_path).exists() {
                self.emit_event("started");
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(anyhow!("Timed out waiting for QEMU QMP socket at {}", self.socket_path))
    }
}

impl Drop for QemuTcgManager {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_tcg_args_aarch64_for_apple_m1_m2() {
        let mut q = QemuTcgManager::new("/tmp/vm-qmp.sock").with_arch("aarch64");
        q.vcpus = 2;
        q.memory_mib = 1024;
        q.kernel_path = Some("/var/lib/vyoma/bin/vmlinux".to_string());
        q.cmdline = Some("console=ttyAMA0 root=/dev/vda rw".to_string());
        q.drives.push(QemuDriveSpec {
            id: "rootfs".to_string(),
            path: "/tmp/overlay.qcow2".to_string(),
            is_root: true,
        });

        let args = q.build_args();
        assert!(args.windows(2).any(|w| w == ["-machine", "virt,accel=tcg"]));
        assert!(args.windows(2).any(|w| w == ["-cpu", "cortex-a57"]));
        assert!(args.windows(2).any(|w| w == ["-m", "1024M"]));
        assert!(args
            .iter()
            .any(|a| a.contains("file=/tmp/overlay.qcow2,if=virtio,format=qcow2,readonly=off")));
        assert!(args.windows(2).any(|w| w == ["-netdev", "user,id=net0"]));
    }

    #[test]
    fn test_qemu_tcg_args_x86_64() {
        let mut q = QemuTcgManager::new("/tmp/vm-qmp.sock").with_arch("x86_64");
        q.vcpus = 4;
        q.memory_mib = 2048;
        let args = q.build_args();
        assert!(args.windows(2).any(|w| w == ["-machine", "q35,accel=tcg"]));
        assert!(args.windows(2).any(|w| w == ["-smp", "4"]));
        assert_eq!(qemu_binary_name_for_arch("aarch64"), "qemu-system-aarch64");
        assert_eq!(qemu_binary_name_for_arch("x86_64"), "qemu-system-x86_64");
    }
}
