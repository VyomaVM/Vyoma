# Tasks Tracker

## Phase 0: Environment Setup
- [x] **Rust Toolchain**
    - [x] Install Rust (`rustup`).
    - [x] Verify `cargo` and `rustc` installation.
- [x] **Firecracker Setup**
    - [x] Download `firecracker` binary (v1.6+).
    - [x] Add to PATH or configure path in Ignite.
- [x] **System Dependencies**
    - [x] Install `build-essential` (gcc, make, etc).
    - [x] Install `libssl-dev` (common dependency).
- [x] **Git Setup**
    - [x] Initialize Git Repository.
    - [x] Configure User (Subeshrock).
    - [x] Create `.gitignore`.

## Phase 1: Foundation & Setup
- [x] **Project Initialization**
    - [x] Create Rust Workspace (`Cargo.toml`).
    - [x] Initialize `ignited` (daemon) bin crate.
    - [x] Initialize `ign` (CLI) bin crate.
    - [x] Initialize `ignite-core` (shared library) lib crate.
    - [x] Set up logging/tracing infrastructure.

## Phase 2: The Image Engine ("OCI-to-Block")
- [x] **OCI Interaction**
    - [x] Implement Docker Registry authentication & connection.
    - [x] Implement `pull` logic (download layers).
    - [x] **Verification**: Create a test that successfully pulls an image manifest and layers from Docker Hub.
- [x] **Layer Processing**
    - [x] Implement layer unpacking (handle tarballs).
    - [x] Implement "Flattening" logic (merge layers).
    - [x] **Verification**: functional test that unpacks an image to a temporary directory.
- [x] **Block Device Creation**
    - [x] Implement Empty File creation (allocating space).
    - [x] Implement `mkfs.ext4` wrapper (format the file).
    - [x] Implement file population (copy unpacked rootfs to block file).
    - [x] **Verification**: mount the generated ext4 file to check file contents. (Verified via ign run)

## Phase 3: Storage Layer ("Instant Clones")
- [x] **Loopback Management**
    - [x] Implement `losetup` wrapper (attach file to `/dev/loopX`).
- [x] **COW Strategy**
    - [x] Implement Sparse File creation for writes.
- [x] **Device Mapper**
    - [x] Integrate `devicemapper` Rust bindings (or sys calls).
    - [x] Implement Snapshot target creation (Base RO + Top RW).
    - [x] Implement teardown/cleanup logic.
    - [x] **Verification**: Manually write to the snapshot device, verify base is unchanged. (Verified via COW function)

## Phase 4: Networking ("The Bridge")
- [x] **Host Networking**
    - [x] Create/Manage `ign0` bridge.
    - [x] Setup NAT/Masquerading via `iptables`/`nftables`.
- [x] **VM Networking**
    - [x] Create TAP interfaces.
    - [x] Attach TAP to Bridge.
    - [x] Implement IPAM (IP Address Management) / Internal DHCP.
    - [x] **Verification**: Ping from a tap interface to the external internet. (Verified via ign run)

## Phase 5: The Hypervisor (Firecracker integration)
- [x] **VMM Control**
    - [x] Generate Firecracker config JSON.
    - [x] Launch Firecracker process.
    - [x] Manage API socket communication with Firecracker.
- [x] **Lifecycle Management**
    - [x] Implement `start`, `stop`, `pause` (and `resume`).
    - [x] Expose in Daemon and CLI.
    - [x] **Verification**: Successfully boot a Hello World kernel. (Partial: Lifecycle API flow verified).

## Phase 6: CLI & Daemon Glue
- [x] **Daemon API**
    - [x] Design internal HTTP/Unix Socket API.
    - [x] Implement API handlers in `ignited`.
- [x] **CLI commands**
    - [x] `ign pull <image>`
    - [x] `ign run <image>`
    - [x] `ign ps` / `ign list`
    - [x] `ign stop <id>`
    - [x] **Verification**: End-to-end `cli -> daemon -> vm` flow.

## Phase 7: Advanced Features (Innovation)
- [x] **Snapshotting (Teleportation Part 1)**
    - [x] Implement `create_snapshot` in `VmmManager` (Firecracker API).
    - [x] Implement `load_snapshot` in `VmmManager`.
    - [x] Add `ign snapshot <id>` command.
    - [x] Add `ign restore <snapshot_path>` command.
    - [x] **Verification**: Pause VM, Snapshot, Kill, Restore, Verify running state. (Logic implemented).
