# Ignite 🔥: The Micro-VM Ecosystem

[![Release Badge](https://img.shields.io/github/v/release/Subeshrock/micro-vm-ecosystem?style=flat-square)](https://github.com/Subeshrock/micro-vm-ecosystem/releases)
[![Build Status](https://img.shields.io/github/actions/workflow/status/Subeshrock/micro-vm-ecosystem/release.yml?style=flat-square)](https://github.com/Subeshrock/micro-vm-ecosystem/actions)

**Ignite** is a production-grade Micro-VM manager. It combines the blazing speed and security of **Firecracker** with the beloved developer experience of **Docker**.

Spin up secure, isolated VMs in milliseconds using standard OCI (Docker) images. No heavy kernels, no complex QEMU flags—just code.

---

## 🚀 Key Features (v1.0)

*   **Docker UX**: Familiar commands: `run`, `ps`, `logs`, `exec`.
*   **Ignite Swarm**: Built-in clustering with **Mesh Networking** (VXLAN) and deterministic IP allocation.
*   **Ignite Compose**: Orchestrate stacks with `ignite-compose.yml`.
*   **Web Dashboard**: Built-in visual management UI (bundled in Daemon, accessible at `http://localhost:3000`).
*   **Self-Contained**: `.deb` / `.rpm` packages bundle primary dependencies (Firecracker).
*   **Persistence**: Volume mounts (`virtiofs`) and Port Mapping.
*   **Snapshotting**: Instant VM snapshots and State Teleportation.

---

## � Installation

### 1. Download Package
Go to the [Releases Page](https://github.com/Subeshrock/micro-vm-ecosystem/releases) and download the latest package for your distro.

### 2. Install (Debian/Ubuntu)
The package automatically installs the Daemon (`ignited`) as a systemd service running as root, and the Client (`ign`) for users.

```bash
sudo dpkg -i ignite_1.0.0_amd64.deb
```

**What's Installed:**
*   `/usr/bin/ignited`: The Daemon (Run via Systemd).
*   `/usr/bin/ign`: The CLI Tool.
*   `/usr/bin/firecracker`: Bundled VMM binary.
*   `/etc/systemd/system/ignited.service`: Service definition.

### 3. Verify Installation
```bash
ign doctor
```
This utility checks for KVM (`/dev/kvm`), tun/tap support, and daemon connectivity.

---

## ⚡ Usage Guide

For a complete reference of all 20+ commands, see [COMMANDS.md](COMMANDS.md).

### Lifecycle
*   **Run a VM**:
    ```bash
    ign run ubuntu:latest --vcpu 2 --memory 1024 -p 8080:80
    ```
*   **List VMs**: `ign ps`
*   **Logs**: `ign logs -f <vm_id>`
*   **Shell Access**: `ign exec <vm_id> /bin/bash`
*   **Stop/Remove**: `ign stop <vm_id>`, `ign rm <vm_id>`

### Networking
Ignite uses CNI for robust networking.
*   **List Networks**: `ign network ls`
*   **Create Network**: `ign network create my-net --subnet 10.10.0.0/16`

### Clustering (Ignite Swarm)
Turn multiple machines into a single mesh.
1.  **Initialize Seed (Leader)**:
    ```bash
    ign swarm init
    # OR specify IP: ign swarm init --advertise-addr 192.168.1.10
    ```
2.  **Join Worker**:
    ```bash
    ign swarm join <SEED_IP>
    ```
3.  **List Nodes**: `ign swarm ls`

Swarm ensures unique Subnet Leases (e.g., `10.42.1.0/24` for Node 1) and routes traffic transparently over VXLAN.

### Orchestration (Ignite Compose)
Define complex stacks using `ignite-compose.yml`:
```yaml
version: "1.0"
services:
  web:
    image: nginx:alpine
    ports: ["80:80"]
  db:
    image: postgres:15
    cpus: 2
```
*   **Deploy**: `ign up -d`
*   **Teardown**: `ign down`

---

## 📘 Ignitefile Reference (Build)

The `Ignitefile` is used to build custom images via `ign build`.

```dockerfile
# Start from a base image (Docker Hub or local)
FROM alpine:3.18

# Run commands to install packages or setup environment
RUN apk add --no-cache python3 curl

# Copy files from host execution context to VM
COPY app.py /app/app.py
COPY config.json /etc/config.json
```
> **Note**: `CMD`, `ENV`, and `ENTRYPOINT` are not yet supported. VMs start with `/bin/sh` init unless overridden.

---

## 📘 Ignite Compose Reference

Use `ignite-compose.yml` to orchestrate multi-VM stacks.

```yaml
version: "1.0"
services:
  web:
    image: nginx:alpine
    ports: 
      - "8080:80"        # Host:VM
    volumes:
      - "./html:/var/www" # Host:VM (Requires virtiofsd)
    cpus: 1              # vCPU limit
    memory: 512          # Memory limit in MiB
    depends_on:
      - db

  db:
    build: 
      context: ./database
      ignitefile: Ignitefile
    environment:         # Environment variables (WIP)
      POSTGRES_PASSWORD: secret
```
> **Note**: Custom `networks:` are not supported in v1.0. Services communicate via the default bridge using their Service Name as hostname (DNS enabled).

---

## 🔧 Architecture & Dependencies

Ignite is designed to be "Batteries Included", but some advanced features need helpers.

**Primary Dependencies (Bundled in Packet):**
*   **Firecracker**: The VMM (Virtual Machine Monitor).
*   **KVM**: Kernel-based Virtual Machine. **MUST be enabled in BIOS/OS** (`/dev/kvm`).
*   **Systemd**: Manages the `ignited` daemon lifecycle.

**Optional Dependencies (Install Manually):**
*   **Virtiofsd**: REQUIRED for Volume Mounts (`-v`).
    *   **Ubuntu/Debian**: `sudo apt install virtiofsd`
    *   **Fedora**: `sudo dnf install virtiofsd`
    *   **Manual**: Download binary from [GitLab](https://gitlab.com/virtio-fs/virtiofsd/-/releases) and place in `$PATH` or `/usr/bin`.

**Privilege Model:**
*   **Daemon (`ignited`)**: Runs as **Root**.
*   **Client (`ign`)**: Runs as **User**.

---

## 🧪 Testing & Development

For developers contributing to Ignite, please refer to:
*   [TESTING.md](TESTING.md) for running tests.
*   [COMMANDS.md](COMMANDS.md) for full CLI reference.

## 🤝 Contributing

We welcome contributions! Please feel free to open issues or PRs.

## 📄 License

MIT License.
