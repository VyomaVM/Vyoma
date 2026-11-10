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

## 2. Instant Clones via Native QCOW2 Overlays

Copying a 2GB base image for every VM is too slow. Rather than kernel Device Mapper (`dm-snapshot`) and loop devices (`/dev/loopX`), Vyoma v3.0+ uses **Cloud Hypervisor v54.0 native QCOW2 copy-on-write overlays**:

```
┌──────────────────────────────────────┐
│    Base Image (Read-Only ext4/sqfs)  │
│  ~/.vyoma/images/<img_hash>/rootfs.* │
└──────────────────┬───────────────────┘
                   │ (backing_file)
                   ▼
┌──────────────────────────────────────┐
│       Thin QCOW2 Overlay Disk        │
│    ~/.vyoma/vms/vm-123/root.qcow2    │
│    (Initial size: ~196 KB, Rootless) │
└──────────────────────────────────────┘
```

### Mechanism:
1. The unpacked OCI filesystem is formatted once into a read-only base image (`rootfs.ext4` or `rootfs.sqfs`).
2. When launching a VM, `vyomad` creates a thin QCOW2 overlay (`root.qcow2`) referencing the base image as its backing file.
3. Cloud Hypervisor attaches `root.qcow2` with `"image_type": "Qcow2"` and `"backing_files": true`, avoiding kernel loop devices, `dmsetup`, and `CAP_SYS_ADMIN`.
4. Deleting a VM simply removes `root.qcow2`, leaving zero orphaned kernel block devices.

See [QCOW2 Storage Engine Architecture](qcow2-storage-engine.md) for full details.

---

## 3. Snapshot Tree & Delta History

In earlier versions, snapshots were stored using git commits. Vyoma now uses a native **Snapshot Tree** engine backed by `sled`:
- Snapshots record the dirty memory state (`.mem`), hypervisor register states (`.snap`), and the filesystem CoW delta (`.cow`).
- Only blocks that have changed since the parent snapshot are persisted.
- Allows instant rollback (`vyoma time-travel --to snap:N`) and non-linear branching without cloning the base image.
