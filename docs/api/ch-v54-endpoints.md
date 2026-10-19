# Cloud Hypervisor v54.0 API Endpoints & Vyoma Integration

This document outlines the API endpoints and CLI commands introduced and updated for Cloud Hypervisor v54.0 integration within the Vyoma daemon (`vyomad`) and CLI (`vyoma`).

---

## 1. Live Migration Cancellation

Cloud Hypervisor v54.0 natively supports aborting in-flight live migrations via its internal API endpoint:
`PUT /api/v1/vm.cancel-migration`.

### Daemon HTTP Endpoints

#### `POST /teleport/cancel/:id`
Aborts an in-progress teleportation/migration session identified by either the migration `session_id` or `vm_id`.

- **Path Parameters**:
  - `id`: Unique session identifier or target VM ID.
- **Response**:
  ```json
  {
    "success": true,
    "message": "Migration for VM vm-01 cancelled successfully",
    "session_id": "8b52f9b1-58dc-4c4f-96a2-6eb6d3d92031",
    "vm_id": "vm-01"
  }
  ```

#### `POST /teleport/cancel`
Alternative endpoint accepting JSON payload:
- **Request Body**:
  ```json
  {
    "session_id": "8b52f9b1-58dc-4c4f-96a2-6eb6d3d92031",
    "vm_id": "vm-01"
  }
  ```

### CLI Command

```bash
vyoma cancel-teleport <vm_id_or_session_id>
```

---

## 2. Dynamic Disk Resizing

Cloud Hypervisor v54.0 allows live volume resizing via `PUT /api/v1/vm.resize-disk`.

### Daemon HTTP Endpoints

#### `POST /vms/:id/resize-disk` and `POST /vms/:id/resize`
Requests an online expansion of a disk drive attached to a running VM.

- **Path Parameters**:
  - `id`: VM ID.
- **Request Body**:
  ```json
  {
    "disk_id": "rootfs",
    "desired_size": 21474836480
  }
  ```
  *(Default `disk_id` is `"rootfs"` if omitted).*
- **Response**:
  ```json
  {
    "success": true,
    "vm_id": "vm-01",
    "disk_id": "rootfs",
    "desired_size": 21474836480,
    "message": "Disk resized successfully"
  }
  ```

> [!NOTE]
> **Upstream Cloud Hypervisor Limitation**:
> Online disk resizing via `/vm.resize-disk` requires standalone disk images (raw or standalone qcow2). Cloud Hypervisor returns `"resize not supported with backing files"` when executed against an overlay with an active backing chain.

### CLI Command

```bash
# Resize with human-friendly units (supports K, M/MB/MiB, G/GB/GiB, or raw bytes):
vyoma resize-disk vm-01 10G
vyoma resize-disk vm-01 2048M --disk-id rootfs
```

---

## 3. Real-Time Event Monitor Stream

Cloud Hypervisor v54 provides a line-delimited JSON event logging facility initiated via `--event-monitor path=<file>`.

### Integration Flow
1. **Daemon Spawning**: `vyomad` automatically initializes a dedicated event monitor socket at `{vm_dir}/ch.sock.events`.
2. **Asynchronous Collector**: An event reader daemon thread parses incoming lines into strongly typed `VmmEvent` records.
3. **SSE Broadcast**: Events are bridged to `state.events_tx` and streamed live to all web clients connected to `GET /events`.

### Event Payload Format
```json
{
  "type": "ch_vmm_event",
  "vm_id": "vm-01",
  "source": "vmm",
  "event": "resumed",
  "timestamp": {
    "secs": 1774005820,
    "nanos": 128450000
  },
  "properties": null
}
```

Common event types captured:
- `started`: MicroVM successfully booted.
- `paused`: VCPUs suspended.
- `resumed`: VCPUs resumed execution.
- `shutdown`: Guest OS executed clean poweroff or ACPI shutdown.
- `migration_cancelled`: Active migration successfully aborted.
