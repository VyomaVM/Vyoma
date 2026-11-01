# Architecture: Native QCOW2 Backing File Storage Engine

**Status**: Active / Production Default  
**Target Release**: Vyoma v3.0.0  
**Backend**: Cloud Hypervisor v54.0 Userspace Storage Driver  

---

## 1. Overview & Motivation

In previous versions of Vyoma (v2.x), microVM instant branching relied on the Linux kernel **Device Mapper** (`dm-snapshot`) and **loop devices** (`/dev/loopX`). 

While functional, this approach incurred significant operational liabilities:
1. **Ghost Disks & Resource Leaks**: If the daemon crashed or received an uncatchable `SIGKILL`, loop devices remained mounted and dm-snapshot devices remained active in the host kernel.
2. **Privilege Overhead**: Required `CAP_SYS_ADMIN` and host root permissions to run `dmsetup` and `losetup`.
3. **Host Inode Exhaustion**: Concurrent VM creation quickly consumed kernel minor loop numbers (`/dev/loop0`–`/dev/loop7`).

Vyoma v3.0.0 transitions the storage subsystem to **native QCOW2 copy-on-write overlays** with backing files directly handled in userspace by Cloud Hypervisor v54.0.

---

## 2. Technical Architecture

```
+-----------------------------------------------------------+
| Host Filesystem (Userspace)                               |
|                                                           |
|   /var/lib/vyoma/images/alpine_latest/rootfs.sqfs         |
|   (Read-Only Base Image - Raw or QCOW2)                   |
|                        ^                                  |
|                        | backing file                     |
|   /var/lib/vyoma/vms/vm-1234/root.qcow2                   |
|   (Thin QCOW2 CoW Overlay - ~196KB initially)             |
+------------------------|----------------------------------+
                         |
                 Virtio-Blk Device
                         v
+-----------------------------------------------------------+
| Guest MicroVM Kernel                                      |
|                                                           |
|   /dev/vda (Virtio-blk) -> /lower                         |
|   OverlayFS (/lower + /overlay/upper) -> /sysroot         |
+-----------------------------------------------------------+
```

### Storage Provisioning Flow
1. **Overlay Creation**:
   When launching VM `vm-id`, `vyomad` invokes `qemu-img` to allocate a thin QCOW2 overlay pointing to the base image:
   ```bash
   qemu-img create -f qcow2 -b <base_image_path> -F <raw|qcow2> <vm_dir>/root.qcow2 10G
   ```
2. **Cloud Hypervisor Attachment**:
   The VMM attaches the disk with native Cloud Hypervisor v54 parameters:
   ```json
   {
     "id": "rootfs",
     "path": "/var/lib/vyoma/vms/vm-id/root.qcow2",
     "image_type": "Qcow2",
     "backing_files": true,
     "readonly": false
   }
   ```
3. **Zero-Cleanup Termination**:
   When a VM is stopped or deleted, `vyomad` simply unlinks the `root.qcow2` file from the filesystem. No kernel device mapper tables or loop devices need detachment.

---

## 3. Resilience & Fallback

If `qemu-img` or userspace QCOW2 creation encounters an unexpected failure, `vyomad` automatically falls back to privileged Device Mapper snapshotting to ensure backward compatibility.
