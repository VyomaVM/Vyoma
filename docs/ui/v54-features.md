# Cloud Hypervisor v54.0 Web UI Integration

This document describes the web UI components and hooks built to support Cloud Hypervisor v54.0 features in the Vyoma management dashboard.

---

## 1. MicroVM Management (`ui/src/features/vms/VmsPage.tsx`)

The MicroVM table was upgraded to provide direct, one-click access to Cloud Hypervisor v54's advanced virtualization capabilities.

### New Actions & Controls

1. **Virtual Disk Resizing (`ResizeDiskDialog`)**:
   - Accessible via the **HardDrive** action icon on any running VM row.
   - Allows users to specify the block device identifier (defaults to `rootfs`) and the target size in Gigabytes.
   - Computes exact byte counts live and invokes `POST /vms/:id/resize-disk`.
   - Displays informational notes regarding standalone virtual disk constraints.

2. **Live Teleportation & In-Flight Cancellation (`TeleportDialog`)**:
   - Accessible via the **Send** action icon on any running VM row.
   - Collects the target node IP/hostname and optional bandwidth constraints in Mbps.
   - Upon initiation, displays the unique Migration Session ID and monitors real-time status.
   - **In-Flight Cancellation**: Features a prominent **"Cancel Migration"** action button that invokes `POST /teleport/cancel/:id`, allowing operators to abort in-flight memory migrations without disrupting or terminating the source MicroVM.

---

## 2. Real-Time Hypervisor Event Stream (`ui/src/features/events/EventsPage.tsx`)

Cloud Hypervisor v54.0 generates structured line-delimited events via `--event-monitor path=<file>`. These events are piped through the daemon's Server-Sent Events (SSE) `/events` stream and visualized in the UI.

### Features
- **Category Filtering**: Quick filter pills for:
  - `All Events`: Global unified stream.
  - `CH v54 VMM`: Dedicated filter for Cloud Hypervisor hardware events (`ch_vmm_event`).
  - `Lifecycle`: VM startup and stop operations.
  - `Teleport`: Live migration dispatches and cancellations.
  - `Disk`: Dynamic disk resize operations.
- **Visual Status Badges**:
  - `started` / `resumed`: Green indicator.
  - `paused`: Yellow indicator.
  - `migration_cancelled`: Amber/red abort indicator.
  - `shutdown`: Neutral gray indicator.
- **Search & Auto-Scroll**: Live text filtering and toggleable auto-scroll lock.

---

## 3. Query & Mutation Hooks (`ui/src/hooks/queries/useVmList.ts`)

New TanStack Query mutations available across the UI:
- `resizeDisk`: Triggers dynamic volume expansion.
- `teleportVm`: Initiates multi-phase iterative live migration.
- `cancelTeleport`: Aborts migration using native hypervisor primitives.
