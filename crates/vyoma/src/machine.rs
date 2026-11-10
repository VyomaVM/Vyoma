use anyhow::{anyhow, Context as AnyhowContext, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::context;

/// Provisioning driver selected for a managed local Linux microVM machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MachineDriver {
    /// Windows Subsystem for Linux 2 with `nestedVirtualization=true` + Cloud Hypervisor KVM.
    Wsl2,
    /// macOS Apple `Virtualization.framework` (`vz`) via Lima with nested KVM (Apple M3+ / Intel).
    LimaVzNested,
    /// macOS Apple Silicon M1/M2 via Lima (`vz`) running `vyomad` in QEMU TCG software fallback mode.
    LimaVzTcg,
    /// Native Linux host running `vyomad` directly.
    LinuxLocal,
}

impl std::fmt::Display for MachineDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wsl2 => write!(f, "wsl2"),
            Self::LimaVzNested => write!(f, "lima-vz (nested KVM)"),
            Self::LimaVzTcg => write!(f, "lima-vz (QEMU TCG)"),
            Self::LinuxLocal => write!(f, "linux-local"),
        }
    }
}

/// Detected host OS / CPU virtualization profile.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPlatformProfile {
    LinuxNative {
        arch: String,
        has_kvm: bool,
    },
    WindowsWsl2 {
        wsl_installed: bool,
        nested_virt_configured: bool,
    },
    MacAppleSiliconM3Plus {
        chip: String,
        macos_version: String,
        supports_nested_vz: bool,
        limactl_installed: bool,
    },
    MacAppleSiliconM1M2 {
        chip: String,
        macos_version: String,
        limactl_installed: bool,
    },
    MacIntel {
        chip: String,
        macos_version: String,
        limactl_installed: bool,
    },
}

/// Persisted metadata for a managed `vyoma machine`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineSpec {
    pub name: String,
    pub driver: MachineDriver,
    pub arch: String,
    pub cpus: u32,
    pub memory: String,
    pub disk: String,
    pub hypervisor_mode: String,
    pub http_port: u16,
    pub config_path: PathBuf,
    pub context_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MachineRegistry {
    #[serde(default)]
    pub machines: Vec<MachineSpec>,
}

/// Default config directory for `vyoma machine` state and profiles.
pub fn machines_base_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vyoma")
        .join("machines")
}

pub fn machines_registry_path() -> PathBuf {
    machines_base_dir().join("machines.json")
}

pub fn load_registry(path: &Path) -> Result<MachineRegistry> {
    if !path.exists() {
        return Ok(MachineRegistry::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(MachineRegistry::default());
    }
    serde_json::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))
}

pub fn save_registry(path: &Path, registry: &MachineRegistry) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(registry)?)?;
    Ok(())
}

/// Parse a macOS `sw_vers -productVersion` string (e.g. `"15.2.1"`) into its major version.
#[allow(dead_code)]
pub fn parse_macos_major_version(version: &str) -> Option<u32> {
    version.trim().split('.').next()?.parse::<u32>().ok()
}

/// Classify macOS chip + OS version into M1/M2 (no nested virt), M3+ (macOS 15+ nested `vz`), or Intel.
#[allow(dead_code)]
pub fn classify_mac_profile(
    chip_brand: &str,
    macos_version: &str,
    arch: &str,
    limactl_installed: bool,
) -> HostPlatformProfile {
    let lower = chip_brand.to_ascii_lowercase();
    let major = parse_macos_major_version(macos_version);

    if arch == "x86_64" || lower.contains("intel") {
        return HostPlatformProfile::MacIntel {
            chip: chip_brand.trim().to_string(),
            macos_version: macos_version.trim().to_string(),
            limactl_installed,
        };
    }

    // Apple M3, M4, M5+ support hardware nested virtualization on macOS 15 (Sequoia)+.
    let is_m3_plus = ["m3", "m4", "m5", "m6"]
        .iter()
        .any(|tag| lower.contains(tag));

    if is_m3_plus {
        let supports_nested_vz = major.map_or(true, |m| m >= 15);
        HostPlatformProfile::MacAppleSiliconM3Plus {
            chip: chip_brand.trim().to_string(),
            macos_version: macos_version.trim().to_string(),
            supports_nested_vz,
            limactl_installed,
        }
    } else {
        HostPlatformProfile::MacAppleSiliconM1M2 {
            chip: chip_brand.trim().to_string(),
            macos_version: macos_version.trim().to_string(),
            limactl_installed,
        }
    }
}

