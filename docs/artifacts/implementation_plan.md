# Implementation Plan - Port Mapping (Phase 8)

## Goal
Allow developers to access services running inside the Micro-VM (e.g., port 80) via their localhost (e.g., port 8080).
Syntax: `ign run -p 8080:80 alpine:latest`

## Technical Design: Userspace TCP Proxy
Instead of complex `iptables` DNAT rules (which require tracking and cleanup on the host network stack), we will implement a lightweight userspace TCP proxy within the `ignited` daemon.

### Architecture
1.  **CLI**: Parse `-p host:vm` flag. Send mapping to Daemon in `RunRequest`.
2.  **Daemon**:
    *   Spawn a `tokio::net::TcpListener` on `0.0.0.0:<host_port>`.
    *   When a connection is received, open a `TcpStream` to the VM's internal IP (`172.16.0.X:<vm_port>`).
    *   Use `tokio::io::copy_bidirectional` to pump bytes between the two sockets.
    *   Store the "Proxy Task" handle in `VmInstance` to cancel it when the VM stops.

### Components Logic

#### 1. CLI (`ign/src/main.rs`)
*   Update `Run` command arguments to accept `-p` / `--port`.
*   Pass list of `PortMapping { host: u16, vm: u16 }` to API.

#### 2. Core (`ignite-core`)
*   Add `PortMapping` struct to `api.rs` (or equivalent shared type location).
*   Add `ProxyManager` struct in `network.rs` (or new module):
    *   `start_proxy(host_port, vm_ip, vm_port) -> JoinHandle`

#### 3. Daemon (`ignited/src/main.rs`)
*   Update `RunRequest` struct.
*   Inside `run_vm`:
    *   After VM network is UP and IP assigned (e.g., `172.16.0.X`), start proxies.
    *   Store handles in `VmInstance`.
*   Inside `stop_vm`:
    *   Abort proxy tasks to release host ports.

## Verification Plan
1.  **Run VM**: `ign run -p 8080:80 alpine:latest`.
2.  **Start Server**: Inside VM, run `nc -lk -p 80 -e echo "Hello from VM"`.
3.  **Test**: On host, run `curl localhost:8080`.
4.  **Result**: Should receive "Hello from VM".
