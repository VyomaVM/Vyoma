# Installation & Setup Guide

This guide details the prerequisites, package installation, and post-installation configuration needed to run Vyoma.

## System Prerequisites

Before installing Vyoma, ensure your host environment satisfies the following requirements:

| Component | Minimum Requirement | Recommended | Check Command |
|-----------|---------------------|-------------|---------------|
| **Operating System** | Linux (Ubuntu 20.04+, Debian 11+, Fedora 36+, RHEL 8+) | Ubuntu 22.04 LTS / 24.04 LTS | `cat /etc/os-release` |
| **Kernel** | Linux 5.10+ | Linux 6.1+ | `uname -r` |
| **Hardware Virtualization** | Intel VT-x or AMD-V enabled in BIOS | Enabled with KVM access | `ls -l /dev/kvm` |
| **Cgroups** | Cgroups v2 enabled | Unified hierarchy | `stat -fc %T /sys/fs/cgroup/` |
| **Networking** | TUN/TAP kernel module | Enabled | `ls -l /dev/net/tun` |

> [!IMPORTANT]
> If you are running inside a virtual machine (e.g. AWS EC2, GCP, VMware, or WSL2), ensure **Nested Virtualization** is enabled so that `/dev/kvm` is present.

---

## 1. Package Installation

Vyoma provides native `.deb` and `.rpm` distribution packages that bundle all runtime binaries (including Cloud Hypervisor, CNI network plugins, and the Web UI).

### Debian / Ubuntu (.deb)

```bash
# Download the release package
wget https://github.com/Subeshrock/vyoma/releases/latest/download/vyoma_amd64.deb

# Install package
sudo apt update
sudo apt install ./vyoma_amd64.deb
```

### Fedora / RHEL / Rocky Linux (.rpm)

```bash
# Download the release package
wget https://github.com/Subeshrock/vyoma/releases/latest/download/vyoma.x86_64.rpm

# Install package
sudo rpm -i vyoma.x86_64.rpm
```

### Installed Components

The installer populates the standard system paths:
- `/usr/bin/vyomad`: The system daemon service.
- `/usr/bin/vyoma`: The user-facing CLI.
- `/usr/bin/cloud-hypervisor`: The bundled Virtual Machine Monitor.
- `/usr/lib/vyoma/cni/bin/`: Bundled CNI plugins (`bridge`, `host-local`, `ptp`).
- `/usr/lib/vyoma/virtiofsd`: Bundled volume sharing daemon.
- `/usr/lib/vyoma/ui/`: Production dashboard web build.
- `/etc/systemd/system/vyomad.service`: Systemd service definition.

---

## 2. Post-Installation Configuration

### Add User to `vyoma` Group

The `vyomad` daemon runs under a dedicated `vyoma` system user, and its control socket at `/run/vyoma/vyoma.sock` has `0660` permissions owned by `vyoma:vyoma`. 

To allow your user account to run `vyoma` commands without `sudo`:

```bash
# Add current user to the vyoma group
sudo usermod -aG vyoma $USER

# Apply the group in your current terminal session (or log out and back in):
newgrp vyoma
```

### Start and Enable the Daemon

```bash
# Reload systemd and enable daemon on boot
sudo systemctl daemon-reload
sudo systemctl enable --now vyomad

# Verify service status
systemctl status vyomad
```

---

## 3. Verify Installation

Run the built-in diagnostic tool:

```bash
vyoma doctor
```

Expected output:
```text
[✓] KVM Virtualization Acceleration (/dev/kvm)
[✓] TUN/TAP Kernel Module (/dev/net/tun)
[✓] Cloud Hypervisor VMM Engine
[✓] CNI Network Plugins
[✓] VirtioFS Daemon (virtiofsd)
[✓] Daemon Socket Connection (/run/vyoma/vyoma.sock)
All system requirements verified successfully!
```
