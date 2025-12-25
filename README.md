# Ignite 🔥

**Ignite** is a modern, open-source micro-VM manager that brings the user experience of Docker to Firecracker micro-VMs. It allows you to spin up secure, fast, and lightweight virtual machines in milliseconds, using standard OCI (Docker) images.

> **Status**: v0.1.0-rc1 (Release Candidate)

## 🚀 Features

*   **Docker-like CLI**: `ign run`, `ign ps`, `ign stop`, `ign logs`.
*   **OCI Image Support**: Pull directly from Docker Hub (`ign pull alpine:latest`).
*   **Instant Clones**: Uses Device Mapper snapshots for sub-second VM delivery.
*   **Developer Friendly**: Port mapping (`-p 8080:80`) and Volume mounts (`-v $PWD:/app`).
*   **Teleportation**: Snapshot a running VM, export it to a file, and move it to another machine.
*   **Time Travel**: Built-in Git integration for VM state management.
*   **GitOps Built-in**: Define your VM builds with `Ignitefile`.

## 🛠️ Installation

### Prerequisites
*   Linux (x86_64) with KVM enabled (`/dev/kvm`).
*   `sudo` privileges (for networking and storage management).
*   `firecracker` (v1.6+) and `virtiofsd` binaries in your path.

### Build from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/ignite.git
cd ignite

# Build release binaries
cargo build --release

# Run validation checks
./target/release/ign doctor
```

## ⚡ Quick Start

1.  **Start the Daemon**
    ```bash
    sudo ./target/release/ignited &
    ```

2.  **Pull an Image**
    ```bash
    ./target/release/ign pull alpine:latest
    ```

3.  **Run a Micro-VM**
    ```bash
    ./target/release/ign run alpine:latest --vcpu 1 --memory 512
    ```

4.  **Check Status**
    ```bash
    ./target/release/ign ps
    ```

## 📖 Documentation

*   [Project Summary](PROJECT_SUMMARY.md): Architecture and deep dive.
*   [Architecture Decisions (ADRs)](ADR.md): Why we made certain design choices.
*   [Development Tasks](tasks.md): Current roadmap and progress.

## 🤝 Contributing

We welcome contributions! Please check `tasks.md` for upcoming features like CNI Networking and Rootless Mode.

## 📄 License

MIT License.