/// Returns `true` if a Windows `.wslconfig` string enables `nestedVirtualization=true`.
pub fn check_wslconfig_nested_virt(ini_content: &str) -> bool {
    let mut in_wsl2 = false;
    for raw_line in ini_content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_wsl2 = line[1..line.len() - 1].trim().eq_ignore_ascii_case("wsl2");
            continue;
        }
        if in_wsl2 {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case("nestedVirtualization")
                    && v.trim().eq_ignore_ascii_case("true")
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Produce an updated `.wslconfig` string with `[wsl2]\nnestedVirtualization=true`.
pub fn ensure_wslconfig_nested_virt(existing: &str) -> String {
    if check_wslconfig_nested_virt(existing) {
        return existing.to_string();
    }
    let trimmed = existing.trim();
    if trimmed.is_empty() {
        return "[wsl2]\nnestedVirtualization=true\n".to_string();
    }

    let mut out = Vec::new();
    let mut saw_wsl2 = false;
    let mut inserted = false;

    for line in existing.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            if saw_wsl2 && !inserted {
                out.push("nestedVirtualization=true".to_string());
                inserted = true;
            }
            saw_wsl2 = t[1..t.len() - 1].trim().eq_ignore_ascii_case("wsl2");
        } else if saw_wsl2 {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case("nestedVirtualization") {
                    out.push("nestedVirtualization=true".to_string());
                    inserted = true;
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }

    if saw_wsl2 && !inserted {
        out.push("nestedVirtualization=true".to_string());
    } else if !saw_wsl2 {
        if !out.is_empty() {
            out.push(String::new());
        }
        out.push("[wsl2]".to_string());
        out.push("nestedVirtualization=true".to_string());
    }

    let mut joined = out.join("\n");
    if !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// Detect the current host OS and virtualization capability profile.
pub fn detect_host_profile() -> HostPlatformProfile {
    #[cfg(target_os = "linux")]
    {
        HostPlatformProfile::LinuxNative {
            arch: std::env::consts::ARCH.to_string(),
            has_kvm: Path::new("/dev/kvm").exists(),
        }
    }

    #[cfg(target_os = "windows")]
    {
        let wsl_installed = Command::new("wsl.exe")
            .arg("--status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let nested_virt_configured = dirs::home_dir()
            .map(|h| h.join(".wslconfig"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|cfg| check_wslconfig_nested_virt(&cfg))
            .unwrap_or(false);
        HostPlatformProfile::WindowsWsl2 {
            wsl_installed,
            nested_virt_configured,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let chip = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Apple Silicon".to_string());
        let macos_version = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "15.0".to_string());
        let limactl_installed = Command::new("which")
            .arg("limactl")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        classify_mac_profile(&chip, &macos_version, std::env::consts::ARCH, limactl_installed)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        HostPlatformProfile::LinuxNative {
            arch: std::env::consts::ARCH.to_string(),
            has_kvm: false,
        }
    }
}

/// Parse memory strings (`2G`, `4096M`, `4GiB`) into MiB for Lima config generation.
pub fn parse_memory_to_mib(mem: &str) -> Result<u32> {
    let s = mem.trim().to_ascii_lowercase();
    if let Some(v) = s.strip_suffix("gib").or_else(|| s.strip_suffix('g')) {
        let n: f64 = v.trim().parse()?;
        return Ok((n * 1024.0).round() as u32);
    }
    if let Some(v) = s.strip_suffix("mib").or_else(|| s.strip_suffix('m')) {
        let n: f64 = v.trim().parse()?;
        return Ok(n.round() as u32);
    }
    Ok(s.parse::<u32>()?)
}

/// Generate a Lima YAML template configured for Mac M3+ (nested KVM + Cloud Hypervisor),
/// Mac M1/M2 (Apple `vz` + QEMU TCG fallback inside Linux VM), or Intel Mac.
pub fn generate_lima_yaml(spec: &MachineSpec) -> String {
    let nested_virt = matches!(spec.driver, MachineDriver::LimaVzNested);
    let mem_gib = ((parse_memory_to_mib(&spec.memory).unwrap_or(2048) as f64) / 1024.0).max(1.0);
    let mem_str = format!("{:.0}GiB", mem_gib.ceil());
    let lima_arch = if spec.arch == "amd64" || spec.arch == "x86_64" {
        "x86_64"
    } else {
        "aarch64"
    };
    let qemu_pkg = if lima_arch == "aarch64" {
        "qemu-system-arm"
    } else {
        "qemu-system-x86"
    };

    let ch_asset = if lima_arch == "aarch64" {
        "cloud-hypervisor-static-aarch64"
    } else {
        "cloud-hypervisor-static"
    };
    let kernel_asset = if lima_arch == "aarch64" {
        "Image"
    } else {
        "bzImage-x86_64"
    };

    let nested_block = if nested_virt {
        "nestedVirtualization: true\n"
    } else {
        ""
    };

    format!(
        r#"# Generated by `vyoma machine init` for {name} ({driver})
vmType: "vz"
arch: "{lima_arch}"
{nested_block}cpus: {cpus}
memory: "{mem_str}"
disk: "{disk}"

images:
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img"
    arch: "x86_64"
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-arm64.img"
    arch: "aarch64"

mounts:
  - location: "~"
    writable: true
  - location: "/tmp/vyoma"
    writable: true

portForwards:
  - guestIP: "127.0.0.1"
    guestPort: {http_port}
    hostIP: "127.0.0.1"
    hostPort: {http_port}

provision:
  - mode: system
    script: |
      #!/bin/bash
      set -euxo pipefail
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -y
      apt-get install -y curl e2fsprogs squashfs-tools qemu-utils {qemu_pkg} || true
      mkdir -p /var/lib/vyoma/bin /run/vyoma
      if [ ! -f /var/lib/vyoma/bin/cloud-hypervisor ]; then
        curl -fL -o /var/lib/vyoma/bin/cloud-hypervisor "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/v54.0/{ch_asset}" || true
        chmod +x /var/lib/vyoma/bin/cloud-hypervisor || true
      fi
      if [ ! -f /var/lib/vyoma/bin/vmlinux ]; then
        curl -fL -o /var/lib/vyoma/bin/vmlinux "https://github.com/cloud-hypervisor/linux/releases/download/ch-release-v6.16.9-20260508/{kernel_asset}" || true
      fi
      cat <<EOF > /etc/default/vyoma
      VYOMA_HYPERVISOR={hypervisor_mode}
      EOF
"#,
        name = spec.name,
        driver = spec.driver,
        lima_arch = lima_arch,
        nested_block = nested_block,
        cpus = spec.cpus,
        mem_str = mem_str,
        disk = spec.disk,
        http_port = spec.http_port,
        qemu_pkg = qemu_pkg,
        ch_asset = ch_asset,
        kernel_asset = kernel_asset,
        hypervisor_mode = spec.hypervisor_mode,
    )
}

/// Generate a WSL2 Linux setup script for Windows 10/11 running `vyomad` natively with `/dev/kvm`.
pub fn generate_wsl_bootstrap_script(spec: &MachineSpec) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
sudo mkdir -p /var/lib/vyoma/bin /run/vyoma
if [ ! -e /dev/kvm ]; then
  echo 'WARNING: /dev/kvm missing inside WSL2. Ensure %USERPROFILE%\.wslconfig has nestedVirtualization=true and run "wsl --shutdown".'
fi
if ! command -v vyomad >/dev/null 2>&1; then
  echo "Install vyoma .deb inside WSL2 (or copy vyomad to /usr/local/bin) before starting the service."
fi
echo "Starting vyomad on 127.0.0.1:{port} (mode={mode})..."
sudo VYOMA_HYPERVISOR="{mode}" nohup vyomad --http-bind-ip 127.0.0.1 --http-port {port} --hypervisor "{mode}" --keep-root >/var/log/vyomad.log 2>&1 &
"#,
        port = spec.http_port,
        mode = spec.hypervisor_mode,
    )
}

/// Resolve the recommended `MachineDriver` & hypervisor mode from CLI `--driver` + host profile.
pub fn resolve_driver_and_mode(
    driver_arg: &str,
    profile: &HostPlatformProfile,
) -> Result<(MachineDriver, String)> {
    match driver_arg.trim().to_ascii_lowercase().as_str() {
        "auto" => match profile {
            HostPlatformProfile::WindowsWsl2 { .. } => {
                Ok((MachineDriver::Wsl2, "auto".to_string()))
            }
            HostPlatformProfile::MacAppleSiliconM3Plus {
                supports_nested_vz, ..
            } => {
                if *supports_nested_vz {
                    Ok((MachineDriver::LimaVzNested, "auto".to_string()))
                } else {
                    Ok((MachineDriver::LimaVzTcg, "qemu-tcg".to_string()))
                }
            }
            HostPlatformProfile::MacIntel { .. } => {
                Ok((MachineDriver::LimaVzNested, "auto".to_string()))
            }
            HostPlatformProfile::MacAppleSiliconM1M2 { .. } => {
                Ok((MachineDriver::LimaVzTcg, "qemu-tcg".to_string()))
            }
            HostPlatformProfile::LinuxNative { has_kvm, .. } => {
                let mode = if *has_kvm { "auto" } else { "qemu-tcg" };
                Ok((MachineDriver::LinuxLocal, mode.to_string()))
            }
        },
        "wsl2" | "wsl" => Ok((MachineDriver::Wsl2, "auto".to_string())),
        "lima-vz" | "vz" | "m3" => Ok((MachineDriver::LimaVzNested, "auto".to_string())),
        "lima-tcg" | "tcg" | "qemu" | "m1" | "m2" => {
            Ok((MachineDriver::LimaVzTcg, "qemu-tcg".to_string()))
        }
        "linux" | "local" => Ok((MachineDriver::LinuxLocal, "auto".to_string())),
        other => Err(anyhow!(
            "Unsupported machine driver '{}': expected auto, wsl2, lima-vz, lima-tcg, or linux",
            other
        )),
    }
}

pub fn init_machine(
    name: String,
    cpus: u32,
    memory: String,
    disk: String,
    arch_override: Option<String>,
    driver_arg: &str,
    http_port: u16,
    dry_run: bool,
) -> Result<()> {
    let profile = detect_host_profile();
    let (driver, hypervisor_mode) = resolve_driver_and_mode(driver_arg, &profile)?;
    let arch = arch_override.unwrap_or_else(|| match std::env::consts::ARCH {
        "aarch64" => "aarch64".to_string(),
        _ => "x86_64".to_string(),
    });

    let machine_dir = machines_base_dir().join(&name);
    let config_path = match driver {
        MachineDriver::Wsl2 => machine_dir.join("bootstrap-wsl2.sh"),
        MachineDriver::LimaVzNested | MachineDriver::LimaVzTcg => machine_dir.join("lima.yaml"),
        MachineDriver::LinuxLocal => machine_dir.join("local.json"),
    };

    let spec = MachineSpec {
        name: name.clone(),
        driver,
        arch,
        cpus,
        memory,
        disk,
        hypervisor_mode,
        http_port,
        config_path: config_path.clone(),
        context_name: name.clone(),
    };

    let artifact_body = match driver {
        MachineDriver::Wsl2 => generate_wsl_bootstrap_script(&spec),
        MachineDriver::LimaVzNested | MachineDriver::LimaVzTcg => generate_lima_yaml(&spec),
        MachineDriver::LinuxLocal => serde_json::to_string_pretty(&spec)?,
    };

    if dry_run {
        println!(
            "# Dry-run machine plan for '{}' ({})\n# Config path: {}\n\n{}",
            spec.name,
            spec.driver,
            spec.config_path.display(),
            artifact_body
        );
        return Ok(());
    }

    std::fs::create_dir_all(&machine_dir)?;
    std::fs::write(&config_path, &artifact_body)?;

    // Ensure Windows .wslconfig enables nestedVirtualization=true when missing
    if matches!(driver, MachineDriver::Wsl2) {
        if let Some(home) = dirs::home_dir() {
            let wslconfig = home.join(".wslconfig");
            let current = std::fs::read_to_string(&wslconfig).unwrap_or_default();
            if !check_wslconfig_nested_virt(&current) {
                let updated = ensure_wslconfig_nested_virt(&current);
                std::fs::write(&wslconfig, updated)?;
                println!(
                    "Updated {} with [wsl2] nestedVirtualization=true (run `wsl --shutdown` in PowerShell for WSL2 KVM to take effect).",
                    wslconfig.display()
                );
            }
        }
    }

    // Save into machines registry + configure CLI context pointing to local forwarded port
    let reg_path = machines_registry_path();
    let mut reg = load_registry(&reg_path)?;
    reg.machines.retain(|m| m.name != spec.name);
    reg.machines.push(spec.clone());
    save_registry(&reg_path, &reg)?;

    let ctx_path = context::default_contexts_path();
    let _ = context::create_context(
        &ctx_path,
        spec.context_name.clone(),
        format!("http://127.0.0.1:{}", spec.http_port),
        None,
        true,
    )?;

    println!(
        "Initialized Vyoma machine '{}' (driver: {}, arch: {}, mode: {}).",
        spec.name, spec.driver, spec.arch, spec.hypervisor_mode
    );
    println!("Config written to: {}", config_path.display());
    println!(
        "CLI context '{}' activated -> http://127.0.0.1:{}",
        spec.context_name, spec.http_port
    );

    if matches!(driver, MachineDriver::LimaVzTcg) {
        println!(
            "\nNote: Apple Silicon M1/M2 does not expose hardware nested virtualization.\n\
             This machine configures QEMU TCG software emulation inside Lima for local parity.\n\
             For full KVM speed on M1/M2, connect to a remote Linux KVM host:\n  \
             vyoma remote tunnel my-kvm --ssh user@linux-kvm-host"
        );
    }

    println!("\nNext: run `vyoma machine start {}`", spec.name);
    Ok(())
}

pub fn start_machine(name: &str) -> Result<()> {
    let reg_path = machines_registry_path();
    let reg = load_registry(&reg_path)?;
    let spec = reg
        .machines
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| anyhow!("Machine '{}' not found. Run `vyoma machine init {}` first.", name, name))?;

    match spec.driver {
        MachineDriver::LimaVzNested | MachineDriver::LimaVzTcg => {
            let status = Command::new("limactl")
                .args([
                    "start",
                    "--tty=false",
                    "--name",
                    &spec.name,
                    &spec.config_path.to_string_lossy(),
                ])
                .status()
                .context("Failed to execute `limactl` (install Lima via `brew install lima`)")?;
            if !status.success() {
                return Err(anyhow!("`limactl start` failed with status {}", status));
            }
        }
        MachineDriver::Wsl2 => {
            let script = spec.config_path.to_string_lossy();
            let status = Command::new("wsl.exe")
                .args(["bash", &crate::paths::windows_to_wsl_path(&script)])
                .status()
                .context("Failed to execute `wsl.exe`")?;
            if !status.success() {
                return Err(anyhow!("WSL2 startup failed with status {}", status));
            }
        }
        MachineDriver::LinuxLocal => {
            println!(
                "Linux native host detected; start `vyomad --hypervisor {} --http-port {}` directly or via systemd (`sudo systemctl start vyomad`).",
                spec.hypervisor_mode, spec.http_port
            );
        }
    }

    let ctx_path = context::default_contexts_path();
    let _ = context::use_context(&ctx_path, &spec.context_name);
    println!("Machine '{}' is ready (context '{}').", spec.name, spec.context_name);
    Ok(())
}

pub fn stop_machine(name: &str) -> Result<()> {
    let reg = load_registry(&machines_registry_path())?;
    let spec = reg
        .machines
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| anyhow!("Machine '{}' not found.", name))?;

    match spec.driver {
        MachineDriver::LimaVzNested | MachineDriver::LimaVzTcg => {
            let status = Command::new("limactl")
                .args(["stop", &spec.name])
                .status()
                .context("Failed to execute `limactl stop`")?;
            if !status.success() {
                return Err(anyhow!("`limactl stop` failed with status {}", status));
            }
        }
        MachineDriver::Wsl2 => {
            let status = Command::new("wsl.exe")
                .args(["--terminate", "Ubuntu"])
                .status()
                .context("Failed to execute `wsl.exe --terminate`")?;
            if !status.success() {
                return Err(anyhow!("`wsl.exe --terminate` failed"));
            }
        }
        MachineDriver::LinuxLocal => {
            println!("Use `sudo systemctl stop vyomad` on native Linux hosts.");
        }
    }
    println!("Stopped machine '{}'.", name);
    Ok(())
}

