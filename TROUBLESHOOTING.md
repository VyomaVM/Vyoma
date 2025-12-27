# Troubleshooting Guide

## Common Issues

### 1. "Ghost Disks" / Leaked Loop Devices
**Symptoms:**
- After stopping `ignite` or restarting your computer, you see many 2GB "Removable Disks" or icons in your file manager.
- Running `losetup -a` shows many `/dev/loopX` devices attached to `.ignite/images/...` or `.ignite/vms/...`.

**Cause:**
- The `ignited` daemon was force-killed (SIGKILL) or crashed before it could run its cleanup logic.
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
3. Detach all Ignite-related loop devices.
4. Clean up network namespaces and TAP interfaces.

### 2. Sudo Password Issues
**Symptoms:**
- `ignited` fails to start or commands fail with permission errors.
**Cause:**
- Ignite currently requires root privileges for KVM, Networking, and Storage management.
**Resolution:**
- Ensure you run `ignited` with `sudo`.
- Future versions (Roadmap Phase 15) aim to support Rootless mode.
