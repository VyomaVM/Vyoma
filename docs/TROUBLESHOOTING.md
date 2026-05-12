# Troubleshooting Guide

## Common Issues

### 1. "Ghost Disks" / Leaked Loop Devices
**Symptoms:**
- After stopping `vyoma` or restarting your computer, you see many 2GB "Removable Disks" or icons in your file manager.
- Running `losetup -a` shows many `/dev/loopX` devices attached to `.vyoma/images/...` or `.vyoma/vms/...`.

**Cause:**
- The `vyomad` daemon was force-killed (SIGKILL) or crashed before it could run its cleanup logic.
- As a result, the loopback devices used to mount the VM's hard drives were not detached.

**Resolution:**
We provide a utility script to clean up these leaked resources.
Run:
```bash
sudo ./cleanup_resources.sh
```
This will:
1. Kill any lingering Firecracker/VirtioFS processes.
2. Remove Device Mapper snapshots.
3. Detach all Vyoma-related loop devices.
4. Clean up network namespaces and TAP interfaces.

### 2. Sudo Password Issues
**Symptoms:**
- `vyomad` fails to start or commands fail with permission errors.
**Cause:**
- Vyoma currently requires root privileges for KVM, Networking, and Storage management.
**Resolution:**
- Ensure you run `vyomad` with `sudo`.
- **Rootless Mode (v0.3+):** You can run `vyomad` as a standard user.
  - Requires `slirp4netns` and `debugfs` (e2fsprogs) installed.
  - Sudo is NOT required for basic runtime and image pulling.
  - Note: Rootless mode uses User Networking (slirp), so VMs are not directly addressable from the host. Use port forwarding.
