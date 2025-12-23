# Tasks Tracker

## Phase 0: Environment Setup
- [x] **Rust Toolchain**
    - [x] Install Rust (`rustup`).
    - [x] Verify `cargo` and `rustc` installation.
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
- [ ] **OCI Interaction**
    - [ ] Implement Docker Registry authentication & connection.
    - [ ] Implement `pull` logic (download layers).
    - [ ] **Verification**: Create a test that successfully pulls an image manifest and layers from Docker Hub.
- [ ] **Layer Processing**
    - [ ] Implement layer unpacking (handle tarballs).
    - [ ] Implement "Flattening" logic (merge layers).
    - [ ] **Verification**: functional test that unpacks an image to a temporary directory.
- [ ] **Block Device Creation**
    - [ ] Implement Empty File creation (allocating space).
    - [ ] Implement `mkfs.ext4` wrapper (format the file).
    - [ ] Implement file population (copy unpacked rootfs to block file).
    - [ ] **Verification**: mount the generated ext4 file to check file contents.

## Phase 3: Storage Layer ("Instant Clones")
- [ ] **Loopback Management**
    - [ ] Implement `losetup` wrapper (attach file to `/dev/loopX`).
- [ ] **COW Strategy**
    - [ ] Implement Sparse File creation for writes.
- [ ] **Device Mapper**
    - [ ] Integrate `devicemapper` Rust bindings (or sys calls).
    - [ ] Implement Snapshot target creation (Base RO + Top RW).
    - [ ] Implement teardown/cleanup logic.
    - [ ] **Verification**: Manually write to the snapshot device, verify base is unchanged.

## Phase 4: Networking ("The Bridge")
- [ ] **Host Networking**
    - [ ] Create/Manage `ign0` bridge.
    - [ ] Setup NAT/Masquerading via `iptables`/`nftables`.
- [ ] **VM Networking**
    - [ ] Create TAP interfaces.
    - [ ] Attach TAP to Bridge.
    - [ ] Implement IPAM (IP Address Management) / Internal DHCP.
    - [ ] **Verification**: Ping from a tap interface to the external internet.

## Phase 5: The Hypervisor (Firecracker integration)
- [ ] **VMM Control**
    - [ ] Generate Firecracker config JSON.
    - [ ] Launch Firecracker process.
    - [ ] Manage API socket communication with Firecracker.
- [ ] **Lifecycle Management**
    - [ ] Implement `start`, `stop`, `pause`.
    - [ ] **Verification**: Successfully boot a Hello World kernel.

## Phase 6: CLI & Daemon Glue
- [ ] **Daemon API**
    - [ ] Design internal HTTP/Unix Socket API.
    - [ ] Implement API handlers in `ignited`.
- [ ] **CLI commands**
    - [ ] `ign pull <image>`
    - [ ] `ign run <image>`
    - [ ] `ign ps` / `ign list`
    - [ ] `ign stop <id>`
    - [ ] **Verification**: End-to-end `cli -> daemon -> vm` flow.

## Phase 7: Advanced Features (Innovation)
- [ ] **Teleportation**
    - [ ] Implement Snapshot logic.
    - [ ] Implement Transfer logic (scp/rsync wrapper).
- [ ] **Time Travel**
    - [ ] Implement Git integration for snapshots.
