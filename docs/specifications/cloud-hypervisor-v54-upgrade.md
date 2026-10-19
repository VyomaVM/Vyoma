# Technical Specification: Upgrading to Cloud Hypervisor v54.0

**Status**: In Progress  
**Target Release**: Vyoma v3.0.0  
**Target Upstream**: Cloud Hypervisor `v54.0` (Statically Linked x86_64 Binary)  

---

## 1. Executive Summary

This specification governs the architectural upgrade of the core Virtual Machine Monitor (VMM) in the Vyoma ecosystem from **Cloud Hypervisor v41.0.0** to **Cloud Hypervisor v54.0**. 

Cloud Hypervisor v54.0 introduces major advancements in **live migration resilience**, **storage backends**, **event streaming**, **security**, and **snapshot performance**. This upgrade enables Vyoma to eliminate legacy operational pain points (such as kernel Device Mapper leaks, ghost loop disks, and migration timeout hangs) while significantly improving boot speeds and reliability.

---

## 2. Upstream Feature Analysis: Cloud Hypervisor v41 vs v54.0

### 2.1 Live Migration & Teleportation
* **Migration Cancellation API (`vm.cancel-migration`)**: Cloud Hypervisor v54 introduces `PUT /api/v1/vm.cancel-migration` (CLI: `ch-remote cancel-migration`). When an active live migration fails to converge or network latency spikes, the migration can be safely aborted without killing the source VM.
* **Postcopy Migration (`userfaultfd`)**: Complements the existing precopy protocol. The VM immediately switches over to the destination host, and memory pages are fetched on-demand from the source over the network.
* **Native mTLS Encryption**: Cloud Hypervisor v53+ supports mutual TLS encryption for the migration stream directly, securing data transfers without needing an external WireGuard wrapper.
* **VFIO Device Live Migration**: Supports live migration of VMs with hardware devices attached (e.g., SR-IOV NICs with `mlx5_vfio_pci` and `iommufd`).
* **`n-2` Version Compatibility**: Guarantees migration compatibility between `v54.0` and earlier versions (`v52.0`, `v53.0`), enabling seamless rolling upgrades across a Vyoma Swarm.

### 2.2 Storage Backends & Disk Formats
* **Native QCOW2 with Backing Files**: Natively handles thin-provisioned copy-on-write disks referencing read-only base images.
* **Flat VMDK Support**: Supports `monolithicFlat` and `twoGbMaxExtentFlat` disk descriptor formats (`--disk image_type=vmdk`).
* **Dynamic Disk Resizing (`/vm.resize-disk`)**: Expands disk capacity for running VMs without stopping the guest.
* **Byte-Range File Locking**: Replaced whole-file advisory locking with byte-range locks for improved compatibility with distributed file systems (NFS, GlusterFS).

### 2.3 Event Monitor Engine
* **Structured Event Streaming (`--event-monitor`)**: Cloud Hypervisor provides a machine-readable JSON event stream over a dedicated file descriptor or named pipe (`path=<path>|fd=<fd>`). Emits events for `vmm.started`, `vm.paused`, `vm.resumed`, `vm.migrating`, `vm.migration-completed`, and `vm.migration-cancelled`.

### 2.4 Snapshot & Restore Performance
* **Offloaded Snapshot Daemon**: Snapshot and restore workloads can be delegated to an external daemon with on-demand paging (`userfaultfd`), enabling near-instant cold resumes for large-memory VMs.
* **Optimized Dirty Bitmaps**: Greatly reduces memory tracking overhead during intensive guest writes.

---

## 3. Vyoma Architectural Evolution

### 3.1 Storage Layer: Transition from Device Mapper to Native QCOW2 Backing Files
* **Previous Pain**: Vyoma's instant clones used Linux Device Mapper (`dmsetup`) and loop devices (`/dev/loopX`). If the daemon crashed or was SIGKILLed, loop devices remained attached, creating "Ghost Disks" on the host.
* **New Design**: Implement native QCOW2 backing files. The read-only base image is `base.qcow2`, and each VM allocates `vm.qcow2` pointing to the base. 
* **Benefits**:
  - No root or `CAP_SYS_ADMIN` required for loop mounting.
  - Zero ghost disks or leaked loop devices.
  - Built-in compression and thin provisioning.