pub fn status_machines(name: Option<&str>) -> Result<()> {
    let profile = detect_host_profile();
    println!("Host platform: {:?}", profile);

    let reg = load_registry(&machines_registry_path())?;
    if reg.machines.is_empty() {
        println!("No managed machines configured. Run `vyoma machine init` to create one.");
        return Ok(());
    }

    println!(
        "{:<14} {:<24} {:<10} {:<12} {}",
        "NAME", "DRIVER", "ARCH", "HYPERVISOR", "ENDPOINT"
    );
    for m in &reg.machines {
        if let Some(filter) = name {
            if m.name != filter {
                continue;
            }
        }
        println!(
            "{:<14} {:<24} {:<10} {:<12} http://127.0.0.1:{}",
            m.name, m.driver, m.arch, m.hypervisor_mode, m.http_port
        );
    }
    Ok(())
}

pub fn remove_machine(name: &str) -> Result<()> {
    let reg_path = machines_registry_path();
    let mut reg = load_registry(&reg_path)?;
    let idx = reg
        .machines
        .iter()
        .position(|m| m.name == name)
        .ok_or_else(|| anyhow!("Machine '{}' not found.", name))?;
    let spec = reg.machines.remove(idx);

    if matches!(
        spec.driver,
        MachineDriver::LimaVzNested | MachineDriver::LimaVzTcg
    ) {
        let _ = Command::new("limactl")
            .args(["delete", "-f", &spec.name])
            .status();
    }

    if let Some(dir) = spec.config_path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    save_registry(&reg_path, &reg)?;
    let _ = context::remove_context(&context::default_contexts_path(), &spec.context_name);
    println!("Removed machine '{}'.", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apple_silicon_m3_vs_m1_m2_detection() {
        // M1 / M2 lacks hardware nested virtualization -> LimaVzTcg + qemu-tcg fallback
        let m1 = classify_mac_profile("Apple M1 Max", "14.6", "aarch64", true);
        let (driver_m1, mode_m1) = resolve_driver_and_mode("auto", &m1).unwrap();
        assert_eq!(driver_m1, MachineDriver::LimaVzTcg);
        assert_eq!(mode_m1, "qemu-tcg");

        let m2 = classify_mac_profile("Apple M2 Pro", "15.1", "aarch64", true);
        let (driver_m2, mode_m2) = resolve_driver_and_mode("auto", &m2).unwrap();
        assert_eq!(driver_m2, MachineDriver::LimaVzTcg);
        assert_eq!(mode_m2, "qemu-tcg");

        // M3+ on macOS 15+ supports nested Virtualization.framework -> Cloud Hypervisor KVM
        let m3_sequoia = classify_mac_profile("Apple M3 Pro", "15.2", "aarch64", true);
        let (driver_m3, mode_m3) = resolve_driver_and_mode("auto", &m3_sequoia).unwrap();
        assert_eq!(driver_m3, MachineDriver::LimaVzNested);
        assert_eq!(mode_m3, "auto");

        // M3 on older macOS 14 falls back to TCG cleanly
        let m3_sonoma = classify_mac_profile("Apple M3", "14.5", "aarch64", true);
        let (driver_old, _) = resolve_driver_and_mode("auto", &m3_sonoma).unwrap();
        assert_eq!(driver_old, MachineDriver::LimaVzTcg);

        // M4 Max on macOS 15 -> LimaVzNested
        let m4 = classify_mac_profile("Apple M4 Max", "15.0", "aarch64", true);
        let (driver_m4, _) = resolve_driver_and_mode("auto", &m4).unwrap();
        assert_eq!(driver_m4, MachineDriver::LimaVzNested);
    }

    #[test]
    fn test_wslconfig_nested_virt_check_and_patch() {
        assert!(!check_wslconfig_nested_virt(""));
        assert!(!check_wslconfig_nested_virt("[wsl2]\nmemory=4GB\n"));
        assert!(check_wslconfig_nested_virt(
            "[wsl2]\nnestedVirtualization=true\n"
        ));

        let patched = ensure_wslconfig_nested_virt("[wsl2]\nmemory=8GB\n");
        assert!(check_wslconfig_nested_virt(&patched));
        assert!(patched.contains("memory=8GB"));
    }

    #[test]
    fn test_lima_yaml_generation_m3_vs_m1() {
        let m3_spec = MachineSpec {
            name: "vyoma-m3".to_string(),
            driver: MachineDriver::LimaVzNested,
            arch: "aarch64".to_string(),
            cpus: 4,
            memory: "4G".to_string(),
            disk: "30GiB".to_string(),
            hypervisor_mode: "auto".to_string(),
            http_port: 8080,
            config_path: PathBuf::from("/tmp/lima.yaml"),
            context_name: "vyoma-m3".to_string(),
        };
        let yaml_m3 = generate_lima_yaml(&m3_spec);
        assert!(yaml_m3.contains("vmType: \"vz\""));
        assert!(yaml_m3.contains("nestedVirtualization: true"));
        assert!(yaml_m3.contains("cloud-hypervisor-static-aarch64"));

        let m1_spec = MachineSpec {
            driver: MachineDriver::LimaVzTcg,
            hypervisor_mode: "qemu-tcg".to_string(),
            ..m3_spec
        };
        let yaml_m1 = generate_lima_yaml(&m1_spec);
        assert!(!yaml_m1.contains("nestedVirtualization: true"));
        assert!(yaml_m1.contains("VYOMA_HYPERVISOR=qemu-tcg"));
        assert!(yaml_m1.contains("qemu-system-arm"));
    }
}
