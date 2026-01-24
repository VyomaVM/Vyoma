# Ignite Daemon API Specification (v0.8+)

## Overview
REST + Streaming API for managing MicroVMs.
Base URL: `http://localhost:3000`

## Core Resources

### 1. VM Management (`/vms`)
*   `GET /ps` -> List running VMs.
*   `GET /vms/:id` -> Inspect details.
*   `POST /run` -> Create/Start VM.
*   `POST /stop/:id`, `POST /start/:id`, `POST /restart/:id`.
*   `DELETE /vms/:id` -> Remove VM.

### 2. Monitoring & Events (New for UI)

#### `GET /events` (SSE)
Streams system-wide events.
**Format**: `Event: <type>\nData: <json>`
**Event Types**:
*   `vm_start`: `{ "id": "vm-123", "name": "web" }`
*   `vm_stop`: `{ "id": "vm-123", "code": 0 }`
*   `image_pull_progress`: `{ "image": "ubuntu", "percentage": 50 }`
*   `stats`: `{ "vm_id": "...", "cpu_percent": 10.5, "mem_usage_mb": 128 }` (Throttle to 1s)

#### `GET /vms/:id/console` (WebSocket)
Bi-directional stream for terminal access.
*   **Protocol**: Binary or Text.
*   **Action**: Connects to the VM's PTY (via `ssh` wrapper or serial socket).
*   **UI Component**: xterm.js

#### `GET /metrics`
Prometheus-compatible metrics endpoint (Future proofing).

## 3. Networking & Swarm
*   `GET /networks`, `POST /networks`.
*   `GET /swarm/nodes`.

## 4. System
*   `GET /info`: Daemon version, Host OS, Rootless status.
*   `GET /health`: 200 OK.

## CORS & Security
*   **Cors**: Allow `http://localhost:5173` (Vite Dev Server) and `file://` (Tauri).
*   **Auth**: Currently None (Localhost only). Future: Token header.