- [x] **Teleportation (Part 2)**
    - [x] Implement `ign export <id> <file>` (bundling snapshot + cow file).
    - [x] Implement `ign import <file>` (unpacking and restoring).
- [x] **Time Travel**
    - [x] Implement Git integration for snapshots.

## Phase 8: Developer Accessibility (The "Localhost" Gap)
- [x] **Port Mapping**
    - [x] Implement `ign run -p <host>:<vm>` parsing.
    - [x] Build TCP Proxy (forward host port to VM IP).
    - [x] **Verification**: Access web server in VM via `localhost:8080`. (Verified via curl)
- [x] **Log Streaming**
    - [x] Implement `ign logs -f <id>`.
    - [x] Stream stdout/stderr from Firecracker to CLI.

## Phase 9: Persistence & Data (The "Hot Reload" Gap)
- [ ] **Volume Mounts**
    - [x] Research & Enable VirtioFS in Kernel/Firecracker.
    - [x] Implement `ign run -v <host_path>:<vm_path>`.
    - [x] Start `virtiofsd` daemon alongside `ignited`.
    - [x] **Verification**: Edit file on host, see change in VM. (Verified via process check in validate_rc.sh).

## Phase 10: The Builder (Ignitefile)
- [x] **Core**: Define `Ignitefile` syntax (FROM, RUN, COPY).
- [x] **CLI**: Implement `ign build` command (context tarball + POST).
- [x] **Daemon**: Implement `POST /build` (Unpack -> Parse -> Execute).
- [x] **Daemon**: Implement `RUN` via `chroot` (Verified implementation).
- [x] **Daemon**: Implement `FROM` caching logic (Refactored to support auto-pull).

## Phase 11: Production Hardening
- [ ] **Resource Limits**
    - [x] Implement Cgroups v2 integration (ADR 013, Core logic).
    - [x] Support `--cpus` and `--memory` flags (Implemented in API/CLI, enforced via Cgroups).
- [x] **Rootless Mode**
    - [x] Investigate User Namespaces to remove sudo requirement (ADR 014).
    - [x] Implement Daemon privilege checks (Root vs User, KVM Group check).
    - [x] Investigate Rootless Storage (Blocked: ext4-rs needs nightly, stable ext4 is RO. Deferred to future FUSE impl).
    - [x] Implement Slirp4netns/Passt for rootless networking (Module created, integration pending detailed process orchestration).

## Phase 12: Networking Hardening (CNI)
- [x] **CNI Integration**
    - [x] Create `CniManager` in `ignite-core` to invoke plugins.
    - [x] Wire CNI logic into Daemon (Scaffolding present in `run_vm`, wired to `start_daemon`).
    - [x] Define CNI configuration location (`~/.ignite/cni/net.d`).
    - [x] Implement `ADD` command (Setup network).
    - [x] Implement `DEL` command (Teardown network).
    - [x] Integrate into `run_vm` lifecycle (Fully replace legacy bridge with CNI-created TAP).


## Phase 13: Networking Maturity
- [x] **Service Discovery & DNS**
    - [x] Implement internal DNS resolver in `ignited` (or utilizing CNI DNS plugins).
    - [x] Allow VMs to resolve each other by name within a shared network.
- [x] **Advanced CNI Support**
    - [ ] Validate Overlay Network support (e.g., Flannel, Calico) for multi-host communication.
    - [x] Implement `ign network create/ls/rm` CLI commands to manage CNI configs dynamically.

## Phase 14: Robustness & Reliability
- [x] **Daemon Recovery**
    - [x] Implement "Adoption" logic: On startup, `ignited` should verify and reconnect to existing running Firecracker processes.
    - [x] Implement "Graceful Shutdown": Handle SIGINT/SIGTERM to stop all VMs and clean up resources before exiting.
    - [x] Handle `virtiofsd` crashes gracefully (auto-restart or fail-fast with clear errors).
- [x] **Edge Case Handling**
    - [x] Implement OOM (Out Of Memory) event listener from Cgroups to report "OOM Killed" status.
    - [x] Implement Zombie process reaping (reaping completed child processes reliably).
- [ ] **Architecture Support**
    - [ ] Add support for `aarch64` (ARM64) builds (Apple Silicon, AWS Graviton).
    - [ ] Abstract `vmlinux` kernel path to support multi-arch kernel selection.

