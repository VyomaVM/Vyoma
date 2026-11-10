# Troubleshooting Guide

## Common Issues

### 1. Lingering VMs / Runtime State Cleanup
**Symptoms:**
- Orphaned `cloud-hypervisor` or `virtiofsd` processes remain after an unclean host shutdown or `SIGKILL`.

**Resolution:**
Run the included cleanup utility:
```bash
sudo ./scripts/cleanup-all.sh
```
This cleans up lingering VMM/VirtioFS processes, any stale TAP/netns interfaces, and temporary VM state.

### 2. Permission Issues (`/dev/kvm` or Daemon Socket)
**Symptoms:**
- `vyomad` logs `/dev/kvm` warnings or `vyoma run` reports `/dev/kvm not found` or permission denied.

**Resolution:**
- **Rootless Mode (v3.0+):** `vyomad` runs rootless using **QCOW2 overlays** and **Vsock L4 port forwarding** — `sudo`, `dmsetup`, `slirp4netns`, and `iptables` are **not** required for container port publishing (`-p 8080:80`).
- Ensure your user (and the `vyoma` service user) is in the `kvm` group (`sudo usermod -aG kvm $USER`) and `/dev/kvm` has `0660` permissions.
- If running inside a VM (e.g., VirtualBox/VMware/WSL2), enable **nested virtualization**.
