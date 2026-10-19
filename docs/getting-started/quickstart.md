# Quickstart Guide

This guide walks you through launching your first Micro-VM, mapping ports, mounting host volumes, executing commands, and monitoring VM state.

---

## 1. Pulling an Image

Vyoma pulls standard OCI images directly from Docker Hub or any private container registry, converting them on-the-fly into bootable ext4/SquashFS filesystems:

```bash
vyoma pull alpine:latest
vyoma pull nginx:alpine
vyoma pull ubuntu:22.04
```

---

## 2. Running Your First Micro-VM

Launch an Alpine Micro-VM:

```bash
vyoma run alpine:latest --name my-alpine --vcpu 1 --memory 512
```

### Parameters
- `--name <NAME>`: Custom name/hostname for the VM.
- `--vcpu <N>`: Number of virtual CPUs (Default: `1`).
- `--memory <MB>`: RAM allocation in MiB (Default: `512`).

---

## 3. Port Forwarding

Publish VM ports to the host machine using `-p <host_port>:<vm_port>`:

```bash
vyoma run nginx:alpine --name web-server -p 8080:80
```

Verify in your browser or with curl:
```bash
curl http://localhost:8080
```

---

## 4. Mounting Host Volumes

Share host directories into the VM using `-v <host_path>:<vm_path>` (backed by high-performance `virtiofs`):

```bash
# Ensure host directory is readable by the vyoma group
mkdir -p ./html
echo "<h1>Hello from Vyoma Micro-VM</h1>" > ./html/index.html

# Launch VM with volume mount
vyoma run nginx:alpine -p 8080:80 -v $(pwd)/html:/usr/share/nginx/html
```

---

## 5. VM Management & Lifecycle

### Listing Running VMs

```bash
vyoma ps
```

Example output:
```text
ID        IMAGE           IP            STATUS    UPTIME
a1b2c3    alpine:latest   172.16.0.5    Running   2m
d4e5f6    nginx:alpine    172.16.0.6    Running   45s
```

### Executing Commands Inside the VM

Vyoma uses an integrated in-VM agent channel rather than requiring SSH setup:

```bash
vyoma exec web-server /bin/sh -c "hostname -I && uname -a"
```

### Streaming Logs

Stream serial console and kernel output:

```bash
vyoma logs -f web-server
```

### Real-Time Resource Stats

Monitor live CPU and RAM consumption:

```bash
vyoma stats web-server
```

### Stopping and Removing VMs

```bash
# Graceful stop
vyoma stop web-server

# Remove VM and clean up all backing block devices, loop mounts, and TAP interfaces
vyoma rm web-server
```
