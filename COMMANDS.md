# Ignite CLI Command Reference 📚

This document lists all available commands for the Ignite CLI (`ign`).

## Core Lifecycle

### `ign run`
Run a new micro-VM from an image.
**Usage**: `ign run [OPTIONS] <IMAGE>`
**Options**:
- `--vcpu <N>`: Number of vCPUs (Default: 1).
- `--memory <MB>`: Memory size in MiB (Default: 512).
- `-p, --port <HOST>:<VM>`: Port mapping.
- `-v, --volume <HOST>:<VM>`: Volume mount (Requires `virtiofsd`).
- `--name <NAME>`: Custom hostname/name.
**Example**:
```bash
ign run alpine:latest --vcpu 2 --memory 1024 -p 8080:80
```

### `ign stop`
Stop a running VM gracefully.
**Usage**: `ign stop <ID>`
**Example**: `ign stop a1b2c3d4`

### `ign start`
Start a stopped VM (Resume execution). (Use `restart` to replace).
**Usage**: `ign start <ID>`

### `ign restart`
Stop and Restart a VM (Full reboot).
**Usage**: `ign restart <ID>`

### `ign ps`
List all active VMs.
**Usage**: `ign ps`
**Example Output**:
```
ID        IMAGE           IP            STATUS    UPTIME
a1b2c3    alpine:latest   172.16.0.5    Running   5m
```

### `ign logs`
Stream logs from a VM's serial console.
**Usage**: `ign logs [-f] <ID>`
**Example**: `ign logs -f web-server`

### `ign exec`
Execute a command inside a running VM.
**Usage**: `ign exec <ID> <COMMAND>`
**Example**: `ign exec web-server /bin/ls -la`

## Image Management

### `ign pull`
Pull an OCI image from a registry (Docker Hub).
**Usage**: `ign pull <IMAGE>`
**Example**: `ign pull nginx:alpine`

### `ign build`
Build a new image using an `Ignitefile`.
**Usage**: `ign build -t <TAG> <CONTEXT>`
**Example**: `ign build -t my-app:v1 .`

## Networking

### `ign network ls`
List available CNI networks.
**Usage**: `ign network ls`

### `ign network create`
Create a new bridge network.
**Usage**: `ign network create <NAME> --subnet <CIDR>`
**Example**: `ign network create backend --subnet 10.50.0.0/16`

## Swarm (Cluster)

### `ign swarm init`
Initialize this node as a Swarm Seed (Leader).
**Usage**: `ign swarm init`

### `ign swarm join`
Join an existing Swarm.
**Usage**: `ign swarm join <SEED_IP>`
**Example**: `ign swarm join 192.168.1.10`

### `ign swarm ls`
List nodes in the swarm.
**Usage**: `ign swarm ls`

## Snapshots & Teleportation

### `ign snapshot`
Create a snapshot of a VM.
**Usage**: `ign snapshot <ID>`
**Example**: `ign snapshot web-server`

### `ign restore`
Restore a VM from a snapshot ID.
**Usage**: `ign restore <SNAPSHOT_ID>`

### `ign export`
Export a snapshot to a tarball.
**Usage**: `ign export <SNAPSHOT_ID> <FILE>`
**Example**: `ign export snap_123 backup.tar`

### `ign import`
Import a VM from a snapshot tarball.
**Usage**: `ign import <FILE>`

## Orchestration (Ignite Compose)

### `ign up`
Create and start resources from `ignite-compose.yml`.
**Usage**: `ign up [-d]`
**Options**: `-d` (Detached mode).

### `ign down`
Stop and remove resources defined in `ignite-compose.yml`.
**Usage**: `ign down`

### `ign scale`
Scale a service to N replicas.
**Usage**: `ign scale <SERVICE>=<COUNT>`
**Example**: `ign scale web=3`

## System

### `ign doctor`
Check system health (KVM, Dependencies).
**Usage**: `ign doctor`

### `ign help`
Show help message.
**Usage**: `ign help` OR `ign <COMMAND> --help`
