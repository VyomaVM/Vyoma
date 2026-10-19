# Storage Engine Architecture

The Vyoma storage subsystem is designed to eliminate the overhead of standard VM provisioning, achieving sub-10ms instantiation through copy-on-write techniques.

---

## 1. The OCI-to-Block Pipeline

Micro-VMs boot from raw block devices, whereas container registries store images as gzipped tarball layers. Vyoma bridges this gap through a Just-In-Time (JIT) conversion engine:

1. **Layer Download**: The daemon downloads layer blobs from the registry.
2. **Layer Unpacking**: Blobs are unpacked in sequence into an assembly directory, handling whiteout files (`.wh.*`) to mimic OverlayFS semantics.
3. **Image Generation**:
   - An empty disk file is allocated (e.g. `rootfs.ext4` or compressed `rootfs.sqfs`).
   - The filesystem is formatted (`mkfs.ext4` or `mksquashfs`).
   - The directory contents are copied into the image.
4. **Caching**: The resulting image is stored in `~/.vyoma/images/<image_hash>/` and marked read-only.

---

## 2. Instant Clones via Device Mapper

Copying a 2GB base image for every VM is too slow. Vyoma instead leverages the Linux Device Mapper (`dm-snapshot`):

```
┌──────────────────────────────────────┐
│       Base Image (Read-Only)         │
│   e.g., ~/.vyoma/images/alpine.ext4  │
│        (Mounted on /dev/loop0)       │
└──────────────────┬───────────────────┘
                   │
                   │ (Read requests)
                   ▼
┌──────────────────────────────────────┐        ┌─────────────────────────────┐
│       Device Mapper Virtual Disk     │        │      Sparse CoW Delta       │
│           /dev/mapper/vyoma-vm-123   │◄───────┤ ~/.vyoma/vms/vm-123/disk.cow│
│        (Passed to Cloud Hypervisor)  │ (Write)│      (Initial size: 0 KB)   │
└──────────────────────────────────────┘        └─────────────────────────────┘
```

### Mechanism:
1. The read-only base image is attached to a loop device (`/dev/loopX`).
2. A sparse Copy-on-Write (CoW) delta file (`disk.cow`) is allocated for the specific VM.
3. A Device Mapper table is constructed mapping the base loop device and the CoW device:
   ```bash
   dmsetup create vyoma-<id> --table "0 <sectors> snapshot /dev/loopX disk.cow P 8"
   ```
4. The resulting virtual block device (`/dev/mapper/vyoma-<id>`) is handed to Cloud Hypervisor as the root drive (`/dev/vda`).

**Result**: VM creation takes <10ms, and disk usage per VM starts at just a few kilobytes.

---

## 3. Snapshot Tree & Delta History

In earlier versions, snapshots were stored using git commits. Vyoma now uses a native **Snapshot Tree** engine backed by `sled`:
- Snapshots record the dirty memory state (`.mem`), hypervisor register states (`.snap`), and the filesystem CoW delta (`.cow`).
- Only blocks that have changed since the parent snapshot are persisted.
- Allows instant rollback (`vyoma time-travel --to snap:N`) and non-linear branching without cloning the base image.
