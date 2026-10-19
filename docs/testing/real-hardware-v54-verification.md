# Real Hardware & KVM E2E Verification Report: Cloud Hypervisor v54.0

This report documents the end-to-end verification of Vyoma's migration to **Cloud Hypervisor v54.0** performed directly on physical hardware with hardware-accelerated Linux KVM (`/dev/kvm`).

---

## 1. System & Hardware Environment

- **Host Operating System**: Linux (Ubuntu x86_64)
- **KVM Device**: `/dev/kvm` (accessible, RW permissions for `subesh` in `kvm` group)
- **Hypervisor Binary**: `bin/cloud-hypervisor` (`v54.0`)
- **Kernel Image**: `bin/vmlinux` (Linux bzImage 6.16.9+, 8.0 MB)
- **Disk Tooling**: `qemu-img` 10.2.1

---

## 2. Test Execution & Verified Capabilities

The verification was executed using the automated test suite [`tests/e2e/test_v54_hardware_real_vm.sh`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/tests/e2e/test_v54_hardware_real_vm.sh) and end-to-end browser subagent testing on the web dashboard.

### Summary of Test Cases

| Step | Test Objective | Implementation Details | Status |
| :--- | :--- | :--- | :--- |
| **1** | KVM Device & CPU Virtualization | Validated presence and read/write access to `/dev/kvm`. | **PASS** |
| **2** | Hypervisor & Kernel Validation | Validated executable `bin/cloud-hypervisor` v54.0 and `bin/vmlinux`. | **PASS** |
| **3** | Native QCOW2 Overlay Engine | Created base QCOW2 and overlay with `-b base.qcow2 -F qcow2`. Device Mapper bypassed. | **PASS** |
| **4** | Daemon Startup & Event Socket | Spawned Cloud Hypervisor with `--api-socket` and `--event-monitor path=<file>`. | **PASS** |
| **5** | MicroVM Hardware Boot on KVM | Configured payload, cpus, memory, disks via API. Booted in 0.22s to `Running` state. | **PASS** |
| **6** | Real-Time Event Monitor | Captured events (`vmm.starting`, `vmm.started`, `vm.booting`, `virtio-device.activated`). | **PASS** |
| **7** | Live Migration Cancellation | Invocated `PUT /api/v1/vm.cancel-migration` on running VM via Unix socket. | **PASS** |
| **8** | Online Dynamic Disk Resizing | Invocated `PUT /api/v1/vm.resize-disk` on active standalone disk, expanding 32M -> 100M live. | **PASS** |
| **9** | Clean Shutdown | Triggered `PUT /api/v1/vmm.shutdown` and verified clean process termination. | **PASS** |
| **10** | Web Dashboard UI | Navigated UI via browser subagent: verified MicroVMs controls, Resize modal, Teleport cancel modal, and Event category filters. | **PASS** |

---

## 3. Key Findings & Architectural Learnings

1. **Native QCOW2 Storage Overlays**:
   - Cloud Hypervisor v54 requires explicit declaration of `"image_type": "Qcow2"` and `"backing_files": true` in disk configurations for overlays.
   - Eliminates kernel loop device exhaustion (`/dev/loop*`) and DM table limits.

2. **Online Disk Resizing**:
   - Dynamic disk resizing via `/vm.resize-disk` succeeds immediately on standalone disks (`raw` and `qcow2`).
   - Cloud Hypervisor upstream returns `"resize not supported with backing files"` when executed against an overlay image with an active backing chain. The UI and CLI inform operators of this upstream constraint.

3. **Multi-Line Event Monitor Stream**:
   - The `--event-monitor` flag outputs structured JSON objects across multiple lines with timestamps (`secs`, `nanos`) and device IDs.
   - Vyoma's `VmmManager` event reader was upgraded to parse JSON brace-depth stream buffers, correctly streaming events to `/events` SSE.

---

## 4. UI State & Verification Assets

- **Browser Subagent Recording**: Recorded interactive verification of dashboard, MicroVMs table, and Event stream filter categories.
- **Vite Production Bundle**: Compiled clean with zero errors via `tsc -b && vite build`.
- **Vitest Unit Tests**: All frontend unit tests passed (3/3 test suites, 7/7 tests).