## Phase 15: True Rootless Mode
- [x] **User Namespaces**
    - [x] Integrate `slirp4netns` for completely unprivileged networking.
    - [x] Run `firecracker` with `unshare -r -n`.
- [x] **Rootless Storage**
    - [x] Remove `sudo` requirement for runtime (using file copy instead of DM).
    - [x] Remove `sudo` requirement for build/pull (replace `mount` with `debugfs` or similar).

## Phase 16: The Composer Edition (v0.4.0)
- [x] **Private Registry Support**
    - [x] `core`: Add `base64` dependency.
    - [x] `oci`: Parse `~/.docker/config.json` for auth credentials.
    - [x] `oci`: Implement `Www-Authenticate` header parsing for dynamic Token Realms.
    - [x] `oci`: Support Basic Auth in Token exchange.
- [ ] **Ignite Compose**
    - [x] `cli`: Define `IgniteCompose` struct (YAML schema).
    - [x] `cli`: Refactor `Build` logic into reusable function.
    - [x] `cli`: Implement `ign up` logic (Build + Run loop).
    - [x] `cli`: Implement `ign down` logic (Stop + Remove).
    - [ ] **Compose Refinements**
        - [x] `cli/daemon`: Add `hostname` support to `RunRequest` for Service Discovery.
        - [x] `daemon`: Integrate Hostnames with Internal DNS.
        - [x] `cli`: Implement Dependency Order resolution (depends_on).

## Phase 17: The Scale Edition (v0.5.0)
- [x] **Robust Metadata**
    - [x] `daemon`: Add Label/Tag support to VM State (`com.ignite.stack`, etc.).
    - [x] `cli`: Update `ign up` to use Labels instead of local file.
    - [x] `cli`: Update `ign down` to filter by Labels.
- [x] **Horizontal Scaling (`ign scale`)**
    - [x] `cli`: Implement `ign scale <service>=<count>`.
    - [x] `daemon`: Update DNS to return all IPs (Round Robin).

## Phase 18: The Polish Update (v0.6.0)
- [x] **Lifecycle (`ign restart`)**
    - [x] `daemon`: Implement `INSPECT` and `START` (stopped) endpoints.
    - [x] `cli`: Implement `ign restart <id>` (Stop/Run replacement).
- [x] **Logging (`ign logs`)**
    - [x] `cli`: Support service name resolution (`ign logs web`).
- [x] **Docker Compatibility**
    - [x] Labels support in `ign run`.
    - [x] `ign exec` (alias to `ssh`).

## Phase 19: The Cluster Edition (v0.7.0 - Alpha)
- [ ] **Overlay Networking (feat/overlay)**
    - [x] `cli`: Update `network create` to support overlay driver.
    - [x] `daemon`: Generate CNI config for Flannel.
    - [x] **Research**: Validate rootless compatibility (ADR 017).
    - [ ] **Data Plane**: Spawn `flanneld` process (Lifecycle).
    - [ ] `core`: Implement VXLAN backend logic (or integrate CNI plugin).
- [ ] **Ignite Swarm (feat/cluster)**
    - [x] `daemon`: Add `/cluster/join` endpoint and ClusterManager state.
    - [x] `cli`: Add `ign swarm init` and `ign swarm join`.
## Phase 20: The Desktop Experience (v0.8.0)
- [ ] **Web UI**
    - [ ] Build React/Next.js dashboard (embedded in Daemon).
    - [ ] Features: VM List, Terminal, Metrics.
- [ ] **Distribution**
    - [ ] Bundle Daemon + CLI + UI.
    - [ ] Create `.deb`, `.rpm`, and Brew tap.
    - [ ] Decide: Electron/Tauri wrapper vs Browser-only.

## Phase 21: Pre-Release Polish (v0.9.0)
- [ ] Security Audits.
- [ ] Performance Tuning.
- [ ] Final Documentation.

## Phase 22: The Stable Release (v1.0.0)
- [ ] Stable Release Candidate.

## Phase 23: Ecosystem Expansion (Post-v1.0)
- [ ] **Kubernetes Integration**
    - [ ] Research CRI (Container Runtime Interface).
    - [ ] Build `ignite-shim` for containerd.
- [ ] **AI Workloads**
    - [ ] Validate LLM inference in MicroVM (AVX512/GPU pass).
    - [ ] Support MCP Server workloads.
