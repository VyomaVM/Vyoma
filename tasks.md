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
    - [ ] **Verification**: mount the generated ext4 file to check file contents. (Skipped: Requires SUDO, logic implemented but not verified in CI)

## Phase 3: Storage Layer ("Instant Clones")
- [x] **Loopback Management**
    - [x] Implement `losetup` wrapper (attach file to `/dev/loopX`).
- [x] **COW Strategy**
    - [x] Implement Sparse File creation for writes.
- [x] **Device Mapper**
    - [x] Integrate `devicemapper` Rust bindings (or sys calls).
    - [x] Implement Snapshot target creation (Base RO + Top RW).
    - [x] Implement teardown/cleanup logic.
    - [ ] **Verification**: Manually write to the snapshot device, verify base is unchanged. (Skipped: Requires SUDO)

## Phase 4: Networking ("The Bridge")
- [x] **Host Networking**
    - [x] Create/Manage `ign0` bridge.
    - [x] Setup NAT/Masquerading via `iptables`/`nftables`.
- [x] **VM Networking**
    - [x] Create TAP interfaces.
    - [x] Attach TAP to Bridge.
    - [x] Implement IPAM (IP Address Management) / Internal DHCP.
    - [ ] **Verification**: Ping from a tap interface to the external internet. (Skipped: Requires SUDO)

## Phase 5: The Hypervisor (Firecracker integration)
- [x] **VMM Control**
    - [x] Generate Firecracker config JSON.
    - [x] Launch Firecracker process.
    - [x] Manage API socket communication with Firecracker.
- [ ] **Lifecycle Management**
    - [ ] Implement `start`, `stop`, `pause`.
    - [ ] **Verification**: Successfully boot a Hello World kernel.

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
- [ ] **Teleportation**
    - [ ] Implement Snapshot logic.
    - [ ] Implement Transfer logic (scp/rsync wrapper).
- [ ] **Time Travel**
    - [ ] Implement Git integration for snapshots.
