# Vyoma: Technical Implementation Specification
## v1.1 → v2.0 Complete Engineering Guide

**Purpose**: This document is the single source of truth for coding Vyoma from its current v1.1 state to the production-grade v2.0 release. Every section is a direct coding directive — no marketing, no vision statements. Follow this phase by phase.

---

## Table of Contents

1. [Current State Audit — What v1.1 Has and What's Wrong](#1-current-state-audit)
2. [Repository & Crate Architecture Target](#2-repository--crate-architecture-target)
3. [Phase 1 — v1.2: Critical Fixes (8 weeks)](#3-phase-1--v12-critical-fixes)
4. [Phase 2 — v1.3: Foundation Hardening (6 weeks)](#4-phase-2--v13-foundation-hardening)
5. [Phase 3 — v1.5: Power Features (12 weeks)](#5-phase-3--v15-power-features)
6. [Phase 4 — v2.0: Revolutionary Features (16 weeks)](#6-phase-4--v20-revolutionary-features)
7. [Cross-Cutting: Testing Strategy](#7-cross-cutting-testing-strategy)
8. [Cross-Cutting: Packaging & Distribution](#8-cross-cutting-packaging--distribution)
9. [Deprecated Decisions & Migration Guide](#9-deprecated-decisions--migration-guide)

---

## 1. Current State Audit

### 1.1 Crate Inventory

| Crate | Binary | Status | Notes |
|-------|--------|--------|-------|
| `crates/vyoma` | `/usr/bin/vyoma` | ✅ Working | CLI, ~20 commands |
| `crates/vyomad` | `/usr/bin/vyomad` | ✅ Working | Daemon, runs as root |
| `crates/vyoma-core` | library | ✅ Working | OCI, storage, network, vmm |
| `crates/vyoma-compose` | library | ✅ Working | compose YAML parsing, `vyoma up/down` |
| `crates/ui` | — | ✅ Working | TypeScript/React dashboard at `:3000` |

### 1.2 Confirmed Working (Do Not Break)

- `vyoma run <image> --vcpu N --memory MB -p H:V -v H:V --name N`
- `vyoma ps`, `vyoma stop`, `vyoma rm`, `vyoma start`, `vyoma restart`
- `vyoma logs -f`, `vyoma exec <id> <cmd>`
- `vyoma pull`, `vyoma build -t <tag> .` (FROM, RUN, COPY only)
- `vyoma snapshot`, `vyoma restore`, `vyoma export`, `vyoma import`
- `vyoma network create/ls/rm`
- `vyoma up -d`, `vyoma down`, `vyoma scale <svc>=N`
- `vyoma swarm init`, `vyoma swarm join <ip>`, `vyoma swarm ls`
- `vyoma doctor`
- OCI pull from Docker Hub (custom reqwest client, handles Index vs V2 Manifest)
- Device Mapper snapshots for instant clone (dm-snapshot via `dmsetup` CLI calls)
- TAP + Linux bridge (`vyoma0`) networking with NAT
- CNI integration (bridge/ptp plugins)
- Internal DNS on gateway IP (172.16.0.1:53)
- Virtiofs volume mounts (external `virtiofsd` binary required)
- State persistence across daemon restarts (`.vyoma/state/` JSON files)
- VXLAN overlay networking skeleton for Swarm
- `.deb`/`.rpm` packaging with bundled Firecracker binary
- `systemd` service unit for `vyomad`

### 1.3 Known Broken / Missing (Coding Debt Ranked by Severity)

#### CRITICAL — blocks production adoption
1. **CMD/ENTRYPOINT/ENV not parsed** — Vyomafile `FROM`/`RUN`/`COPY` work but `CMD`, `ENTRYPOINT`, `ENV` are silently dropped. VMs start `/bin/sh`. Most Docker Hub images are non-functional.
2. **virtiofsd is an unmanaged external dep** — `-v` silently fails on clean installs. "Batteries Included" promise is broken.
3. **vyomad runs as full root** — ADR-019 adopted the Docker model but never constrained capabilities. Should be a dedicated `vyoma` system user with `CAP_NET_ADMIN` + `CAP_SYS_ADMIN` only, not a full root process.
4. **No WAL / crash recovery** — ADR-008 acknowledged daemon restart loses running VM control handles. ADR consequence: "Future task: State persistence/recovery" — never implemented properly. JSON state files partially address this but have no crash-safe write path.

#### IMPORTANT — blocks cluster production use
5. **Compose schema `version: "1.0"` is incompatible with Docker Compose v3** — Users cannot `vyoma up` from existing `docker-compose.yml` without manual edits. `networks:` top-level key is not supported. All services share one default bridge.
6. **Swarm VXLAN traffic is unencrypted plaintext** — No WireGuard or any encryption on the overlay. Multi-tenant or cloud deployment is insecure.
7. **Swarm uses seed-node model** — Single point of failure. No documented recovery if seed goes down.
8. **Git-based time travel is a hack** — Using `git commit` on snapshot files works but: (a) git binary is an external dep, (b) it has no delta storage, (c) checkout semantics are wrong for VM state, (d) not atomic.

#### ENHANCEMENT — quality/ecosystem
9. **CLI system calls (ADR-002)** — `std::process::Command` wrapping `dmsetup`, `losetup`, `iptables`, `ip link` works but is fragile. Error handling is string parsing of stderr.
10. **No gRPC interface** — REST-only API blocks Kubernetes CRI implementation.
11. **No VMIF image format** — Images are raw OCI converted at runtime every pull. No signing, no caching spec, no defined format for the Hub.
12. **`vyoma commit`, `vyoma save`, `vyoma load` missing** — Referenced in docs but not implemented.
13. **`vyoma rm` behavior unclear** — Must clean up: DM snapshot, loop device, COW file, tap device, state JSON.
14. **Environment variable injection** — `environment:` key in compose YAML is WIP (noted in README).

---

## 2. Repository & Crate Architecture Target

### 2.1 Workspace Cargo.toml Changes

Add these crates progressively through the phases. Each gets its own `crates/<name>/` directory and entry in the workspace `members` array.

```toml
# Cargo.toml (workspace root) — Target state
[workspace]
members = [
    "crates/vyoma",             # CLI binary
    "crates/vyomad",         # Daemon binary
    "crates/vyoma-core",     # VM lifecycle, OCI, storage, network, vmm
    "crates/vyoma-compose",  # Compose YAML schema + orchestrator
    "crates/vyoma-net",      # Network management (Phase 1 refactor)
    "crates/vyoma-storage",  # Storage layer refactor (Phase 2)
    "crates/vyoma-image",    # VMIF format + OCI bridge (Phase 3)
    "crates/vyoma-agent",    # In-VM agent binary, musl target (Phase 4)
    "crates/vyoma-teleport", # Pre-copy live migration (Phase 3)
    "crates/vyoma-proto",    # Protobuf definitions for gRPC (Phase 3)
]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
thiserror = "1"
axum = "0.7"
reqwest = { version = "0.11", features = ["json", "stream"] }
clap = { version = "4", features = ["derive"] }
uuid = { version = "1", features = ["v4"] }
```

### 2.2 Final Directory Structure

```
micro-vm-ecosystem/
├── crates/
│   ├── ign/                    # CLI
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/       # One file per command group
│   │       │   ├── lifecycle.rs   (run, stop, start, restart, rm, ps)
│   │       │   ├── image.rs       (pull, build, push, images, rmi, tag)
│   │       │   ├── network.rs     (network create/ls/rm/connect)
│   │       │   ├── volume.rs      (volume create/ls/rm)
│   │       │   ├── snapshot.rs    (snapshot, restore, history, time-travel, branch, diff)
│   │       │   ├── compose.rs     (up, down, scale, logs)
│   │       │   ├── swarm.rs       (swarm init/join/ls, service create/ls/update)
│   │       │   └── system.rs      (doctor, stats, inspect)
│   │       └── client.rs       # HTTP+gRPC client for vyomad
│   │
│   ├── vyomad/                # Daemon
│   │   └── src/
│   │       ├── main.rs
│   │       ├── api/            # REST handlers (axum routes)
│   │       │   ├── vms.rs
│   │       │   ├── images.rs
│   │       │   ├── networks.rs
│   │       │   ├── volumes.rs
│   │       │   ├── snapshots.rs
│   │       │   └── swarm.rs
│   │       ├── state/          # WAL + persistent state
│   │       │   ├── wal.rs         # Write-ahead log using sled
│   │       │   ├── store.rs       # State store abstraction
│   │       │   └── recovery.rs    # WAL replay on startup
│   │       ├── vm_manager.rs   # VM lifecycle FSM
│   │       ├── firecracker.rs  # Firecracker API wrapper
│   │       └── metrics.rs      # Prometheus exporter
│   │
│   ├── vyoma-core/            # Existing — refactored
│   │   └── src/
│   │       ├── oci.rs          # OCI client (keep custom impl, improve)
│   │       ├── layers.rs       # Layer unpacking
│   │       ├── vmm.rs          # Firecracker HTTP client
│   │       └── types.rs        # Shared domain types
│   │
│   ├── vyoma-net/             # NEW — extracted from vyoma-core
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── bridge.rs       # Linux bridge management via rtnetlink
│   │       ├── tap.rs          # TAP device creation
│   │       ├── ipam.rs         # IP allocation (deterministic subnet leases)
│   │       ├── dns.rs          # Embedded DNS server
│   │       ├── nat.rs          # iptables NAT management
│   │       └── wireguard.rs    # boringtun integration (Phase 3)
│   │
│   ├── vyoma-storage/         # NEW — extracted + improved
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── dm.rs           # Device Mapper via devicemapper-rs crate
│   │       ├── cow.rs          # CoW layer management
│   │       ├── ext4.rs         # ext4 image creation and population
│   │       └── snapshot_tree.rs # CoW delta history tree (TimeMachine backend)
│   │
│   ├── vyoma-image/           # NEW — VMIF format
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── vmif.rs         # VMIF struct (kernel + rootfs + metadata)
│   │       ├── hub_bridge.rs   # Docker Hub → VMIF conversion
│   │       └── signing.rs      # Ed25519 signing (Phase 4)
│   │
│   ├── vyoma-compose/         # Existing — extended
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── schema_v3.rs    # Docker Compose v3 schema (replaces v1.0)
│   │       └── orchestrator.rs
│   │
│   ├── vyoma-proto/           # NEW — gRPC definitions (Phase 3)
│   │   ├── proto/
│   │   │   ├── vm.proto
│   │   │   ├── image.proto
│   │   │   └── cri.proto       # Kubernetes CRI v1
│   │   └── build.rs
│   │
│   ├── vyoma-teleport/        # NEW — live migration (Phase 3)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── sender.rs       # Source node: dirty page tracking
│   │       ├── receiver.rs     # Destination node: page receiver
│   │       └── protocol.rs     # Wire protocol for memory transfer
│   │
│   └── vyoma-agent/           # NEW — in-VM binary (Phase 4)
│       └── src/
│           └── main.rs         # Compiled for musl, .vyoma400KB static binary
│
├── services/
│   └── vyoma-hub/             # NEW — OCI registry + Docker Hub bridge (Phase 3)
│       └── src/
│           ├── main.rs
│           ├── registry.rs     # OCI registry protocol
│           ├── bridge.rs       # Docker Hub pull + VMIF conversion
│           └── cache.rs        # Converted image cache
│
├── vk8s/                       # NEW — Kubernetes CRI plugin in Go (Phase 4)
│   ├── cmd/vk8s-shim/
│   └── pkg/cri/
│
├── ui/                         # Existing TypeScript dashboard
│   └── src/
│
├── tests/
│   ├── integration/            # Existing
│   ├── chaos/                  # NEW — WAL crash recovery tests (Phase 1)
│   ├── compat/                 # NEW — Docker Hub image compat matrix (Phase 2)
│   └── bench/                  # NEW — perf benchmarks (Phase 2)
│
├── kernels/                    # NEW — slim kernel build configs (Phase 3)
├── packaging/
│   ├── systemd/
│   │   └── vyomad.service     # systemd unit with capability constraints
│   ├── deb/
│   └── rpm/
├── scripts/
├── bin/                        # Bundled Firecracker binary
└── Cargo.toml
```

---

## 3. Phase 1 — v1.2: Critical Fixes

**Duration**: 8 weeks  
**Goal**: Make `vyoma run` actually work for the vast majority of Docker Hub images. Make the system production-safe at the privilege level. Make the daemon crash-safe.

### 3.1 CMD / ENTRYPOINT / ENV Support in Vyomafile + OCI Bridge

**Crate**: `vyoma-core` (oci.rs, layers.rs) + `crates/vyomad` (vm startup path)  
**Priority**: P0 — highest impact fix in the entire project

#### 3.1.1 What to Build

When `vyoma pull <image>` or `vyoma build` runs, extract the OCI image config JSON and persist it alongside the ext4 rootfs. When `vyoma run` starts a VM, inject the config values as a small init wrapper.

#### 3.1.2 Implementation

**Step 1**: Extend the OCI client in `vyoma-core/src/oci.rs` to parse the image config blob.

```rust
// vyoma-core/src/oci.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciImageConfig {
    /// e.g. ["/usr/sbin/nginx", "-g", "daemon off;"]
    pub entrypoint: Option<Vec<String>>,
    /// e.g. ["-c", "/etc/nginx/nginx.conf"]
    pub cmd: Option<Vec<String>>,
    /// e.g. ["PATH=/usr/local/bin:/usr/bin", "PORT=8080"]
    pub env: Option<Vec<String>>,
    /// Working directory inside the VM
    pub working_dir: Option<String>,
    /// Exposed ports metadata (informational only)
    pub exposed_ports: Option<HashMap<String, serde_json::Value>>,
    /// User to run as (informational, used to set up init script)
    pub user: Option<String>,
}

impl OciImageConfig {
    /// Produce the full command to exec: ENTRYPOINT + CMD combined
    pub fn full_command(&self) -> Vec<String> {
        let mut cmd = vec![];
        if let Some(ep) = &self.entrypoint {
            cmd.extend_from_slice(ep);
        }
        if let Some(c) = &self.cmd {
            cmd.extend_from_slice(c);
        }
        if cmd.is_empty() {
            cmd.push("/bin/sh".to_string());
        }
        cmd
    }
}
```

**Step 2**: After layer flattening, write `vyoma-config.json` into the image cache directory.

```
.vyoma/.vyoma/images/<image-hash>/
    rootfs.ext4       # Read-only base image
    vyoma-config.json  # NEW: parsed OCI config
    manifest.json
```

**Step 3**: In `crates/vyomad/src/vm_manager.rs`, before calling the Firecracker boot API, generate `/sbin/vyoma-init` inside the VM's rootfs COW layer and set it as the kernel `init=` boot parameter.

```rust
// crates/vyomad/src/vm_manager.rs

fn generate_init_script(config: &OciImageConfig) -> String {
    let mut script = String::from("#!/bin/sh\n");
    
    // Inject environment variables
    if let Some(env_vars) = &config.env {
        for var in env_vars {
            // var is "KEY=VALUE"
            script.push_str(&format!("export {}\n", var));
        }
    }
    
    // Set working directory
    if let Some(wd) = &config.working_dir {
        script.push_str(&format!("cd {}\n", wd));
    }
    
    // Exec the actual command (ENTRYPOINT + CMD)
    let full_cmd = config.full_command();
    let exec_line = full_cmd.iter()
        .map(|s| shell_escape(s))
        .collect::<Vec<_>>()
        .join(" ");
    script.push_str(&format!("exec {}\n", exec_line));
    
    script
}

/// Inject /sbin/vyoma-init into the COW layer before VM boot.
/// We use debugfs to write the script without mounting the filesystem.
fn inject_init_script(cow_path: &Path, config: &OciImageConfig) -> Result<()> {
    let script = generate_init_script(config);
    let script_path = temp_file_with_content(&script)?;
    
    // Write script into ext4 image via debugfs (no mount required, no root needed)
    Command::new("debugfs")
        .args(["-w", cow_path.to_str().unwrap()])
        .stdin(format!("write {} /sbin/vyoma-init\nchmod 0755 /sbin/vyoma-init\n", 
                       script_path.display()))
        .status()?;
    
    Ok(())
}
```

**Step 4**: Pass `init=/sbin/vyoma-init` in the kernel boot arguments when launching Firecracker.

```rust
// In the kernel boot args construction:
let boot_args = format!(
    "console=ttyS0 reboot=k panic=1 pci=off \
     ip={vm_ip}::172.16.0.1:255.255.0.0::eth0:on \
     hostname={name} \
     init=/sbin/vyoma-init"
);
```

**Step 5**: For the Vyomafile `CMD`, `ENTRYPOINT`, `ENV` directives in `vyoma build`:

```rust
// crates/vyoma-core/src/oci.rs - Vyomafile parser extension

pub enum VyomafileInstruction {
    From(String),
    Run(String),
    Copy { src: String, dst: String },
    Cmd(Vec<String>),            // NEW
    Entrypoint(Vec<String>),     // NEW
    Env { key: String, val: String }, // NEW
    WorkDir(String),             // NEW
    Expose(u16),                 // NEW (informational)
    // VM-specific extensions
    VmKernel(String),
    VmVcpus(u32),
    VmMemory(u64),
    VmSnapshotPolicy(String),
    VmIopsLimit(u64),
}
```

After `vyoma build` completes, serialize the accumulated `CMD`/`ENTRYPOINT`/`ENV` into `vyoma-config.json` in the image cache.

### 3.2 Bundle virtiofsd — Make `-v` Work on Clean Install

**Crate**: `packaging/deb/`, `packaging/rpm/`, `crates/vyomad/src/`  
**Priority**: P0

#### 3.2.1 Static virtiofsd Bundle

Add the static `virtiofsd` binary to `bin/` alongside `firecracker`. The packaging scripts must include it in the `.deb`/`.rpm` at `/usr/lib/vyoma/virtiofsd`.

```bash
# packaging/scripts/download_deps.sh
VIRTIOFSD_VERSION="v1.11.1"
VIRTIOFSD_URL="https://gitlab.com/virtio-fs/virtiofsd/-/releases/${VIRTIOFSD_VERSION}/downloads/virtiofsd-x86_64"
curl -L -o bin/virtiofsd "${VIRTIOFSD_URL}"
chmod +x bin/virtiofsd
```

#### 3.2.2 Daemon virtiofsd Lookup

```rust
// crates/vyomad/src/vm_manager.rs

fn find_virtiofsd() -> Option<PathBuf> {
    // Priority order: bundled > system PATH
    let candidates = [
        PathBuf::from("/usr/lib/vyoma/virtiofsd"),
        PathBuf::from("/usr/bin/virtiofsd"),
        PathBuf::from("/usr/local/bin/virtiofsd"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn start_virtiofsd(host_path: &Path, socket: &Path) -> Result<Child> {
    let binary = find_virtiofsd()
        .ok_or_else(|| anyhow!("virtiofsd not found. This is a packaging bug."))?;
    
    Command::new(binary)
        .args([
            &format!("--socket-path={}", socket.display()),
            "--shared-dir", host_path.to_str().unwrap(),
            "--cache=auto",
        ])
        .spawn()
        .map_err(Into::into)
}
```

#### 3.2.3 ign doctor Check

```rust
// crates/vyoma/src/commands/system.rs

fn check_virtiofsd(results: &mut Vec<DoctorCheck>) {
    let found = ["/usr/lib/vyoma/virtiofsd", "/usr/bin/virtiofsd"]
        .iter()
        .any(|p| Path::new(p).exists());
    
    results.push(DoctorCheck {
        name: "virtiofsd".to_string(),
        status: if found { CheckStatus::Ok } else { CheckStatus::Warning },
        message: if found {
            "virtiofsd found — volume mounts enabled".to_string()
        } else {
            "virtiofsd not found — volume mounts (-v) will fail. Run: sudo apt install virtiofsd".to_string()
        },
    });
}
```

### 3.3 Privilege Model Fix — Dedicated System User

**Files**: `packaging/systemd/vyomad.service`, `vyomad/src/main.rs`  
**Priority**: P0

#### 3.3.1 systemd Service Unit — Constrained Capabilities

Replace the current blanket-root service with a dedicated user and explicit capabilities:

```ini
# packaging/systemd/vyomad.service
[Unit]
Description=Vyoma MicroVM Daemon
Documentation=https://github.com/Subeshrock/micro-vm-ecosystem
After=network.target
Wants=network.target

[Service]
Type=notify
ExecStart=/usr/bin/vyomad
ExecReload=/bin/kill -s HUP $MAINPID

# ── Privilege Model ──────────────────────────────────────────────
# Run as dedicated vyoma system user, NOT root
User=vyoma
Group=vyoma

# Grant only the capabilities actually needed:
#   CAP_NET_ADMIN  — create/manage TAP devices, bridges, iptables rules
#   CAP_SYS_ADMIN  — mount operations, device mapper, cgroups v2 delegation
#   CAP_NET_RAW    — raw socket access for VXLAN
#   CAP_SETUID     — for jailer subprocess (Firecracker)
#   CAP_SETGID     — for jailer subprocess
AmbientCapabilities=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_RAW CAP_SETUID CAP_SETGID
CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_RAW CAP_SETUID CAP_SETGID

# Lock down the rest
NoNewPrivileges=false
PrivateTmp=true
ProtectHome=read-only
ProtectSystem=false  # Must manage /var/lib/vyoma

# Runtime directory
RuntimeDirectory=vyoma
RuntimeDirectoryMode=0750
StateDirectory=vyoma
StateDirectoryMode=0750

# Socket group allows ign CLI (user) to connect
SocketGroup=vyoma

[Install]
WantedBy=multi-user.target
```

#### 3.3.2 Post-Install Script

```bash
# packaging/scripts/postinstall.sh
#!/bin/sh
# Create vyoma system user if it doesn't exist
if ! id vyoma >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin \
            --comment "Vyoma MicroVM Daemon" vyoma
fi

# Add vyoma to kvm group for /dev/kvm access
usermod -aG kvm vyoma

# Set socket permissions so ign CLI users can connect
# Users must be in the 'vyoma' group to use ign CLI
chown root:vyoma /var/run/vyoma.sock 2>/dev/null || true
chmod 0660 /var/run/vyoma.sock 2>/dev/null || true

# /dev/kvm access
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

systemctl daemon-reload
systemctl enable vyomad
```

#### 3.3.3 Socket Permissions in vyomad

```rust
// crates/vyomad/src/main.rs

fn create_api_socket(path: &Path) -> Result<UnixListener> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    
    // Set socket to group-writable so 'vyoma' group members can connect
    std::os::unix::fs::chown(path, None, Some(get_gid("vyoma")?))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    
    Ok(listener)
}
```

### 3.4 WAL + Crash Recovery

**Crate**: `crates/vyomad/src/state/` (new module)  
**Priority**: P1 — prevents data loss on daemon crash

#### 3.4.1 WAL Design

Use `sled` embedded database for the WAL. Sled is pure Rust, has atomic batch writes, and does not require a separate process.

```toml
# crates/vyomad/Cargo.toml
[dependencies]
sled = "0.34"
```

#### 3.4.2 WAL Entries

Every mutation to daemon state writes a WAL entry before changing in-memory state:

```rust
// crates/vyomad/src/state/wal.rs

use sled::{Db, Tree};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    VmCreating { vm_id: String, config: VmConfig },
    VmStarted  { vm_id: String, pid: u32, ip: String, fc_socket: String },
    VmStopping { vm_id: String },
    VmStopped  { vm_id: String },
    VmDestroyed { vm_id: String },
    SnapshotCreated { vm_id: String, snap_id: String, path: String },
    VolumeAttached  { vm_id: String, host_path: String, vm_path: String },
    NetworkCreated  { net_id: String, name: String, subnet: String, bridge: String },
    NetworkDeleted  { net_id: String },
}

pub struct Wal {
    db: Db,
    log: Tree,
    state: Tree,
}

impl Wal {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db = sled::open(data_dir.join("wal.db"))?;
        let log = db.open_tree("log")?;
        let state = db.open_tree("state")?;
        Ok(Self { db, log, state })
    }

    /// Append an entry to the log and flush synchronously (O_SYNC semantics)
    pub fn append(&self, entry: &WalEntry) -> Result<u64> {
        let seq = self.db.generate_id()?;
        let key = seq.to_be_bytes();
        let value = serde_json::to_vec(entry)?;
        self.log.insert(key, value)?;
        self.log.flush()?;  // fsync — critical for crash safety
        Ok(seq)
    }

    /// Write committed state (used after a transition is complete)
    pub fn commit_state(&self, vm_id: &str, state: &VmState) -> Result<()> {
        let value = serde_json::to_vec(state)?;
        self.state.insert(vm_id.as_bytes(), value)?;
        self.state.flush()?;
        Ok(())
    }

    /// Remove a VM from committed state
    pub fn remove_state(&self, vm_id: &str) -> Result<()> {
        self.state.remove(vm_id.as_bytes())?;
        self.state.flush()?;
        Ok(())
    }

    /// Iterate all committed states — called on daemon startup for recovery
    pub fn all_states(&self) -> Result<Vec<VmState>> {
        self.state
            .iter()
            .map(|r| {
                let (_, v) = r?;
                Ok(serde_json::from_slice(&v)?)
            })
            .collect()
    }
}
```

#### 3.4.3 Recovery on Startup

```rust
// crates/vyomad/src/state/recovery.rs

pub struct RecoveryResult {
    pub adopted: Vec<String>,   // VMs successfully re-adopted
    pub orphaned: Vec<String>,  // VMs whose Firecracker process is gone
    pub cleaned: Vec<String>,   // Resources cleaned up
}

pub async fn recover_from_wal(wal: &Wal, fc_manager: &mut VmManager) -> Result<RecoveryResult> {
    let mut result = RecoveryResult::default();
    
    for state in wal.all_states()? {
        match state.status {
            VmStatus::Running { pid, fc_socket, ip } => {
                // Check if the Firecracker process is still alive
                if process_exists(pid) {
                    // Re-adopt the running VM — reconnect to Firecracker socket
                    match fc_manager.adopt(state.vm_id.clone(), pid, fc_socket, ip).await {
                        Ok(_) => result.adopted.push(state.vm_id),
                        Err(e) => {
                            tracing::warn!("Failed to adopt VM {}: {}", state.vm_id, e);
                            cleanup_orphan(&state, wal).await?;
                            result.orphaned.push(state.vm_id);
                        }
                    }
                } else {
                    // Process is gone — clean up DM, TAP, COW resources
                    cleanup_orphan(&state, wal).await?;
                    result.orphaned.push(state.vm_id);
                }
            }
            VmStatus::Creating => {
                // Was in the middle of creation — incomplete, clean up
                cleanup_orphan(&state, wal).await?;
                result.cleaned.push(state.vm_id);
            }
            VmStatus::Stopped => {
                // Just restore stopped VM metadata into memory (no process to adopt)
                fc_manager.restore_stopped_metadata(state);
            }
        }
    }
    
    Ok(result)
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}
```

### 3.5 Docker Compose v3 Schema Compatibility

**Crate**: `crates/vyoma-compose/src/`  
**Priority**: P1

#### 3.5.1 Replace Custom v1.0 Schema

Create `schema_v3.rs` that parses standard Docker Compose v3 YAML. The existing `vyoma-compose.yml` with `version: "1.0"` must still parse for backward compatibility, then internally convert to the v3 schema struct.

```rust
// crates/vyoma-compose/src/schema_v3.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Root compose file — compatible with Docker Compose v3.x
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFile {
    /// Accept "1.0", "3", "3.8", "3.x" — map all to v3 internally
    #[serde(default)]
    pub version: Option<String>,
    
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    
    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,
    
    #[serde(default)]
    pub volumes: HashMap<String, VolumeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub image: Option<String>,
    pub build: Option<BuildConfig>,
    
    #[serde(default)]
    pub ports: Vec<PortMapping>,         // ["8080:80", "443:443"]
    
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,       // ["./data:/var/lib/data"]
    
    #[serde(default)]
    pub environment: EnvSpec,            // Map or list form
    
    #[serde(default)]
    pub networks: Vec<String>,           // ["frontend", "backend"]
    
    #[serde(default)]
    pub depends_on: DependsOnSpec,
    
    #[serde(default)]
    pub healthcheck: Option<HealthCheck>,
    
    pub deploy: Option<DeployConfig>,   // for 'replicas'
    
    /// VM-specific extension block (Vyoma-only, ignored by docker-compose)
    pub vm: Option<VmExtension>,
    
    // Renamed from v1.0 'cpus'/'memory' — keep for backward compat
    pub cpus: Option<f64>,
    pub memory: Option<u64>,
    pub command: Option<CommandSpec>,   // Override CMD
    pub entrypoint: Option<CommandSpec>, // Override ENTRYPOINT
    pub hostname: Option<String>,
    pub restart: Option<String>,        // "no", "always", "on-failure", "unless-stopped"
}

/// Vyoma-specific VM configuration — silently ignored by docker-compose
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmExtension {
    pub kernel: Option<String>,         // "vyoma/kernels:6.1-slim"
    pub vcpus: Option<u32>,
    pub memory: Option<u64>,            // MiB
    pub iops_limit: Option<u32>,
    pub snapshot_interval: Option<String>, // "1h", "30m"
    pub volume_encryption: Option<String>, // "aes256-xts"
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkConfig {
    pub driver: Option<String>,         // "bridge", "overlay", "host", "none"
    pub driver_opts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub internal: bool,                 // No external connectivity
    pub ipam: Option<IpamConfig>,
    /// Vyoma overlay will use WireGuard automatically when driver="overlay"
    pub external: Option<bool>,
}

/// For DependsOn — support both list form and condition form
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependsOnSpec {
    List(Vec<String>),
    Map(HashMap<String, DependsOnCondition>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependsOnCondition {
    pub condition: String, // "service_started", "service_healthy", "service_completed_successfully"
}

/// Support both map form and list form for environment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvSpec {
    Map(HashMap<String, String>),
    List(Vec<String>),  // "KEY=VALUE" or "KEY" (inherit from host)
}
```

#### 3.5.2 networks: Top-Level Key — Network Segmentation

When a compose stack specifies `networks:`, create a separate Linux bridge per named network. Services without an explicit network assignment go on the default bridge.

```rust
// crates/vyoma-compose/src/orchestrator.rs

async fn provision_compose_networks(
    compose: &ComposeFile,
    daemon_client: &DaemonClient,
) -> Result<HashMap<String, CreatedNetwork>> {
    let mut created = HashMap::new();
    
    for (name, config) in &compose.networks {
        let driver = config.driver.as_deref().unwrap_or("bridge");
        
        let req = CreateNetworkRequest {
            name: format!("{}_{}_{}", stack_name, name, random_suffix()),
            driver: driver.to_string(),
            internal: config.internal,
            subnet: config.ipam.as_ref()
                .and_then(|i| i.config.first())
                .and_then(|c| c.subnet.clone()),
        };
        
        let net = daemon_client.create_network(req).await?;
        created.insert(name.clone(), net);
    }
    
    // Ensure default network exists for services without explicit network config
    if !created.contains_key("default") {
        let default_net = daemon_client.create_network(CreateNetworkRequest {
            name: format!("{}_default", stack_name),
            driver: "bridge".to_string(),
            ..Default::default()
        }).await?;
        created.insert("default".to_string(), default_net);
    }
    
    Ok(created)
}
```

### 3.6 Fix `vyoma rm` Resource Cleanup

**Crate**: `crates/vyomad/src/api/vms.rs` and `vm_manager.rs`  
**Priority**: P1 — current behavior leaves dangling resources

#### 3.6.1 Complete Teardown Sequence

```rust
// crates/vyomad/src/vm_manager.rs

pub async fn destroy_vm(&self, vm_id: &str) -> Result<()> {
    let state = self.get_state(vm_id)?;
    
    // 1. Ensure VM is stopped
    if state.status == VmStatus::Running { .. } {
        self.stop_vm(vm_id).await?;
    }
    
    // 2. Kill virtiofsd processes for this VM
    for vol in &state.volumes {
        if let Some(pid) = vol.virtiofsd_pid {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        }
    }
    
    // 3. Remove Device Mapper snapshot
    if let Some(dm_name) = &state.dm_device_name {
        Command::new("dmsetup").args(["remove", dm_name]).status()?;
    }
    
    // 4. Detach loop device for COW file  
    if let Some(loop_dev) = &state.cow_loop_dev {
        Command::new("losetup").args(["-d", loop_dev]).status()?;
    }
    
    // 5. Delete COW file
    if let Some(cow_path) = &state.cow_path {
        let _ = std::fs::remove_file(cow_path);
    }
    
    // 6. Delete TAP device
    if let Some(tap_name) = &state.tap_device {
        Command::new("ip")
            .args(["link", "delete", tap_name])
            .status()?;
    }
    
    // 7. Release IP back to IPAM pool
    self.ipam.release(&state.ip)?;
    
    // 8. Remove Firecracker socket and working directory
    if let Some(fc_dir) = &state.firecracker_dir {
        let _ = std::fs::remove_dir_all(fc_dir);
    }
    
    // 9. Remove from WAL and in-memory state
    self.wal.remove_state(vm_id)?;
    self.vms.lock().await.remove(vm_id);
    
    Ok(())
}
```

### 3.7 Implement `vyoma commit`, `vyoma save`, `vyoma load`

**Crate**: `crates/vyoma/src/commands/image.rs`, `crates/vyomad/src/api/images.rs`  
**Priority**: P2

#### 3.7.1 `vyoma commit <vm-id> <new-image-tag>`

Pauses the VM, flushes the COW delta to the base image layer to create a new read-only image, then resumes.

```rust
// crates/vyomad/src/api/images.rs

pub async fn commit_vm(vm_id: &str, tag: &str, vm_manager: &VmManager) -> Result<ImageId> {
    // 1. Pause VM via Firecracker API
    vm_manager.pause_vm(vm_id).await?;
    
    // 2. Merge COW layer into new base image
    //    Use dmsetup to read the merged snapshot into a new ext4 file
    let src_dm = vm_manager.get_dm_device(vm_id)?;
    let new_image_path = images_dir().join(format!("{}.ext4", tag_to_path(tag)));
    
    // dd the device mapper device into a new file
    Command::new("dd")
        .args([
            &format!("if={}", src_dm),
            &format!("of={}", new_image_path.display()),
            "bs=4M",
        ])
        .status()?;
    
    // 3. Write vyoma-config.json for the new image
    let config = vm_manager.get_vm_config(vm_id)?;
    std::fs::write(
        new_image_path.with_extension("json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    
    // 4. Resume VM
    vm_manager.resume_vm(vm_id).await?;
    
    // 5. Register in local image store
    let image_id = register_local_image(tag, &new_image_path)?;
    Ok(image_id)
}
```

#### 3.7.2 `vyoma save <image> -o <file.tar.gz>` and `vyoma load -i <file.tar.gz>`

Bundle the ext4 image file + vyoma-config.json into a compressed tar. This is a simpler version of the existing `vyoma export` / `vyoma import` which operates on VM snapshots.

```rust
// crates/vyoma/src/commands/image.rs

pub async fn cmd_save(image: &str, output: &Path, client: &Client) -> Result<()> {
    let resp = client.get_image_export(image).await?;
    let file = File::create(output)?;
    let gz = GzEncoder::new(file, Compression::default());
    let mut tar = TarBuilder::new(gz);
    
    tar.append_data(&mut resp.ext4_reader(), "rootfs.ext4")?;
    tar.append_data(&mut Cursor::new(&resp.config_json), "vyoma-config.json")?;
    tar.append_data(&mut Cursor::new(&resp.manifest_json), "manifest.json")?;
    
    tar.finish()?;
    println!("Saved {} to {}", image, output.display());
    Ok(())
}

pub async fn cmd_load(input: &Path, client: &Client) -> Result<()> {
    let file = File::open(input)?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);
    
    let resp = client.post_image_import(archive.entries()?).await?;
    println!("Loaded image: {}", resp.tag);
    Ok(())
}
```

---

## 4. Phase 2 — v1.3: Foundation Hardening

**Duration**: 6 weeks  
**Goal**: Refactor the brittle CLI-subprocess internals into proper Rust-native library calls. Build the chaos test framework. Expand the Docker Hub compatibility test matrix.

### 4.1 Storage Refactor — `vyoma-storage` Crate

**Priority**: P1 — `std::process::Command("dmsetup")` error handling is string parsing; brittle.

Extract `vyoma-core/src/storage.rs` into `crates/vyoma-storage/` and replace the CLI subprocess calls with the `devicemapper` Rust crate.

```toml
# crates/vyoma-storage/Cargo.toml
[dependencies]
devicemapper = "0.34"   # Safe Rust bindings for libdevmapper
loopdev = "0.4"         # Safe Rust bindings for loop devices
```

#### 4.1.1 Device Mapper — Replace subprocess calls

```rust
// crates/vyoma-storage/src/dm.rs

use devicemapper::{DmOptions, DmName, DevId, Segment, LinearDev, SnapshotDev};

pub struct DmSnapshots {
    dm: DM,
}

impl DmSnapshots {
    pub fn new() -> Result<Self> {
        Ok(Self { dm: DM::new()? })
    }

    /// Create a snapshot of base_dev, writing changes to cow_dev
    pub fn create_snapshot(
        &self,
        name: &str,
        base_dev: &Path,      // loop device of base ext4
        cow_dev: &Path,       // loop device of sparse COW file
    ) -> Result<DmDevice> {
        let dm_name = DmName::new(name)?;
        
        // Origin target wraps the read-only base
        let origin = LinearDev::setup(
            &self.dm,
            &DmName::new(&format!("{}-origin", name))?,
            None,
            vec![Segment::new(base_dev, Sector(0), device_size(base_dev)?)],
        )?;
        
        // Snapshot combines origin + COW
        let snap = SnapshotDev::setup(
            &self.dm,
            &dm_name,
            None,
            &origin,
            cow_dev,
            true,  // persistent=true
        )?;
        
        Ok(DmDevice { name: name.to_string(), path: snap.path()? })
    }

    pub fn remove_snapshot(&self, name: &str) -> Result<()> {
        self.dm.device_remove(&DevId::Name(DmName::new(name)?), &DmOptions::default())?;
        Ok(())
    }
}
```

#### 4.1.2 Loop Device — Replace losetup subprocess

```rust
// crates/vyoma-storage/src/cow.rs

use loopdev::{LoopControl, LoopDevice};

pub fn attach_loop_device(file: &Path) -> Result<LoopDevice> {
    let control = LoopControl::open()?;
    let dev = control.next_free()?;
    dev.with().read_only(false).attach(file)?;
    Ok(dev)
}

pub fn detach_loop_device(dev: &LoopDevice) -> Result<()> {
    dev.detach()?;
    Ok(())
}

pub fn create_cow_file(path: &Path, size_bytes: u64) -> Result<()> {
    // Create a sparse file (no actual disk allocation until written)
    let file = File::create(path)?;
    file.set_len(size_bytes)?;
    Ok(())
}
```

### 4.2 Network Refactor — `vyoma-net` Crate

Extract `vyoma-core/src/network.rs` into `crates/vyoma-net/`. Replace `ip link`/`brctl`/`iptables` subprocess calls with `rtnetlink`.

```toml
# crates/vyoma-net/Cargo.toml
[dependencies]
rtnetlink = "0.13"
netlink-packet-route = "0.17"
ipnetwork = "0.20"
```

```rust
// crates/vyoma-net/src/bridge.rs

use rtnetlink::{new_connection, Handle};

pub struct BridgeManager {
    handle: Handle,
}

impl BridgeManager {
    pub async fn new() -> Result<Self> {
        let (conn, handle, _) = new_connection()?;
        tokio::spawn(conn);
        Ok(Self { handle })
    }

    pub async fn create_bridge(&self, name: &str) -> Result<u32> {
        self.handle
            .link()
            .add()
            .bridge(name.to_string())
            .execute()
            .await?;
        
        // Get the interface index
        let link = self.get_link_by_name(name).await?;
        
        // Set UP
        self.handle.link().set(link.header.index).up().execute().await?;
        
        Ok(link.header.index)
    }

    pub async fn add_tap_to_bridge(&self, tap_name: &str, bridge_idx: u32) -> Result<()> {
        // Create TAP device
        self.handle
            .link()
            .add()
            .tap(tap_name.to_string())
            .execute()
            .await?;
        
        let tap_link = self.get_link_by_name(tap_name).await?;
        
        // Attach to bridge
        self.handle
            .link()
            .set(tap_link.header.index)
            .controller(bridge_idx)
            .execute()
            .await?;
        
        // Set UP
        self.handle.link().set(tap_link.header.index).up().execute().await?;
        
        Ok(())
    }
}
```

### 4.3 Replace Git-Based Time Travel with Proper Snapshot Tree

**Crate**: `crates/vyoma-storage/src/snapshot_tree.rs`  
**Priority**: P1 — git is an external dep, wrong abstraction, and breaks CoW delta efficiency.

The git-based approach (ADR in the codebase) must be removed and replaced with a proper snapshot graph backed by sled.

#### 4.3.1 Snapshot Tree Data Model

```rust
// crates/vyoma-storage/src/snapshot_tree.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub id: String,           // UUID
    pub vm_id: String,
    pub parent_id: Option<String>,  // None = root snapshot
    pub created_at: u64,            // Unix timestamp
    pub label: Option<String>,      // User-defined e.g. "pre-deploy"
    pub tag: Option<String>,        // e.g. "snap:6"
    pub memory_path: PathBuf,       // Firecracker .mem file
    pub snapshot_path: PathBuf,     // Firecracker .snap file
    pub cow_delta_path: PathBuf,    // COW diff since parent
    pub cow_delta_size: u64,        // Bytes
    pub memory_size: u64,
}

pub struct SnapshotTree {
    db: sled::Tree,
}

impl SnapshotTree {
    /// Create a new snapshot, optionally parenting off another
    pub fn create(&self, node: &SnapshotNode) -> Result<()> {
        let key = node.id.as_bytes().to_vec();
        let value = serde_json::to_vec(node)?;
        self.db.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    /// List all snapshots for a VM in chronological order
    pub fn history(&self, vm_id: &str) -> Result<Vec<SnapshotNode>> {
        let mut nodes: Vec<SnapshotNode> = self.db
            .iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|n: &SnapshotNode| n.vm_id == vm_id)
            .collect();
        
        nodes.sort_by_key(|n| n.created_at);
        Ok(nodes)
    }

    /// Fork a new VM from a historical snapshot (like git checkout -b)
    pub fn branch(&self, snap_id: &str, new_vm_id: &str) -> Result<SnapshotNode> {
        let parent = self.get(snap_id)?;
        
        // Create a new COW layer that reads from the snapshot's state
        // The new VM will start from the snapshot's disk + memory state
        let new_node = SnapshotNode {
            id: uuid::Uuid::new_v4().to_string(),
            vm_id: new_vm_id.to_string(),
            parent_id: Some(snap_id.to_string()),
            created_at: unix_now(),
            label: Some(format!("branched-from-{}", snap_id)),
            ..parent.clone()
        };
        
        self.create(&new_node)?;
        Ok(new_node)
    }

    /// Compute filesystem diff between two snapshots
    pub fn diff(&self, snap_a: &str, snap_b: &str) -> Result<SnapshotDiff> {
        // Mount both COW layers read-only and run a recursive diff
        let a = self.get(snap_a)?;
        let b = self.get(snap_b)?;
        
        // Use debugfs to list changed inodes without mounting
        compute_ext4_diff(&a.cow_delta_path, &b.cow_delta_path)
    }
}
```

### 4.4 Chaos Test Framework

**Location**: `tests/chaos/`  
**Priority**: P1 — required to validate WAL recovery

```rust
// tests/chaos/wal_recovery_test.rs

#[tokio::test]
#[ignore = "requires KVM and root"]
async fn test_recovery_after_sigkill_during_create() {
    let daemon = TestDaemon::start().await;
    
    // Begin VM creation
    let create_handle = tokio::spawn(async {
        daemon.client().create_vm("alpine:latest").await
    });
    
    // Kill daemon immediately after WAL write but before VM boot
    tokio::time::sleep(Duration::from_millis(50)).await;
    daemon.kill_sigkill().await;
    
    // Restart daemon
    let daemon2 = TestDaemon::start_with_existing_data(daemon.data_dir()).await;
    
    // VMs that were mid-creation should be cleaned up (not left in broken state)
    let vms = daemon2.client().list_vms().await.unwrap();
    assert!(vms.is_empty(), "Half-created VM should have been cleaned up");
    
    // No dangling loop devices
    assert!(!daemon2.has_dangling_loop_devices().await);
    // No dangling DM snapshots
    assert!(!daemon2.has_dangling_dm_devices().await);
}

#[tokio::test]
#[ignore = "requires KVM and root"]  
async fn test_running_vm_survives_daemon_restart() {
    let daemon = TestDaemon::start().await;
    let vm_id = daemon.client().run_vm("alpine:latest").await.unwrap();
    
    // Wait for VM to be running
    daemon.wait_for_status(&vm_id, VmStatus::Running).await;
    
    // Gracefully restart the daemon
    daemon.restart().await;
    
    // The running VM should still be there
    let vms = daemon.client().list_vms().await.unwrap();
    assert_eq!(vms.len(), 1);
    assert_eq!(vms[0].id, vm_id);
    assert_eq!(vms[0].status, VmStatus::Running);
}
```

### 4.5 Docker Hub Compatibility Matrix

**Location**: `tests/compat/`  
**Priority**: P2

Build an automated nightly test that pulls the top-N Docker Hub images and verifies:
1. Pull succeeds (OCI → ext4 conversion)
2. `vyoma-config.json` is correctly extracted (CMD/ENTRYPOINT/ENV)
3. VM boots (`vyoma run` returns Running status)
4. Healthcheck passes (if declared in the OCI config)
5. Main process responds (TCP probe on exposed port if declared)

```rust
// tests/compat/docker_hub_matrix.rs

const TEST_IMAGES: &[(&str, &str, Option<u16>)] = &[
    ("nginx:alpine",     "nginx: master process",  Some(80)),
    ("redis:7",          "Ready to accept connections", Some(6379)),
    ("postgres:15",      "database system is ready",   Some(5432)),
    ("ubuntu:22.04",     "",                            None),
    ("alpine:latest",    "",                            None),
    ("python:3.11-slim", "",                            None),
    ("node:20-alpine",   "",                            None),
    ("golang:1.21",      "",                            None),
    ("rust:1.75",        "",                            None),
    ("debian:bookworm",  "",                            None),
    // ... expand to 100+ images
];

#[tokio::test]
#[ignore = "nightly CI only — requires KVM"]
async fn test_docker_hub_compatibility() {
    let mut results = vec![];
    
    for (image, expected_log, port) in TEST_IMAGES {
        let result = test_single_image(image, expected_log, *port).await;
        results.push(CompatResult { image, result });
    }
    
    let pass_count = results.iter().filter(|r| r.result.is_ok()).count();
    let total = results.len();
    let pass_rate = pass_count as f64 / total as f64;
    
    println!("Compat matrix: {}/{} passed ({:.1}%)", pass_count, total, pass_rate * 100.0);
    
    // Assert 98% pass rate
    assert!(pass_rate >= 0.98, "Compat rate below 98%: {:.1}%", pass_rate * 100.0);
}
```

---

## 5. Phase 3 — v1.5: Power Features

**Duration**: 12 weeks  
**Goal**: WireGuard-encrypted Swarm, Raft consensus, live VM migration (Teleport), Vyoma Hub, gRPC interface, Prometheus metrics.

### 5.1 WireGuard Encryption in Swarm — `vyoma-net` Crate

**Priority**: P0 for multi-tenant Swarm — all VXLAN traffic is currently plaintext.

Use `boringtun` (pure Rust WireGuard implementation) embedded in vyomad. No external `wg` binary dependency.

```toml
# crates/vyoma-net/Cargo.toml
[dependencies]
boringtun = "0.6"
```

#### 5.1.1 WireGuard Integration

```rust
// crates/vyoma-net/src/wireguard.rs

use boringtun::crypto::{X25519PublicKey, X25519SecretKey};
use boringtun::device::drop_privileges;
use boringtun::device::{DeviceConfig, DeviceHandle};

pub struct WireGuardNode {
    secret_key: X25519SecretKey,
    public_key: X25519PublicKey,
    handle: DeviceHandle,
}

impl WireGuardNode {
    /// Called on `vyoma swarm init` or `vyoma swarm join` — generates keypair,
    /// creates a WireGuard interface, and listens for peer configurations.
    pub fn new(listen_port: u16) -> Result<Self> {
        let secret_key = X25519SecretKey::new();
        let public_key = secret_key.public_key();
        
        let config = DeviceConfig {
            n_threads: 2,
            use_connected_socket: true,
            ..Default::default()
        };
        
        let handle = DeviceHandle::new("vyoma-wg0", config)?;
        
        Ok(Self { secret_key, public_key, handle })
    }

    pub fn public_key_base64(&self) -> String {
        base64::encode(self.public_key.as_bytes())
    }

    /// Add a peer (called when a new Swarm node joins)
    pub fn add_peer(&self, public_key_b64: &str, endpoint: SocketAddr, allowed_ips: &[IpNetwork]) -> Result<()> {
        let pk_bytes = base64::decode(public_key_b64)?;
        let pk = X25519PublicKey::from(pk_bytes.as_slice());
        
        self.handle.add_peer(
            pk,
            Some(endpoint),
            allowed_ips,
            None,           // preshared_key
            Some(25),       // keepalive seconds
        )?;
        
        Ok(())
    }
}
```

#### 5.1.2 Swarm Init/Join Key Exchange

When `vyoma swarm init` is called:
1. Generate WireGuard keypair, store in `/var/lib/vyoma/wg.key`
2. Start listening on UDP port 51820
3. Advertise public key + endpoint in the swarm gossip state

When `vyoma swarm join <seed-ip>` is called:
1. Generate WireGuard keypair
2. POST to seed node's `/api/v1/swarm/join` with `{ public_key, endpoint, subnet_lease_request }`
3. Seed responds with its public key + all existing peer public keys
4. Both nodes call `add_peer()` on each other
5. VXLAN traffic flows inside WireGuard tunnel

### 5.2 Raft Consensus for Swarm — Replace Seed Node Model

**Crate**: `crates/vyomad/src/swarm/`  
**Priority**: P1

Replace the current seed-based approach with Raft via the `openraft` crate.

```toml
# crates/vyomad/Cargo.toml
[dependencies]
openraft = { version = "0.9", features = ["serde"] }
```

```rust
// crates/vyomad/src/swarm/raft.rs

use openraft::{Config, Raft, RaftMetrics};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmCommand {
    RegisterNode { node_id: u64, ip: String, public_key: String, subnet: String },
    DeregisterNode { node_id: u64 },
    UpdateVmPlacement { vm_id: String, node_id: u64 },
    RemoveVmPlacement { vm_id: String },
    CreateService { name: String, spec: ServiceSpec },
    UpdateService { name: String, spec: ServiceSpec },
    DeleteService { name: String },
}

pub type VyomaRaft = Raft<VyomaTypeConfig>;

pub async fn create_raft_node(
    node_id: u64,
    config: Arc<Config>,
    network: Arc<VyomaNetwork>,
    storage: Arc<VyomaStorage>,
) -> Result<VyomaRaft> {
    let raft = Raft::new(node_id, config, network, storage).await?;
    Ok(raft)
}

/// Called on `vyoma swarm init` — bootstrap a single-node cluster
pub async fn bootstrap_cluster(raft: &VyomaRaft, node_id: u64, addr: String) -> Result<()> {
    let members = BTreeMap::from([(node_id, BasicNode { addr })]);
    raft.initialize(members).await?;
    Ok(())
}

/// Called on `vyoma swarm join` — add this node to existing cluster
pub async fn join_cluster(
    raft: &VyomaRaft,
    leader_addr: &str,
    node_id: u64,
    my_addr: String,
) -> Result<()> {
    // Contact the leader and request to be added
    let client = SwarmClient::new(leader_addr);
    client.add_learner(node_id, my_addr.clone()).await?;
    client.change_membership(node_id).await?;
    Ok(())
}
```

### 5.3 Teleport — Live VM Migration

**Crate**: `crates/vyoma-teleport/`  
**Priority**: P2 — flagship feature

#### 5.3.1 Protocol Overview

Pre-copy memory migration protocol:

```
Source Node                              Destination Node
-----------                              ----------------
1. Mark all memory pages as "dirty"      1. Allocate memory buffer
   via KVM_GET_DIRTY_LOG ioctl
   
2. Bulk copy ALL pages over WireGuard    2. Write pages to buffer
   overlay network
   
3. Repeat: copy only dirty pages         3. Receive dirty pages
   (pages written since last round)      
   Iterate until dirty rate < threshold
   
4. Pause VM (VMCL: pause call to         4. Receive final delta
   Firecracker /pause)                   
   
5. Copy final dirty pages + CPU state   5. Reconstruct VM memory
   (Firecracker snapshot files)         
   
6. Notify destination: "start VM"       6. Load snapshot into new
                                           Firecracker instance
   
7. Update overlay network routing       7. VM resumes
   (VM IP now routes to dest node)

8. Destroy source VM
```

#### 5.3.2 Source Node Implementation

```rust
// crates/vyoma-teleport/src/sender.rs

use kvm_ioctls::{Kvm, VmFd};

pub struct MigrationSender {
    vm_fd: VmFd,
    fc_client: FirecrackerClient,
    wg_stream: TcpStream,   // WireGuard-encrypted stream to destination
}

impl MigrationSender {
    pub async fn migrate(
        mut self,
        vm_id: &str,
        dest_addr: SocketAddr,
    ) -> Result<MigrationStats> {
        let total_pages = self.vm_fd.get_num_pages()?;
        let page_size = 4096u64;
        
        // Phase 1: Enable dirty tracking
        self.vm_fd.enable_dirty_log()?;
        
        // Phase 2: Initial bulk transfer
        let initial_dirty = self.get_all_pages()?;
        self.send_pages(&initial_dirty).await?;
        
        // Phase 3: Iterative refinement
        let mut round = 0;
        loop {
            let dirty = self.get_dirty_pages()?;
            let dirty_count = dirty.count_ones() as u64;
            
            tracing::debug!("Migration round {}: {} dirty pages", round, dirty_count);
            
            if dirty_count < MIGRATION_THRESHOLD_PAGES {
                break;  // Dirty rate low enough to do final pause
            }
            
            self.send_pages_by_bitmap(&dirty).await?;
            round += 1;
        }
        
        // Phase 4: Final pause + snapshot
        self.fc_client.pause_vm().await?;
        
        // Send final dirty pages
        let final_dirty = self.get_dirty_pages()?;
        self.send_pages_by_bitmap(&final_dirty).await?;
        
        // Send Firecracker snapshot (CPU state)
        let snap = self.fc_client.create_snapshot().await?;
        self.send_snapshot(snap).await?;
        
        // Signal destination to resume
        self.send_signal(MigrationSignal::Resume).await?;
        
        // Update routing in Swarm overlay
        update_vm_routing(vm_id, dest_addr).await?;
        
        Ok(MigrationStats { rounds: round + 1, total_pages })
    }
    
    fn get_dirty_pages(&self) -> Result<BitVec> {
        // KVM_GET_DIRTY_LOG ioctl
        self.vm_fd.get_dirty_log(0, todo!("slot size"))
            .map(|bitmap| BitVec::from_vec(bitmap))
            .map_err(Into::into)
    }
}
```

### 5.4 gRPC Interface — `vyoma-proto` Crate

**Priority**: P1 — required for vk8s CRI (Phase 4)

```protobuf
// crates/vyoma-proto/proto/vm.proto
syntax = "proto3";
package vyoma.v1;

service VmService {
    rpc CreateVm (CreateVmRequest) returns (CreateVmResponse);
    rpc StartVm  (VmIdRequest) returns (VmStatusResponse);
    rpc StopVm   (VmIdRequest) returns (VmStatusResponse);
    rpc DeleteVm (VmIdRequest) returns (google.protobuf.Empty);
    rpc ListVms  (ListVmsRequest) returns (ListVmsResponse);
    rpc GetVm    (VmIdRequest) returns (VmInfo);
    rpc ExecCommand (ExecRequest) returns (stream ExecOutput);
    rpc StreamLogs  (LogRequest) returns (stream LogLine);
    rpc CreateSnapshot (SnapshotRequest) returns (SnapshotInfo);
    rpc RestoreSnapshot (RestoreRequest) returns (VmInfo);
    rpc MigrateVm (MigrateRequest) returns (stream MigrationProgress);
}

message VmInfo {
    string id = 1;
    string image = 2;
    string status = 3;
    string ip = 4;
    uint32 vcpus = 5;
    uint64 memory_mb = 6;
    repeated PortMapping ports = 7;
    int64 created_at = 8;
}
```

```toml
# crates/vyoma-proto/Cargo.toml
[dependencies]
tonic = "0.11"
prost = "0.12"
[build-dependencies]
tonic-build = "0.11"
```

Add gRPC server alongside the existing axum REST server in vyomad:

```rust
// crates/vyomad/src/main.rs

#[tokio::main]
async fn main() -> Result<()> {
    let state = Arc::new(DaemonState::new().await?);
    
    // REST API on Unix socket (for ign CLI)
    let rest_server = start_rest_server(state.clone());
    
    // gRPC on TCP (for vk8s CRI plugin, SDK)
    let grpc_server = Server::builder()
        .add_service(VmServiceServer::new(GrpcVmService::new(state.clone())))
        .serve("[::1]:7071".parse()?);
    
    tokio::select! {
        _ = rest_server => {},
        _ = grpc_server => {},
    }
    
    Ok(())
}
```

### 5.5 Prometheus Metrics Endpoint

**Priority**: P2

```rust
// crates/vyomad/src/metrics.rs

use prometheus::{Registry, Gauge, Counter, Histogram, GaugeVec};

pub struct VyomaMetrics {
    pub vms_running:      Gauge,
    pub vms_total:        Counter,
    pub vm_boot_duration: Histogram,
    pub vm_memory_usage:  GaugeVec,  // labels: vm_id
    pub vm_cpu_usage:     GaugeVec,  // labels: vm_id
    pub snapshot_count:   GaugeVec,  // labels: vm_id
}

// Expose at GET /metrics in axum router
async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        buffer,
    )
}
```

### 5.6 VMIF Image Format — `vyoma-image` Crate

**Priority**: P2

```rust
// crates/vyoma-image/src/vmif.rs

/// VMIF (VM Image Format) — the stable on-disk format for Vyoma images
/// Layout (OCI-compatible artifact stored in any OCI registry):
///   vyoma.toml  — image metadata
///   rootfs.sqfs  — squashfs root filesystem (read-only, compressed)
///   kernel.vmlinuz — guest kernel (optional, uses bundled default if absent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmifManifest {
    pub schema_version: u32,        // 1
    pub created: String,            // RFC3339 timestamp
    pub arch: String,               // "amd64", "arm64"
    pub kernel: Option<String>,     // OCI digest of kernel layer
    pub rootfs: String,             // OCI digest of rootfs layer
    pub config: OciImageConfig,     // CMD, ENTRYPOINT, ENV, etc.
    pub labels: HashMap<String, String>,
    pub size_bytes: u64,            // Uncompressed rootfs size
}
```

#### 5.6.1 Docker Hub → VMIF Bridge

```rust
// crates/vyoma-image/src/hub_bridge.rs

/// Convert a Docker Hub OCI image to VMIF format.
/// This is the core of the "Vyoma Hub bridge" feature.
/// Called once per image tag; result cached forever.
pub async fn convert_docker_hub_to_vmif(
    image_ref: &str,
    kernel_ref: Option<&str>,
) -> Result<VmifManifest> {
    // 1. Pull OCI layers from Docker Hub
    let oci_client = OciClient::new();
    let (manifest, config, layers) = oci_client.pull_all(image_ref).await?;
    
    // 2. Unpack layers into staging directory (existing layer flattening logic)
    let staging_dir = temp_dir();
    unpack_layers(&layers, &staging_dir).await?;
    
    // 3. Convert ext4 → squashfs for better compression + read-only semantics
    let sqfs_path = staging_dir.join("rootfs.sqfs");
    Command::new("mksquashfs")
        .args([staging_dir.to_str().unwrap(), sqfs_path.to_str().unwrap(),
               "-comp", "zstd", "-Xcompression-level", "9"])
        .status()?;
    
    // 4. Build vyoma.toml metadata
    let vmif = VmifManifest {
        schema_version: 1,
        created: chrono::Utc::now().to_rfc3339(),
        arch: "amd64".to_string(),
        kernel: kernel_ref.map(str::to_string),
        rootfs: sha256_of_file(&sqfs_path)?,
        config: parse_oci_config(&config)?,
        labels: manifest.annotations.unwrap_or_default(),
        size_bytes: file_size(&sqfs_path)?,
    };
    
    Ok(vmif)
}
```

### 5.7 Vyoma Studio v2 — Enhanced Dashboard

**Location**: `ui/src/`  
**Priority**: P2

Extend the existing TypeScript dashboard. Add these views:

1. **TimeMachine View** — horizontal scrollable snapshot timeline per VM. Each snapshot is a node. Click to preview metadata. Drag two nodes for diff view. Button to restore.

2. **Network Topology View** — D3.js force-directed graph. Nodes = VMs, edges = network connections. Color by compose stack. Click a VM for inline stats panel.

3. **Compose Editor** — Monaco editor (same as VS Code) with YAML schema validation for `vyoma-compose.yml`. Live validate against the Docker Compose v3 JSON schema. One-click deploy button.

4. **Hub Browser** — Search box. Hit Vyoma Hub API (local cache) first, fall back to Docker Hub bridge. Shows conversion status badge.

---

## 6. Phase 4 — v2.0: Revolutionary Features

**Duration**: 16 weeks  
**Goal**: TimeMachine (full git-for-runtime), Hibernation, Kubernetes CRI (vk8s), Trusted Boot, in-VM agent, SDK.

### 6.1 TimeMachine — Full Implementation

**Crate**: `crates/vyoma-storage/src/snapshot_tree.rs` (extends Phase 2 foundation)  
**Priority**: P1 — headline v2.0 feature

The snapshot tree is already built in Phase 2. This phase wires it to the CLI commands.

#### 6.1.1 `vyoma history <vm-id>`

```rust
// crates/vyoma/src/commands/snapshot.rs

pub async fn cmd_history(vm_id: &str, client: &Client) -> Result<()> {
    let history = client.get_snapshot_history(vm_id).await?;
    
    println!("{:<6}  {:<20}  {:<10}  {:<20}  {}", 
             "TAG", "ID", "DELTA", "CREATED", "LABEL");
    
    for (i, snap) in history.iter().enumerate() {
        println!("{:<6}  {:<20}  {:<10}  {:<20}  {}",
            format!("snap:{}", history.len() - i - 1),
            &snap.id[..8],
            human_bytes(snap.cow_delta_size),
            format_relative_time(snap.created_at),
            snap.label.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}
```

#### 6.1.2 `vyoma time-travel <vm-id> --to snap:N`

```rust
pub async fn cmd_time_travel(vm_id: &str, target: &str, client: &Client) -> Result<()> {
    // Parse "snap:N" to get the snapshot index
    let index: usize = target.strip_prefix("snap:")
        .ok_or_else(|| anyhow!("Invalid snapshot ref. Use snap:N"))?
        .parse()?;
    
    let history = client.get_snapshot_history(vm_id).await?;
    let snap = history.get(history.len() - 1 - index)
        .ok_or_else(|| anyhow!("Snapshot snap:{} not found", index))?;
    
    println!("Stopping {} and restoring to {}...", vm_id, snap.label.as_deref().unwrap_or(&snap.id));
    
    client.stop_vm(vm_id).await?;
    client.restore_snapshot_to_vm(vm_id, &snap.id).await?;
    client.start_vm(vm_id).await?;
    
    println!("✓ Time-traveled to snap:{}", index);
    Ok(())
}
```

#### 6.1.3 Auto-Snapshot Policy (from Vyomafile)

```rust
// crates/vyomad/src/vm_manager.rs

pub struct AutoSnapshotTask {
    vm_id: String,
    interval: Duration,
    retain_count: usize,
}

impl AutoSnapshotTask {
    /// Spawned as a tokio task when a VM starts with VM_SNAPSHOT_POLICY set
    pub async fn run(self, wal: Arc<Wal>, tree: Arc<SnapshotTree>) {
        let mut interval_timer = tokio::time::interval(self.interval);
        interval_timer.tick().await; // Skip first immediate tick
        
        loop {
            interval_timer.tick().await;
            
            match take_snapshot(&self.vm_id, None, &wal).await {
                Ok(snap) => {
                    tracing::info!("Auto-snapshot {} for VM {}", snap.id, self.vm_id);
                    prune_old_snapshots(&self.vm_id, self.retain_count, &tree).await;
                }
                Err(e) => tracing::warn!("Auto-snapshot failed for {}: {}", self.vm_id, e),
            }
        }
    }
}
```

### 6.2 Hibernation — State-to-Disk, All Resources Released

**Crate**: `crates/vyomad/src/vm_manager.rs`  
**Priority**: P1

```rust
pub async fn hibernate_vm(&self, vm_id: &str) -> Result<HibernationInfo> {
    let state = self.get_running_vm(vm_id)?;
    
    // 1. Firecracker snapshot (CPU + memory state to files)
    let hib_dir = hibernation_dir().join(vm_id);
    std::fs::create_dir_all(&hib_dir)?;
    
    self.fc_client.pause_vm(&state.fc_socket).await?;
    self.fc_client.create_snapshot(
        &state.fc_socket,
        &hib_dir.join("vm.snap"),
        &hib_dir.join("vm.mem"),
        SnapshotType::Full,
    ).await?;
    
    // 2. Stop the Firecracker process (releases vCPUs and memory)
    let _ = state.fc_process.kill();
    state.fc_process.wait()?;
    
    // 3. Detach TAP device (releases network slot)
    //    Keep the TAP device OBJECT so we can re-attach; but disable it
    Command::new("ip").args(["link", "set", &state.tap_device, "down"]).status()?;
    
    // 4. Release IP back to IPAM (optional — keep IP for fast resume)
    //    For hibernation we KEEP the IP reserved so VM resumes with same address
    
    // 5. Update WAL state
    self.wal.commit_state(vm_id, &VmState {
        status: VmStatus::Hibernated {
            hib_dir: hib_dir.clone(),
            snap_path: hib_dir.join("vm.snap"),
            mem_path: hib_dir.join("vm.mem"),
        },
        ..state.clone()
    })?;
    
    // 6. Remove from in-memory VM map (resources truly freed)
    self.vms.lock().await.remove(vm_id);
    
    Ok(HibernationInfo {
        vm_id: vm_id.to_string(),
        hib_dir,
        preserved_ip: state.ip,
    })
}

pub async fn resume_vm_from_hibernation(&self, vm_id: &str) -> Result<()> {
    let state = self.wal.get_state(vm_id)?;
    
    let VmStatus::Hibernated { snap_path, mem_path, .. } = &state.status else {
        return Err(anyhow!("VM {} is not hibernated", vm_id));
    };
    
    // 1. Re-enable TAP device
    Command::new("ip").args(["link", "set", &state.tap_device, "up"]).status()?;
    
    // 2. Start new Firecracker process
    let fc_socket = new_fc_socket_path(vm_id);
    let fc_process = spawn_firecracker(vm_id, &fc_socket)?;
    
    // 3. Load snapshot (Firecracker resumes from exact state)
    self.fc_client.load_snapshot(
        &fc_socket,
        snap_path,
        mem_path,
        &state.tap_device,
        &state.dm_device_path,
    ).await?;
    
    // 4. Resume execution
    self.fc_client.resume_vm(&fc_socket).await?;
    
    // 5. Update WAL
    self.wal.commit_state(vm_id, &VmState {
        status: VmStatus::Running { pid: fc_process.id(), fc_socket },
        ..state
    })?;
    
    self.vms.lock().await.insert(vm_id.to_string(), Arc::new(fc_process));
    
    Ok(())
}
```

### 6.3 vk8s — Kubernetes CRI Plugin

**Location**: `vk8s/`  
**Language**: Go (CRI spec is Go-native, generated from protobuf)  
**Priority**: P2

```go
// vk8s/pkg/cri/runtime.go

package cri

import (
    pb "k8s.io/cri-api/pkg/apis/runtime/v1"
    vyoma "github.com/Subeshrock/micro-vm-ecosystem/sdk/go"
)

// VyomaCriServer implements the CRI RuntimeService and ImageService
type VyomaCriServer struct {
    client *vyoma.Client  // gRPC client to vyomad
    pb.UnimplementedRuntimeServiceServer
    pb.UnimplementedImageServiceServer
}

// RunPodSandbox — called when kubelet creates a new Pod
// Each Pod = one Vyoma MicroVM
func (s *VyomaCriServer) RunPodSandbox(
    ctx context.Context,
    req *pb.RunPodSandboxRequest,
) (*pb.RunPodSandboxResponse, error) {
    
    config := req.Config
    
    // Create VM configuration from pod spec
    vmConfig := &vyoma.CreateVmRequest{
        Name:       config.Metadata.Name,
        Namespace:  config.Metadata.Namespace,
        Vcpus:      uint32(config.Linux.Resources.CpuQuota / 100000),
        MemoryMb:   uint64(config.Linux.Resources.MemoryLimitInBytes / 1024 / 1024),
        Labels: map[string]string{
            "k8s.io/pod-name":       config.Metadata.Name,
            "k8s.io/pod-namespace":  config.Metadata.Namespace,
            "k8s.io/pod-uid":        config.Metadata.Uid,
        },
    }
    
    resp, err := s.client.CreateVm(ctx, vmConfig)
    if err != nil {
        return nil, status.Errorf(codes.Internal, "failed to create VM: %v", err)
    }
    
    return &pb.RunPodSandboxResponse{PodSandboxId: resp.VmId}, nil
}

// CreateContainer — called for each container in a pod
// Containers within a pod share the same VM via namespaces
func (s *VyomaCriServer) CreateContainer(
    ctx context.Context,
    req *pb.CreateContainerRequest,
) (*pb.CreateContainerResponse, error) {
    // The VM is already running (from RunPodSandbox).
    // "Containers" within a pod are processes inside the VM.
    // We use `vyoma exec` semantics to run the container command.
    
    vm_id := req.PodSandboxId
    cmd := append(req.Config.Command, req.Config.Args...)
    
    execReq := &vyoma.ExecRequest{
        VmId:    vm_id,
        Command: cmd,
        Env:     req.Config.Envs,
        WorkDir: req.Config.WorkingDir,
    }
    
    execResp, err := s.client.StartExec(ctx, execReq)
    // Returns a container ID = "vmid/process_id"
    return &pb.CreateContainerResponse{ContainerId: execResp.ExecId}, nil
}
```

```bash
# Kubernetes setup
# kubelet config (containerd-style):
containerRuntimeEndpoint: unix:///var/run/vyoma-cri.sock

# Pod spec to use Vyoma MicroVM isolation:
spec:
  runtimeClassName: vyoma-microvm
  containers:
    - name: web
      image: nginx:alpine
      ports:
        - containerPort: 80
```

### 6.4 Trusted Boot Chain

**Crate**: `crates/vyomad/src/` and `crates/vyoma-image/src/signing.rs`  
**Priority**: P3

```rust
// crates/vyoma-image/src/signing.rs

use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use sha2::{Sha256, Digest};

/// Sign a VMIF manifest with an Ed25519 key
pub fn sign_manifest(manifest: &VmifManifest, key: &SigningKey) -> Result<SignedManifest> {
    let manifest_bytes = serde_json::to_vec(manifest)?;
    let signature = key.sign(&manifest_bytes);
    
    Ok(SignedManifest {
        manifest: manifest.clone(),
        signature: signature.to_bytes().to_vec(),
        public_key: key.verifying_key().to_bytes().to_vec(),
    })
}

/// Verify a signed VMIF manifest before booting
pub fn verify_manifest(signed: &SignedManifest, trusted_key: &VerifyingKey) -> Result<()> {
    let manifest_bytes = serde_json::to_vec(&signed.manifest)?;
    let signature = Signature::from_bytes(&signed.signature.as_slice().try_into()?);
    
    trusted_key.verify(&manifest_bytes, &signature)
        .map_err(|e| anyhow!("Image signature verification failed: {}", e))
}
```

```ini
# Enforce signature policy via vyomad config
# /etc/vyoma/config.toml

[security]
require_signed_images = true
trusted_keys = [
    "/etc/vyoma/trusted-keys/ci.pub",
    "/etc/vyoma/trusted-keys/hub.pub",
]
```

### 6.5 vyoma-agent — In-VM Binary

**Crate**: `crates/vyoma-agent/`  
**Target**: `x86_64-unknown-linux-musl` — static binary, .vyoma400KB  
**Priority**: P2

```rust
// crates/vyoma-agent/src/main.rs
// This binary is injected into every VMIF image at build time.
// It runs as PID 2 (alongside the actual workload) via a wrapper init.

use vsock::{VsockListener, VMADDR_CID_HOST};

const VSOCK_PORT: u32 = 9999;

#[tokio::main]
async fn main() -> Result<()> {
    // Listen on vsock for daemon communication
    let listener = VsockListener::bind(&VsockAddr::new(VMADDR_CID_HOST, VSOCK_PORT))?;
    
    loop {
        let (stream, _) = listener.accept()?;
        tokio::spawn(handle_connection(stream));
    }
}

async fn handle_connection(stream: VsockStream) -> Result<()> {
    let mut framed = LengthDelimitedCodec::new().framed(stream);
    
    while let Some(frame) = framed.next().await {
        let request: AgentRequest = serde_json::from_slice(&frame?)?;
        
        let response = match request {
            AgentRequest::ProcessList => {
                AgentResponse::ProcessList(collect_process_tree().await?)
            }
            AgentRequest::ExecCommand { cmd, env, workdir } => {
                AgentResponse::ExecStarted(exec_command(cmd, env, workdir).await?)
            }
            AgentRequest::GetMetrics => {
                AgentResponse::Metrics(collect_metrics().await?)
            }
            AgentRequest::FileRead { path } => {
                AgentResponse::FileContent(std::fs::read(&path)?)
            }
        };
        
        framed.send(Bytes::from(serde_json::to_vec(&response)?)).await?;
    }
    
    Ok(())
}

/// Collect /proc stats and return structured metrics
async fn collect_metrics() -> Result<VmMetrics> {
    Ok(VmMetrics {
        cpu_user_ms:   read_proc_stat()?.user_time,
        cpu_system_ms: read_proc_stat()?.system_time,
        mem_used_kb:   read_proc_meminfo()?.mem_total - read_proc_meminfo()?.mem_free,
        mem_total_kb:  read_proc_meminfo()?.mem_total,
        load_avg_1:    read_loadavg()?.one,
        process_count: read_proc_count()?,
    })
}
```

### 6.6 SDK — Go, Rust, Python

**Priority**: P3 — enables ecosystem growth

The Go SDK is auto-generated from the protobuf definitions in `vyoma-proto`. The Rust and Python SDKs are thin ergonomic wrappers.

```go
// sdk/go/client.go — Auto-generated from proto, then wrap with ergonomic API

package vyoma

import (
    "google.golang.org/grpc"
    pb "github.com/Subeshrock/micro-vm-ecosystem/vyoma-proto/gen/go"
)

type Client struct {
    conn *grpc.ClientConn
    vm   pb.VmServiceClient
}

func NewClient(addr string) (*Client, error) {
    conn, err := grpc.Dial(addr, grpc.WithInsecure())
    if err != nil { return nil, err }
    return &Client{conn: conn, vm: pb.NewVmServiceClient(conn)}, nil
}

func (c *Client) Run(ctx context.Context, image string, opts ...RunOption) (*Vm, error) {
    cfg := &RunConfig{Image: image, Vcpus: 1, MemoryMb: 512}
    for _, o := range opts { o(cfg) }
    
    resp, err := c.vm.CreateVm(ctx, &pb.CreateVmRequest{
        Image:    cfg.Image,
        Vcpus:    cfg.Vcpus,
        MemoryMb: cfg.MemoryMb,
    })
    if err != nil { return nil, err }
    return &Vm{Id: resp.VmId, client: c}, nil
}
```

---

## 7. Cross-Cutting: Testing Strategy

### 7.1 Test Pyramid

| Level | Crate | Command | CI Trigger | KVM Required |
|-------|-------|---------|------------|--------------|
| Unit | all | `cargo test` | Every PR | No |
| Component (mocked) | vyomad, vyoma-core | `cargo test -p vyomad` | Every PR | No |
| Integration (real FC) | tests/integration | `./scripts/test_integration.sh` | Every merge to main | Yes |
| Chaos | tests/chaos | `cargo test --test chaos -- --ignored` | Nightly | Yes |
| Compat matrix | tests/compat | `cargo test --test compat -- --ignored` | Nightly | Yes |
| Performance | tests/bench | `cargo bench` | Weekly | Yes |

### 7.2 Performance Benchmark Targets

| Metric | Target | Regression Alert |
|--------|--------|-----------------|
| Cold boot time (alpine:latest) | < 150ms | > 200ms |
| VM creation (DM snapshot) | < 10ms | > 50ms |
| Memory overhead (idle VM) | < 8MB | > 20MB |
| 100 concurrent idle VMs memory | < 800MB total | > 2GB |
| Block I/O overhead vs bare dm-dev | < 3% | > 10% |
| Network throughput overhead | < 2% | > 5% |
| Teleport downtime (512MB VM) | < 100ms | > 500ms |
| Hibernate + resume cycle | < 200ms | > 500ms |

### 7.3 Mock Firecracker Server

For unit tests that test vyomad behavior without KVM:

```rust
// tests/mocks/firecracker_server.rs

pub struct MockFirecracker {
    addr: SocketAddr,
    state: Arc<Mutex<MockVmState>>,
}

impl MockFirecracker {
    pub async fn start() -> Self {
        let state = Arc::new(Mutex::new(MockVmState::default()));
        let app = Router::new()
            .route("/", axum::routing::put(mock_machine_config))
            .route("/boot-source", axum::routing::put(mock_boot_source))
            .route("/drives/:id", axum::routing::put(mock_drive))
            .route("/network-interfaces/:id", axum::routing::put(mock_net))
            .route("/actions", axum::routing::put(mock_actions))
            .route("/snapshot/create", axum::routing::put(mock_snapshot_create))
            .route("/snapshot/load", axum::routing::put(mock_snapshot_load))
            .with_state(state.clone());
        
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app));
        
        Self { addr, state }
    }
}
```

---

## 8. Cross-Cutting: Packaging & Distribution

### 8.1 .deb Package Contents

The `.deb` produced by CI must contain:

```
/usr/bin/vyomad              # Daemon binary
/usr/bin/vyoma                # CLI binary
/usr/lib/vyoma/firecracker   # Bundled Firecracker VMM
/usr/lib/vyoma/virtiofsd     # Bundled virtiofs daemon (Phase 1)
/usr/lib/vyoma/kernels/      # Pre-built minimal kernels (Phase 3)
    vyoma-6.1-slim.vmlinuz
    vyoma-6.1-io_uring.vmlinuz
/etc/systemd/system/vyomad.service
/var/lib/vyoma/              # Runtime state directory (created by postinstall)
/var/log/vyoma/              # Log directory
```

### 8.2 systemd Service Versions

| Version | User | Capabilities |
|---------|------|-------------|
| v1.1 (current) | root | ALL |
| v1.2 (Phase 1) | vyoma | CAP_NET_ADMIN, CAP_SYS_ADMIN, CAP_NET_RAW, CAP_SETUID, CAP_SETGID |
| v2.0 (target) | vyoma | Same — no regression in capability set |

### 8.3 GitHub Actions CI Workflow

```yaml
# .github/workflows/ci.yml

name: CI

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test
      - run: cargo clippy -- -D warnings
      - run: cargo fmt --check

  integration-tests:
    runs-on: [self-hosted, kvm]   # Bare-metal runner with KVM access
    needs: unit-tests
    steps:
      - uses: actions/checkout@v4
      - run: cargo build --release
      - run: sudo ./scripts/test_integration.sh

  nightly-compat:
    runs-on: [self-hosted, kvm]
    if: github.event_name == 'schedule'
    steps:
      - uses: actions/checkout@v4
      - run: cargo build --release
      - run: cargo test --test compat -- --ignored
```

---

## 9. Deprecated Decisions & Migration Guide

This section documents every decision in the existing codebase that must be changed, and the exact migration path.

### 9.1 Git-Based Time Travel (Remove Entirely)

**Current**: The daemon calls `git init`, `git add`, `git commit` on snapshot directories. Lives in `vyomad/src/` and uses `std::process::Command("git")`.

**Remove**: All `git` calls from the daemon. Delete any `.git` directories in `.vyoma/vms/`.

**Replace with**: The `SnapshotTree` implemented in `vyoma-storage/src/snapshot_tree.rs` (Phase 2). The sled-backed snapshot tree provides all the same functionality (history, branching, time-travel) without the external git binary dependency and with proper CoW delta semantics.

**Migration for existing snapshots**: Write a one-time migration script that reads existing git history and converts it to sled snapshot tree entries. Run on first daemon startup after upgrade.

### 9.2 In-Memory VM State HashMap (Replace with WAL-backed Store)

**Current**: `Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<VmmManager>>>>>` in vyomad. ADR-008 acknowledged this loses state on restart.

**Replace with**: WAL + sled store from Phase 1 (Section 3.4). The in-memory HashMap becomes a cache of the WAL-persisted state. On startup, WAL replay rebuilds the HashMap.

**Migration**: Non-breaking. The new store is additive. Existing state JSON files in `.vyoma/state/` can be imported as the initial WAL state on first upgrade.

### 9.3 `std::process::Command` for dmsetup/losetup (Migrate Progressively)

**Current**: ADR-002 chose CLI-subprocess wrapping as MVP approach with explicit note to migrate to native crates for production.

**Migration path**:
- Phase 2: Migrate `storage.rs` to `devicemapper` + `loopdev` crates in `vyoma-storage` crate.
- Phase 2: Migrate `network.rs` to `rtnetlink` crate in `vyoma-net` crate.
- Keep `iptables` subprocess calls for now (the `iptables` Rust crate is less mature).
- Do NOT migrate `debugfs` calls — they're already the correct approach for rootless file population.

### 9.4 Compose Schema `version: "1.0"` (Supersede with Docker Compose v3)

**Current**: Custom `version: "1.0"` YAML schema in `vyoma-compose`.

**Migration**: The new parser (Phase 1, Section 3.5) accepts both `version: "1.0"` (old) and `version: "3.x"` (new) by branching in the deserialization path. Old files continue working. New documentation always shows Docker Compose v3 format.

### 9.5 Rootless Mode (Demoted — Do Not Restore)

**Current**: ADR-019 explicitly demoted rootless mode to "Experimental/Alpha" because:
- Device Mapper (instant clones) requires root
- VXLAN overlay (Swarm) requires root
- User namespace restrictions cause crashes in standard shells

**Decision stands**: Do NOT reintroduce rootless mode as a first-class feature in v1.2-v2.0. The privileged service model with a constrained `vyoma` user (Phase 1, Section 3.3) is the correct security model — same as Docker.

Mark rootless in docs as "not recommended for production" and remove it from the test matrix to reduce CI surface area.

### 9.6 Custom OCI Client (Keep — Improve Error Handling)

**Current**: ADR-003 chose a custom `reqwest`-based OCI client instead of `oci-distribution` crate.

**Decision stands**: Keep the custom implementation. The `oci-distribution` crate issues (v0.9.4 compatibility) were valid. Our client handles the OCI Index vs Docker V2 Manifest distinction correctly.

**Improvements needed** (Phase 2):
- Replace string-based error messages with typed `OciError` enum
- Add retry logic with exponential backoff for transient 429/503 from Docker Hub
- Add support for `.vyoma/.docker/config.json` auth (already partially in changelog — verify completeness)
- Add `Bearer` token caching to avoid re-authenticating on every layer pull

### 9.7 Userspace TCP Port Proxy (Keep — Minor Improvement)

**Current**: ADR-009 chose Tokio userspace proxy for port mapping. This is correct and should stay.

**Improvement** (Phase 2): Add metrics on proxy throughput per port mapping. Add configurable `so_reuseport` to allow zero-downtime port re-binding during VM restart.

---

## Version Summary

| Version | Focus | Duration | Key Deliverable |
|---------|-------|----------|-----------------|
| **v1.1** | Current state | — | Baseline |
| **v1.2** | Critical fixes | 8 weeks | CMD/ENTRYPOINT works, virtiofsd bundled, constrained privileges, WAL, Compose v3 |
| **v1.3** | Hardening | 6 weeks | Storage/net refactor to Rust-native crates, chaos tests, 98% Docker Hub compat |
| **v1.5** | Power features | 12 weeks | WireGuard Swarm, Raft consensus, Teleport, gRPC, Vyoma Hub, VMIF, Studio v2 |
| **v2.0** | Revolutionary | 16 weeks | TimeMachine, Hibernation, vk8s CRI, Trusted Boot, vyoma-agent, SDK |
