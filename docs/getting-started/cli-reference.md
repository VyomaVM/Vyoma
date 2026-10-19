# Vyoma CLI Reference

The `vyoma` command-line utility provides complete control over the Micro-VM lifecycle, networking, clustering, images, and state operations.

---

## Command Overview

| Category | Commands |
|----------|----------|
| **VM Lifecycle** | `run`, `stop`, `start`, `restart`, `ps`, `rm`, `logs`, `exec`, `stats`, `inspect` |
| **Image Management** | `pull`, `build`, `commit`, `save`, `load` |
| **Networking** | `network ls`, `network create`, `network rm` |
| **Swarm & Cluster** | `swarm init`, `swarm join`, `swarm ls` |
| **Snapshots & TimeMachine** | `snapshot`, `restore`, `export`, `import`, `history`, `time-travel` |
| **Orchestration** | `up`, `down`, `scale` |
| **System** | `doctor`, `help`, `--version` |

---

## 1. Lifecycle Commands

### `vyoma run [OPTIONS] <IMAGE>`
Start a new Micro-VM.
- `--vcpu <N>`: vCPU limit (default: `1`).
- `--memory <MB>`: Memory in MiB (default: `512`).
- `-p, --port <HOST>:<VM>`: Forward host port to VM.
- `-v, --volume <HOST>:<VM>`: Share directory via VirtioFS.
- `--name <NAME>`: Custom VM name / hostname.
- `--network <NET>`: Connect to specific CNI network.

### `vyoma stop <ID>`
Send shutdown signal to the VM and unmount active resources.

### `vyoma start <ID>`
Resume execution of a stopped VM.

### `vyoma restart <ID>`
Perform a full reboot of the VM.

### `vyoma ps`
List all active and monitored VMs with IP address and uptime.

### `vyoma rm <ID>`
Remove VM, freeing Device Mapper snapshot, CoW sparse disk, and network interfaces.

### `vyoma logs [-f] <ID>`
Stream console and serial logs from the VM. Use `-f` to follow stream.

### `vyoma exec <ID> <COMMAND...>`
Run a command synchronously inside the target VM via the guest agent.

### `vyoma stats <ID>`
Print real-time CPU percentage and memory utilization.

---

## 2. Image Commands

### `vyoma pull <IMAGE>`
Fetch an OCI image from Docker Hub or a configured registry and convert to a local base image.

### `vyoma build -t <TAG> <CONTEXT>`
Build a new image using a `Vyomafile` located in `<CONTEXT>`.

### `vyoma commit <ID> <NEW_IMAGE_TAG>`
Create a new reusable base image from a modified VM's current disk state.

---

## 3. Network Commands

### `vyoma network ls`
List available CNI networks and subnet allocations.

### `vyoma network create <NAME> --subnet <CIDR>`
Create a new isolated bridge network with custom IPAM range.

### `vyoma network rm <NAME>`
Remove a user-defined network.

---

## 4. Swarm Commands

### `vyoma swarm init [--advertise-addr <IP>]`
Initialize this host as the cluster leader and start the Raft state machine.

### `vyoma swarm join <LEADER_IP>`
Join an existing Vyoma Swarm cluster, set up VXLAN overlay, and exchange WireGuard keys.

### `vyoma swarm ls`
List all active nodes in the cluster with status and subnets.

---

## 5. Snapshots & TimeMachine

### `vyoma snapshot <ID> [--label <LABEL>]`
Capture RAM state and filesystem changes to the snapshot tree.

### `vyoma restore <SNAPSHOT_ID>`
Restore a VM from an existing snapshot point.

### `vyoma history <ID>`
View the full timeline of snapshots and deltas taken for a VM.

### `vyoma time-travel <ID> --to snap:N`
Roll back the VM runtime state instantly to snapshot index `N`.

---

## 6. Compose Commands

### `vyoma up [-d]`
Deploy multi-VM stacks defined in `vyoma-compose.yml`. Use `-d` for detached mode.

### `vyoma down`
Tear down all services and networks declared in the compose file.

### `vyoma scale <SERVICE>=<COUNT>`
Scale a compose service horizontally to `<COUNT>` replicas.
