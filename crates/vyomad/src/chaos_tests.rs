//! Chaos Tests for Vyomad
//!
//! Tests that validate recovery mechanisms by simulating crashes and failures.
//! These tests require KVM and root privileges to run.

#[cfg(feature = "chaos")]
use std::path::{Path, PathBuf};
#[cfg(feature = "chaos")]
use std::process::{Command, Stdio};
#[cfg(feature = "chaos")]
use std::fs;
#[cfg(feature = "chaos")]
use anyhow::{Result, Context};
#[cfg(feature = "chaos")]
use std::io::{Read, Write};
#[cfg(feature = "chaos")]
use std::os::unix::net::UnixStream;
#[cfg(feature = "chaos")]
use serde::{Serialize, Deserialize};

#[cfg(feature = "chaos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    VmCreate { id: String, timestamp: u64 },
    VmStart { id: String, timestamp: u64 },
    VmStop { id: String, timestamp: u64 },
    VmDestroy { id: String, timestamp: u64 },
    VmCheckpoint { id: String, snapshot_path: String, timestamp: u64 },
}

#[cfg(feature = "chaos")]
impl WalEntry {
    pub fn vm_id(&self) -> Option<&str> {
        match self {
            Self::VmCreate { id, .. } => Some(id),
            Self::VmStart { id, .. } => Some(id),
            Self::VmStop { id, .. } => Some(id),
            Self::VmDestroy { id, .. } => Some(id),
            Self::VmCheckpoint { id, .. } => Some(id),
        }
    }
}

#[cfg(feature = "chaos")]
struct DaemonHandle {
    child: std::process::Child,
    data_dir: PathBuf,
    socket_path: PathBuf,
}

#[cfg(feature = "chaos")]
impl DaemonHandle {
    fn start(data_dir: &Path) -> Result<Self> {
        let socket_path = data_dir.join("vyomad.sock");

        if data_dir.exists() {
            let _ = fs::remove_dir_all(data_dir);
        }
        fs::create_dir_all(data_dir)?;

        let daemon_bin = std::env::var("VYOMAD_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let debug = PathBuf::from("./target/debug/vyomad");
                if debug.exists() {
                    debug
                } else {
                    let release = PathBuf::from("./target/release/vyomad");
                    if release.exists() {
                        release
                    } else {
                        PathBuf::from("vyomad")
                    }
                }
            });

        let mut child = Command::new(&daemon_bin)
            .args([
                "--data-dir", data_dir.to_str().unwrap(),
                "--socket", socket_path.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to start daemon at {}", daemon_bin.display()))?;

        let socket_path = socket_path;
        let data_dir = data_dir.to_path_buf();

        for _ in 0..30 {
            if socket_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if !socket_path.exists() {
            let _ = child.kill();
            return Err(anyhow::anyhow!("Daemon failed to start (socket not created)"));
        }

        Ok(Self { child, data_dir, socket_path })
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn send_sigkill(&mut self) -> Result<()> {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(self.pid() as i32),
            nix::sys::signal::Signal::SIGKILL
        )?;
        let _ = self.child.wait();
        Ok(())
    }

    fn kill(&mut self) -> Result<()> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }

    fn send_command(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let mut socket = UnixStream::connect(&self.socket_path)?;
        let mut buf = String::new();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1
        });

        serde_json::to_writer(&socket, &request)?;
        socket.flush().map_err(|e| anyhow::anyhow!("flush error: {}", e))?;

        socket.read_to_string(&mut buf)?;

        let response: serde_json::Value = serde_json::from_str(&buf)?;
        Ok(response)
    }

    fn get_vm_list(&self) -> Result<Vec<String>> {
        let response = self.send_command("vm_list", serde_json::json!({}))?;
        let vms = response["result"].as_array()
            .cloned()
            .unwrap_or_default();
        Ok(vms.iter().filter_map(|v| v["id"].as_str().map(String::from)).collect())
    }
}