### 3.2 Teleportation: Safe Abort & Telemetry
* Add `cancel_migration` to [`crates/vyoma-teleport`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-teleport) to invoke `PUT /api/v1/vm.cancel-migration`.
* Support postcopy mode for write-heavy workloads that fail precopy convergence.

### 3.3 Event Streaming Engine
* Configure `vyomad` to launch Cloud Hypervisor with `--event-monitor fd=X`.
* Replace 500ms/5s sleep polling loops with Tokio asynchronous stream listeners.

---

## 4. Phase-by-Phase Implementation Plan

### Phase 1: Binary, Packaging & Verification
- **Branch**: `feat/ch-v54-packaging`
- **Scope**:
  - Download and bundle `cloud-hypervisor-static` v54.0 into `bin/cloud-hypervisor`.
  - Update `packaging/deb/build.sh` and `packaging/rpm/build.sh` with the v54.0 static URL and verified SHA256 checksum (`4f4f98a3d661a40f3e5272627cc895a1b2e525f2f6bedf5f895f9cd21ba5c65b`).
  - Verify binary execution and version detection in `vyoma doctor`.
- **Deliverables**: Updated packaging scripts, bundled binary, and package build verification.

### Phase 2: Event Monitor Engine
- **Branch**: `feat/ch-v54-event-monitor`
- **Scope**:
  - Add `--event-monitor` argument configuration in `vyoma-core` VMM launcher.
  - Implement async JSON event parser in `vyomad`.
  - Hook event stream to the VM lifecycle FSM, replacing polling in `vmm.rs`.
- **Deliverables**: Event monitor listener module, integration tests.

### Phase 3: Teleportation Enhancements & Migration Cancellation
- **Branch**: `feat/ch-v54-migration-enhancements`
- **Scope**:
  - Implement `cancel_migration` endpoint in `crates/vyoma-teleport` and `vyomad`.
  - Add timeout safety: if migration takes >5 minutes without converging, trigger automatic cancellation.
  - Expose `vyoma teleport cancel <vm-id>` in the CLI.
- **Deliverables**: Cancellation API, postcopy options, unit/integration tests.

### Phase 4: Storage Modernization & QCOW2 Backing Files
- **Branch**: `feat/ch-v54-qcow2-storage`
- **Scope**:
  - Implement QCOW2 backing file provisioner in `crates/vyoma-storage`.
  - Add `/vm.resize-disk` API wrapper for dynamic disk resizing.
  - Enable running without Device Mapper when QCOW2 is selected.
- **Deliverables**: QCOW2 storage driver, disk resize API, storage unit tests.

### Phase 5: GitHub CI/CD & Test Automation Alignment
- **Branch**: `feat/ch-v54-ci-cd`
- **Scope**:
  - Update `.github/workflows/smoke-test.yml` and `release.yml`.
  - Verify package creation, dependencies, and automated test passes.
- **Deliverables**: Green CI pipeline on Ubuntu runners.

### Phase 6: Integration, Final Smoke Tests & Release Merge
- **Branch**: `develop` → `main`
- **Scope**:
  - Run full test suite (`cargo test --workspace`).
  - Merge `develop` into `main` using `--no-ff`.
  - Tag release `v3.0.0`.

---

## 5. Git Workflow & Commit Guidelines

1. **Base Branch**: `develop`
2. **Feature Branch Pattern**: `feat/ch-v54-<component>`
3. **Commit Standards**: Conventional Commits:
   - `feat(packaging): upgrade Cloud Hypervisor to v54.0 static binary`
   - `feat(vmm): implement event monitor push notification stream`
   - `feat(teleport): add migration cancellation and postcopy support`
   - `feat(storage): support native QCOW2 backing files and disk resize`
   - `ci: update workflows and checksum verification for v54.0`
4. **Merge Strategy**: `--no-ff` into `develop`, tested before final merge into `main`.
