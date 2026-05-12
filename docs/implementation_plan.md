# Product Spec: The Micro-VM Ecosystem

## Vision
**"Vyoma: Docker for Micro-VMs"**
A comprehensive tooling suite that makes creating, running, and managing Micro-VMs (like Firecracker) as easy as managing containers.

## Technology Stack: Rust 🦀
*   **Daemon (`vyomad`)**: Rust + Tokio + Hyper.
*   **CLI (`vyoma`)**: Rust + Clap.
*   **Core Logic**: `rtnetlink` (Networking), `devicemapper-rs` (Storage).

## Technical Deep Dive: The Internals

### 1. The Image Engine: "OCI-to-Block" 📦
The hardest problem: Docker images are tarballs; VMs need block devices.
*   **Strategy**: "Just-in-Time Conversion"
    1.  **Pull**: Use `oci-distribution` crate to pull standard Docker layers.
    2.  **Flatten**: Unpack layers (OverlayFS style) into a temporary directory.
    3.  **Format**: Create an empty file, format as `ext4`, and copy files in.
    4.  **Result**: `.vyoma/.vyoma/images/ubuntu-22.04.ext4` (Read-Only Base Image).
*   **Kernel**: We ship a default high-performance kernel (Linux 6.x) bundled with the daemon, but allow images to override it.

### 2. Storage Strategy: "Instant Clones" 💾
We cannot copy a 500MB disk for every container start.
*   **Solution**: **Device Mapper Snapshots** (The tech behind Docker's original speed).
    *   **Base**: The `ubuntu-22.04.ext4` file is setup as a Loop Device (`/dev/loop0`).
    *   **Instance**: When `vyoma run` starts, we create a tiny "Cow File" (sparse file).
    *   **Map**: We use Device Mapper to create a virtual block device that reads from `loop0` but writes changes to the `cow-file`.
    *   **Speed**: Creation time is < 10ms. Disk usage is .vyoma10KB per new VM.

### 3. Networking: "The Bridge to Everywhere" 🌐
*   **Architecture**:
    *   **Host**: A bridge interface `ign0` (Default Gateway: 172.16.0.1).
    *   **VM**: Each VM gets a TAP device (e.g., `vmtap123`) connected to `ign0`.
    *   **IP Management**: The Daemon runs a tiny internal DHCP server (or configures static IPs via kernel command line arguments) to assign 172.16.0.x IPs.
    *   **Internet**: `iptables` rules on the host to Masquerade (NAT) traffic from `ign0` to the main internet interface (WiFi/Ethernet).

### 4. Innovation Implementation ⚡
*   **Teleportation**:
    *   Firecracker supports `snapshot`. It dumps the entire RAM + CPU Registers to a file.
    *   To "Teleport": Pause VM -> Snapshot -> `scp` the snapshot file & the COW file -> Load Snapshot on remote.
*   **Time Travel**:
    *   Just a Git wrapper around the Firecracker Snapshot files. `git commit` = `vyoma snapshot`.

## Ecosystem Architecture

### 1. The Engine (`vyomad`)
*   **Role**: Background daemon.
*   **Responsibilities**: API Server, Hypervisor Abstraction, Network Manager.

### 2. The Registry & Image Strategy
*   **Standard**: Use OCI Registries (Docker Hub).

## Next Steps
1.  **Environment**: Since we need `KVM`, `TAP`, and `Device Mapper`, implementing this directly on Windows is impossible.
2.  **Constraint**: We run this on **Pure Linux (Ubuntu)**.
3.  **Action**: Set up a Rust development environment.