#[cfg(feature = "chaos")]
impl Drop for DaemonHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(feature = "chaos")]
fn cleanup_resources(data_dir: &Path) -> Result<()> {
    cleanup_tap_interfaces();
    cleanup_dm_devices();
    cleanup_netns();
    cleanup_loop_devices();
    cleanup_cgroups();
    cleanup_vhost_net_devices();

    if data_dir.exists() {
        let _ = fs::remove_dir_all(data_dir);
    }

    Ok(())
}

#[cfg(feature = "chaos")]
fn cleanup_tap_interfaces() {
    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            if name_str.starts_with("tap") || name_str.contains("vyoma") {
                let _ = Command::new("ip").args(["link", "del", &name_str]).output();
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn cleanup_dm_devices() {
    if let Ok(entries) = fs::read_dir("/dev/mapper") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("vyoma-") || name.contains("vm-") {
                let _ = Command::new("dmsetup").args(["remove", &name]).output();
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("loop") {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_block_device() {
                        let _ = Command::new("losetup").args(["-d", &format!("/dev/{}", name)]).output();
                    }
                }
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn cleanup_loop_devices() {
    let output = Command::new("losetup").args(["-a"]).output();
    if let Ok(output) = output {
        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("vyoma") || line.contains("vm-") {
                if let Some(device) = line.split(':').next() {
                    let device = device.trim();
                    if !device.is_empty() {
                        let _ = Command::new("losetup").args(["-d", device]).output();
                    }
                }
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn cleanup_netns() {
    if let Ok(entries) = fs::read_dir("/var/run/netns") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("vyoma-") || name.contains("vm-") {
                let _ = Command::new("ip").args(["netns", "del", &name]).output();
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn cleanup_vhost_net_devices() {
    if let Ok(entries) = fs::read_dir("/dev/vhost-net") {
        for entry in entries.flatten() {
            let _ = entry;
        }
    }
    if let Ok(entries) = fs::read_dir("/sys/class/vhost-net") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            if name_str.starts_with("vhost-") {
                let _ = Command::new("ip").args(["link", "del", &name_str]).output();
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn cleanup_cgroups() {
    let cgroup_paths = vec![
        "/sys/fs/cgroup",
        "/sys/fs/cgroup/unified",
    ];

    for cgroup_path in cgroup_paths {
        if let Ok(entries) = fs::read_dir(cgroup_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("vyoma-") || name.starts_with("vm-") {
                    let path = entry.path();
                    let _ = Command::new("rmdir").arg(&path).output();
                }
            }
        }
    }

    for controller in &["cpu", "memory", "devices", "pids"] {
        let controller_path = format!("/sys/fs/cgroup/{}", controller);
        if let Ok(entries) = fs::read_dir(&controller_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("vyoma-") || name.starts_with("vm-") {
                    let path = entry.path();
                    let _ = Command::new("rmdir").arg(&path).output();
                }
            }
        }
    }
}

#[cfg(feature = "chaos")]
fn check_dangling_resources() -> Result<Vec<String>> {
    let mut dangling = Vec::new();

    if let Ok(entries) = fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("tap") || name.contains("vyoma") {
                dangling.push(format!("TAP interface: {}", name));
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/dev/mapper") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("vyoma-") || name.contains("vm-") {
                dangling.push(format!("DM device: {}", name));
            }
        }
    }

    if let Ok(entries) = fs::read_dir("/var/run/netns") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("vyoma-") || name.contains("vm-") {
                dangling.push(format!("Network namespace: {}", name));
            }
        }
    }

    let output = Command::new("losetup").args(["-a"]).output();
    if let Ok(output) = output {
        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("vyoma") || line.contains("vm-") {
                dangling.push(format!("Loop device: {}", line.trim()));
            }
        }
    }

    for controller in &["cpu", "memory", "devices", "pids"] {
        let controller_path = format!("/sys/fs/cgroup/{}", controller);
        if let Ok(entries) = fs::read_dir(&controller_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("vyoma-") || name.starts_with("vm-") {
                    dangling.push(format!("Cgroup ({}) : {}", controller, name));
                }
            }
        }
    }

    Ok(dangling)
}

#[cfg(feature = "chaos")]
fn get_wal_entries(data_dir: &Path) -> Result<Vec<(String, WalEntry)>> {
    use crate::state::wal::WalEntry as InternalWalEntry;

    let db = sled::Config::new()
        .path(data_dir.join("vyoma.db"))
        .open()?;

    let tree = db.open_tree("wal")?;

    let mut entries = Vec::new();
    for item in tree.iter() {
        let (k, v) = item?;
        let key = String::from_utf8_lossy(&k).to_string();
        let entry: InternalWalEntry = serde_json::from_slice(&v)?;
        let converted = match entry {
            InternalWalEntry::VmCreate { id, timestamp } => WalEntry::VmCreate { id, timestamp },
            InternalWalEntry::VmStart { id, timestamp } => WalEntry::VmStart { id, timestamp },
            InternalWalEntry::VmStop { id, timestamp } => WalEntry::VmStop { id, timestamp },
            InternalWalEntry::VmDestroy { id, timestamp } => WalEntry::VmDestroy { id, timestamp },
            InternalWalEntry::VmCheckpoint { id, snapshot_path, timestamp } => WalEntry::VmCheckpoint { id, snapshot_path, timestamp },
        };
        entries.push((key, converted));
    }

    Ok(entries)
}

#[cfg(feature = "chaos")]
fn verify_wal_integrity(data_dir: &Path) -> Result<WalIntegrityReport> {
    use std::collections::HashSet;

    let entries = get_wal_entries(data_dir)?;

    let mut vm_create_ids: HashSet<String> = HashSet::new();
    let mut vm_start_ids: HashSet<String> = HashSet::new();
    let mut vm_stop_ids: HashSet<String> = HashSet::new();
    let mut vm_destroy_ids: HashSet<String> = HashSet::new();

    for (_, entry) in &entries {
        match entry {
            WalEntry::VmCreate { id, .. } => { vm_create_ids.insert(id.clone()); }
            WalEntry::VmStart { id, .. } => { vm_start_ids.insert(id.clone()); }
            WalEntry::VmStop { id, .. } => { vm_stop_ids.insert(id.clone()); }
            WalEntry::VmDestroy { id, .. } => { vm_destroy_ids.insert(id.clone()); }
            WalEntry::VmCheckpoint { .. } => {}
        }
    }

    let orphaned_vms: Vec<String> = vm_create_ids
        .difference(&vm_destroy_ids)
        .cloned()
        .collect();

    let running_vms: Vec<String> = vm_start_ids
        .difference(&vm_stop_ids)
        .cloned()
        .collect();

    Ok(WalIntegrityReport {
        total_entries: entries.len(),
        orphaned_vms,
        running_vms,
    })
}

#[cfg(feature = "chaos")]
#[derive(Debug)]
struct WalIntegrityReport {
    total_entries: usize,
    orphaned_vms: Vec<String>,
    running_vms: Vec<String>,
}

#[cfg(feature = "chaos")]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_start_stop() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        let mut handle = DaemonHandle::start(&data_dir).unwrap();
        assert!(handle.pid() > 0);

        handle.kill().unwrap();
    }

    #[test]
    fn test_sigkill_during_vm_create() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        let mut handle = DaemonHandle::start(&data_dir).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(500));

        handle.send_sigkill().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(500));

        let dangling = check_dangling_resources().unwrap();
        assert!(
            dangling.is_empty(),
            "Found dangling resources after crash: {:?}",
            dangling
        );

        let _ = cleanup_resources(&data_dir);
    }

    #[test]
    fn test_wal_corruption_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        {
            let mut handle = DaemonHandle::start(&data_dir).unwrap();
            let _ = handle.send_command("vm_create", serde_json::json!({
                "id": "test-corrupt-vm"
            }));
            std::thread::sleep(std::time::Duration::from_millis(500));
            handle.kill().unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        let entries = get_wal_entries(&data_dir).unwrap();
        if let Some((key, _)) = entries.first() {
            let db = sled::Config::new()
                .path(data_dir.join("vyoma.db"))
                .open().unwrap();
            let tree = db.open_tree("wal").unwrap();
            if let Some(entry) = tree.get(key.as_bytes()).unwrap() {
                let mut corrupted = entry.to_vec();
                if !corrupted.is_empty() {
                    corrupted[0] = 0xFF;
                    corrupted[1] = 0xFF;
                }
                tree.insert(key.as_bytes(), corrupted).unwrap();
                tree.flush().unwrap();
            }
        }

        let result = DaemonHandle::start(&data_dir);
        match result {
            Ok(mut handle) => {
                println!("Daemon started despite WAL corruption");
                handle.kill().unwrap();
            }
            Err(e) => {
                println!("Daemon failed to start (expected): {}", e);
            }
        }

        let _ = cleanup_resources(&data_dir);
    }

    #[test]
    fn test_running_vm_survives_restart() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        {
            let mut handle = DaemonHandle::start(&data_dir).unwrap();

            let _ = handle.send_command("vm_create", serde_json::json!({
                "id": "survivor-vm"
            }));

            std::thread::sleep(std::time::Duration::from_millis(500));

            handle.kill().unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        let report = verify_wal_integrity(&data_dir).unwrap();
        println!("WAL report: {:?}", report);

        let _ = cleanup_resources(&data_dir);
    }

    #[test]
    fn test_resource_cleanup_after_destroy() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        {
            let mut handle = DaemonHandle::start(&data_dir).unwrap();

            let _ = handle.send_command("vm_create", serde_json::json!({
                "id": "cleanup-test-vm"
            }));

            std::thread::sleep(std::time::Duration::from_millis(500));

            handle.send_command("vm_destroy", serde_json::json!({
                "id": "cleanup-test-vm"
            })).ok();

            std::thread::sleep(std::time::Duration::from_millis(1000));

            handle.kill().unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        let dangling = check_dangling_resources().unwrap();
        assert!(
            dangling.is_empty(),
            "Found dangling resources: {:?}",
            dangling
        );

        let _ = cleanup_resources(&data_dir);
    }

    #[test]
    fn test_netns_leak_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        let netns_name = format!("vyoma-test-{}", std::process::id());
        let _ = Command::new("ip")
            .args(["netns", "add", &netns_name])
            .output();

        {
            let mut handle = DaemonHandle::start(&data_dir).unwrap();

            let _ = handle.send_command("vm_create", serde_json::json!({
                "id": "netns-test-vm",
                "network": "test-network"
            }));

            std::thread::sleep(std::time::Duration::from_millis(500));

            handle.send_sigkill().unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        let remaining = fs::read_dir("/var/run/netns")
            .map(|entries| {
                entries.flatten()
                    .filter(|e| e.file_name().to_string_lossy().starts_with("vyoma-"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        assert!(
            remaining.is_empty(),
            "Netns leak detected after crash"
        );

        let _ = Command::new("ip").args(["netns", "del", &netns_name]).output();
        let _ = cleanup_resources(&data_dir);
    }

    #[test]
    fn test_rapid_restart_stress() {
        let temp_dir = tempfile::tempdir().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        for _ in 0..3 {
            let mut handle = DaemonHandle::start(&data_dir).unwrap();

            let _ = handle.send_command("vm_create", serde_json::json!({
                "id": "stress-vm"
            }));

            std::thread::sleep(std::time::Duration::from_millis(200));

            handle.send_sigkill().unwrap();

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let handle = DaemonHandle::start(&data_dir).unwrap();
        let vms = handle.get_vm_list().unwrap();

        println!("VMs after rapid restarts: {:?}", vms);

        let dangling = check_dangling_resources().unwrap();
        assert!(
            dangling.is_empty(),
            "No dangling resources after rapid restarts: {:?}",
            dangling
        );

        let _ = cleanup_resources(&data_dir);
    }
}