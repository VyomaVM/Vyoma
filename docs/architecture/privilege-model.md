# Security & Privilege Model

To prevent host breakout vulnerabilities, Vyoma implements strict privilege separation following the principle of least privilege.

---

## 1. The Service Model

Earlier designs ran the daemon as full `root`. While hypervisor operations require kernel capabilities, running the entire REST and gRPC API server under `root` is dangerous.

Vyoma isolates system components using systemd capability bounding sets:

```
┌────────────────────────────────────────────────────────┐
│                   CLI (`vyoma`)                        │
│            Runs as Standard Unprivileged User          │
│                (Member of `vyoma` group)               │
└───────────────────────────┬────────────────────────────┘
                            │
               Unix Socket: /run/vyoma/vyoma.sock
               Permissions: 0660 (vyoma:vyoma)
                            │
┌───────────────────────────▼────────────────────────────┐
│                  Daemon (`vyomad`)                     │
│                User: vyoma | Group: vyoma              │
│                                                        │
│  AmbientCapabilities:                                  │
│    - CAP_NET_ADMIN   (Manage TAP/Bridges/WireGuard)    │
│    - CAP_SYS_ADMIN   (Device Mapper & Cgroups v2)      │
│    - CAP_NET_RAW     (Raw packet routing / VXLAN)      │
│    - CAP_SETUID      (Process credential dropping)     │
│    - CAP_SETGID      (Group management)                │
└────────────────────────────────────────────────────────┘
```

---

## 2. Systemd Service Definition

The daemon service is managed at `/etc/systemd/system/vyomad.service`:

```ini
[Unit]
Description=Vyoma Micro-VM Daemon
After=network.target

[Service]
Type=simple
User=vyoma
Group=vyoma
ExecStart=/usr/bin/vyomad
Restart=always
RestartSec=3

# Kernel capability constraints
AmbientCapabilities=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_RAW CAP_SETUID CAP_SETGID
CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_NET_RAW CAP_SETUID CAP_SETGID

# Filesystem protections
ProtectSystem=full
ProtectHome=read-only
ReadWritePaths=/var/lib/vyoma /run/vyoma

[Install]
WantedBy=multi-user.target
```

---

## 3. Post-Install User Permissions

Non-root users interact with the daemon by joining the `vyoma` group:

```bash
sudo usermod -aG vyoma $USER
```

When a user in the `vyoma` group sends commands over `/run/vyoma/vyoma.sock`, the daemon verifies the connecting credentials via socket peer credentials (`SO_PEERCRED`), preventing privilege escalation.
