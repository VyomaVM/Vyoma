# Ignite: Docker for Micro-VMs - Project Summary

## Overview
**Ignite** is a Rust-based container-like orchestrator for Micro-VMs (Firecracker). It brings the developer experience of Docker (images, containers, ease of use) to the world of secure, isolated Micro-VMs.

## Architecture
The project is organized as a Rust Workspace with three main crates:

1.  **`ignited` (Daemon)**: The background service running as root.
    *   Manages the lifecycle of VMs.
    *   Handles Storage (Loop devices, Device Mapper).
    *   Handles Networking (Tun/Tap, Bridges, DHCP/IPAM).
    *   Interacts with the Firecracker VMM via Unix Socket API.
    *   Exposes a REST API for the CLI.

2.  **`ign` (CLI)**: The user-facing command-line tool.
    *   Communicates with the daemon via HTTP.
    *   Commands: `run`, `stop`, `ps`, `snapshot`, `restore`, `export`, `import`.

3.  **`ignite-core` (Library)**: Shared business logic.
    *   **`oci`**: Pulls Docker images (manifests/layers) and parses them.
    *   **`layers`**: Unpacks gzipped tarballs into raw filesystems.
    *   **`storage`**: manages `mkfs.ext4`, `losetup`, `dmsetup` (snapshots), and sparse COW files.
    *   **`network`**: Manages `ip link`, `iptables`, and TAP device creation.
    *   **`vmm`**: A wrapper around the Firecracker HTTP API.

## Key Features Implemented

### 1. OCI-to-Block Engine ("Just-in-Time Conversion")
*   **Problem**: Firecracker needs a block device (file), Docker gives tarballs.
*   **Solution**: Ignite pulls layers, unpacks them into a temp dir, creates a sized empty file, formats it as `ext4`, and populates it.

### 2. Storage: Instant Clones
*   **Problem**: Copying a 2GB OS image for every VM is slow.
*   **Solution**:
    *   **Base Image**: Read-Only loop device.
    *   **Diff File**: Tiny sparse "Copy-on-Write" (COW) file.
    *   **Device Mapper**: Creates a snapshot combining the Base and Diff.
*   **Result**: VM creation takes milliseconds.

### 3. Networking
*   **Setup**: Creates a host bridge `ign0`.
*   **VMs**: Each VM gets a TAP interface connected to the bridge.
*   **NAT**: Traffic is masqueraded to allow internet access from inside the VM.

### 4. Innovation: Teleportation
*   **Snapshot**: Freezes VM RAM and CPU state to disk.
*   **Export**: Bundles the RAM snapshot (`.snap`, `.mem`) AND the Disk state (`.cow`) into a standard tarball.
*   **Import/Restore**: Unpacks the tarball and restores the VM on any machine (conceptually).

### 5. Innovation: Time Travel
*   **Git Integration**: The daemon explicitly initializes a Git repository inside the VM's state directory.
*   **Versioning**: Every snapshot action triggers a `git commit`, enabling version control of the VM's entire runtime state.

## Directory Structure
```text
micro-vm-ecosystem/
├── crates/
│   ├── ign/            # CLI Tool
│   │   └── src/main.rs
│   ├── ignited/        # Daemon
│   │   └── src/main.rs
│   └── ignite-core/    # Core Logic
│       ├── src/
│       │   ├── oci.rs       # Image Pulling
│       │   ├── storage.rs   # Device Mapper & Filesystem
│       │   ├── network.rs   # Bridge & TAP
│       │   ├── vmm.rs       # Firecracker Client
│       │   └── layers.rs    # Tarball handling
│       └── tests/           # Integration Tests
├── bin/
│   ├── firecracker     # Binary dependencies
│   └── vmlinux         # Kernel binary
├── .ignite/            # Runtime State (Created in User Home)
│   ├── images/         # Cached Base Images
│   └── vms/            # Active VM State (COW, Snapshots, Git)
├── tasks.md            # Progress Tracker
└── ADR.md              # Architectural Decisions
```

## How to Run

### Prerequisites
*   Linux (Ubuntu recommended).
*   `sudo` access (passwordless recommended for automation).
*   `firecracker` binary in `bin/`.
*   `vmlinux` kernel in `bin/`.
*   `virtiofsd` binary in `bin/` (for Volume Mounts).

### Steps
1.  **Build**:
    ```bash
    cargo build --release
    ```
2.  **Start Daemon** (Needs Root):
    ```bash
    sudo ./target/release/ignited
    ```
3.  **Run a VM** (In new terminal):
    ```bash
    ./target/release/ign run alpine:latest
    ```
4.  **Manage**:
    ```bash
    ./target/release/ign ps
    ./target/release/ign snapshot <vm-id>
    ./target/release/ign export <vm-id> backup.tar.gz
    ```

## Future Work
*   **Secure Networking**: Add CNI plugin support.
*   **Resource Limits**: fully utilize cgroups for CPU/Memory isolation (Firecracker does this, but Ignite can expose fine-grained controls).
*   **Registry Auth**: Support private Docker registries.
