# Initramfs Agent & Boot Injection Architecture

Vyoma uses a dynamic in-memory **gzipped CPIO initramfs archive** to inject system init scripts and binaries into guest VMs without modifying or mounting the underlying block device.

---

## 1. Motivation: Replacing Mount-Based Injection

In earlier designs, injecting `/sbin/vyoma-init` and agent binaries required:
1. Mounting the VM's disk image on the host.
2. Copying files into `/sbin/`.
3. Unmounting the disk before handing it to the hypervisor.

### Critical Problems Solved:
- **Mount Races**: Parallel VM launches frequently collided on host mount points.
- **Root Requirement**: Mounting filesystem images required elevated host privileges.
- **Latency**: Mount/unmount added ~500ms of overhead per VM boot.

---

## 2. The Initramfs Approach

Before launching Cloud Hypervisor, the daemon dynamically creates `initramfs.cpio.gz` inside the VM's runtime directory:

```
┌────────────────────────────────────────────────────────┐
│               initramfs.cpio.gz Structure              │
├────────────────────────────────────────────────────────┤
│  /init                 (Wrapper script)                │
│  /sbin/vyoma-init      (Dynamic boot script: ENV, CMD) │
│  /sbin/vyoma-agent-vm  (Static Musl guest agent binary)│
└────────────────────────────────────────────────────────┘
```

1. **Generation**: Constructed in memory via `cpio::newc::Builder` and compressed with `flate2::GzEncoder`.
2. **Payload Delivery**: Passed to Cloud Hypervisor via `PayloadConfig::initramfs`.
3. **Boot Execution**: The guest Linux kernel uncompresses the initramfs into RAM and executes `/init`, which sets up environment variables, mounts `/dev/vda` as root, and invokes the in-VM agent and container entrypoint.

**Performance Impact**: Reduces boot injection time from ~500ms to <15ms.
