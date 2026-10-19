# In-VM Agent (`vyoma-agent-vm`)

Vyoma embeds a lightweight, statically compiled agent inside every Micro-VM, providing host introspection and execution without requiring SSH.

---

## 1. Capabilities

The agent binary ([`crates/vyoma-agent-vm`](file:///home/subesh/Startup_Projects/Vyoma/vyoma/crates/vyoma-agent-vm)) is compiled using the `x86_64-unknown-linux-musl` target to produce a static ~400KB binary with zero runtime dependencies.

Supported capabilities:
- **Process Listing (`ProcessList`)**: Returns all running guest processes, PIDs, and memory footprints.
- **Resource Telemetry (`GetMetrics`)**: Samples CPU utilization, memory pressure, and load averages.
- **Direct Command Execution (`ExecCommand`)**: Runs commands synchronously or streaming within the guest environment (`vyoma exec`).
- **File Read (`FileRead`)**: Reads guest files directly (e.g. `/etc/resolv.conf`, `/proc/version`) for debugging.

---

## 2. Communication Channel

The agent communicates with the host `vyomad` daemon over **virtio-vsock** (or private TCP port `9999` in development):

```
┌─────────────────────────────────┐
│           Guest VM              │
│                                 │
│      vyoma-agent-vm             │
│            ▲                    │
│            │ (vsock port 9999)  │
└────────────┼────────────────────┘
             │ virtio-vsock
┌────────────┼────────────────────┐
│            ▼                    │
│         vyomad                  │
│       (Host Daemon)             │
└─────────────────────────────────┘
```

### JSON Protocol

Requests and responses are serialized using newline-delimited JSON:

```json
{"type": "ProcessList"}
{"type": "GetMetrics"}
{"type": "ExecCommand", "cmd": ["ls", "-la", "/var/log"], "env": {}, "workdir": "/"}
```
