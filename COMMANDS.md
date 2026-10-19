# Vyoma CLI Command Reference 📚

This document lists all available commands for the Vyoma CLI (`vyoma`).

## Core Lifecycle

### `vyoma run`
Run a new micro-VM from an image.
**Usage**: `vyoma run [OPTIONS] <IMAGE>`
**Options**:
- `--vcpu <N>`: Number of vCPUs (Default: 1).
- `--memory <SIZE>`: Memory size in human-readable units or MiB (e.g. `512M`, `1G`, `2GiB`, or raw MiB digits `1024`, Default: `512`).
- `-p, --port <HOST>:<VM>`: Port mapping.
- `-v, --volume <HOST>:<VM>`: Volume mount (Requires `virtiofsd`).
- `--name <NAME>`: Custom hostname/name.
**Example**:
```bash
vyoma run alpine:latest --vcpu 2 --memory 1G -p 8080:80
```

### `vyoma stop`
Stop a running VM gracefully.
**Usage**: `vyoma stop <ID>`
**Example**: `vyoma stop a1b2c3d4`

### `vyoma start`
Start a stopped VM (Resume execution). (Use `restart` to replace).
**Usage**: `vyoma start <ID>`

### `vyoma restart`
Stop and Restart a VM (Full reboot).
**Usage**: `vyoma restart <ID>`

### `vyoma ps`
List all active VMs.
**Usage**: `vyoma ps`
**Example Output**:
```
ID        IMAGE           IP            STATUS    UPTIME
a1b2c3    alpine:latest   172.16.0.5    Running   5m
```

### `vyoma inspect`
Inspect low-level details and configuration of a VM (like `docker inspect`) in structured JSON.
**Usage**: `vyoma inspect <ID>`
**Example**:
```bash
vyoma inspect a1b2c3
```
**Example Output**:
```json
{
  "id": "a1b2c3...",
  "status": "running",
  "ip_address": "172.16.0.5",
  "vcpu": 1,
  "mem_size_mib": 512,
  "tap_name": "tapa1b2c3",
  "ports": [],
  "volumes": []
}
```

### `vyoma logs`
Stream logs from a VM's serial console.
**Usage**: `vyoma logs [-f] <ID>`
**Example**: `vyoma logs -f web-server`

### `vyoma exec`
Execute a command inside a running VM.
**Usage**: `vyoma exec <ID> <COMMAND>`
**Example**: `vyoma exec web-server /bin/ls -la`

## Image Management

### `vyoma pull`
Pull an OCI image from a registry (Docker Hub).
**Usage**: `vyoma pull <IMAGE>`
**Example**: `vyoma pull nginx:alpine`

### `vyoma build`
Build a new image using an `Vyomafile`.
**Usage**: `vyoma build -t <TAG> <CONTEXT>`
**Example**: `vyoma build -t my-app:v1 .`

## Networking

### `vyoma network ls`
List available CNI networks.
**Usage**: `vyoma network ls`

### `vyoma network create`
Create a new bridge network.
**Usage**: `vyoma network create <NAME> --subnet <CIDR>`
**Example**: `vyoma network create backend --subnet 10.50.0.0/16`

## Swarm (Cluster)

### `vyoma swarm init`
Initialize this node as a Swarm Seed (Leader).
**Usage**: `vyoma swarm init`

### `vyoma swarm join`
Join an existing Swarm.
**Usage**: `vyoma swarm join <SEED_IP>`
**Example**: `vyoma swarm join 192.168.1.10`

### `vyoma swarm ls`
List nodes in the swarm.
**Usage**: `vyoma swarm ls`

## Snapshots & Teleportation

### `vyoma snapshot`
Create a snapshot of a VM.
**Usage**: `vyoma snapshot <ID> [--name <NAME>]`
**Options**:
- `--name <NAME>`: Optional human-readable tag or name for the snapshot.
**Example**:
```bash
vyoma snapshot my-vm --name before-deploy
```

### `vyoma restore`
Restore a VM to a previous snapshot (by tag name, snapshot ID, or `snap:N` reference).
**Usage**: `vyoma restore <ID> --from <SNAPSHOT>`
**Options**:
- `--from <SNAPSHOT>`: Snapshot tag name (e.g. `before-deploy`), snapshot ID, or reference (e.g. `snap:1`).
**Example**:
```bash
vyoma restore my-vm --from before-deploy
```

### `vyoma time-travel`
Time travel a VM to a specific snapshot (alias for `vyoma restore`).
**Usage**: `vyoma time-travel <ID> --to <SNAPSHOT>`
**Example**:
```bash
vyoma time-travel my-vm --to before-deploy
```

### `vyoma branch`
Branch an existing snapshot into a new independent MicroVM backed by copy-on-write QCOW2 storage.
**Usage**: `vyoma branch <ID> --from <SNAPSHOT> <NEW_NAME>`
**Options**:
- `--from <SNAPSHOT>`: Snapshot tag name (e.g. `before-deploy`), snapshot ID, or reference (e.g. `snap:6`).
**Example**:
```bash
vyoma branch my-db --from snap:6 experiment
```

### `vyoma export`
Export a snapshot to a tarball.
**Usage**: `vyoma export <SNAPSHOT_ID> <FILE>`
**Example**: `vyoma export snap_123 backup.tar`

### `vyoma import`
Import a VM from a snapshot tarball.
**Usage**: `vyoma import <FILE>`

## Orchestration (Vyoma Compose)

Vyoma automatically discovers compose files in the current working directory in priority order:
`vyoma-compose.yml`, `vyoma-compose.yaml`, `compose.yaml`, `compose.yml`, `docker-compose.yml`, `docker-compose.yaml`.
Existing `docker-compose.yml` stacks work completely unchanged!

### `vyoma up`
Create and start resources from a compose file.
**Usage**: `vyoma up [OPTIONS]`
**Options**:
- `-f, --file <FILE>`: Explicit compose file path (auto-discovered if omitted).
- `-d, --detach`: Run services in the background (detached mode).
**Example**:
```bash
# docker-compose.yml works unchanged
vyoma up -d
```

### `vyoma down`
Stop and remove resources defined in the compose file.
**Usage**: `vyoma down [-f <FILE>]`
**Example**:
```bash
vyoma down
```

### `vyoma scale`
Scale a service to N replicas.
**Usage**: `vyoma scale <SERVICE>=<COUNT> [-f <FILE>]`
**Example**:
```bash
vyoma scale web=3
```

## System

### `vyoma doctor`
Check system health (KVM, Dependencies).
**Usage**: `vyoma doctor`

### `vyoma help`
Show help message.
**Usage**: `vyoma help` OR `vyoma <COMMAND> --help`
