# ADR-044: Ignite Connection Architecture

## Status
Accepted

## Context
Clarify how different clients connect to the ignited daemon:
- CLI (`ign`)
- Browser Dashboard (UI)
- VS Code Extension
- Other tools

## Decision

### Connection Matrix

| Client | Protocol | Endpoint | Security |
|--------|----------|----------|----------|
| CLI (`ign`) | Unix Socket | `/var/run/ignite/ignite.sock` | File permissions + group |
| Browser Dashboard | HTTP | `http://localhost:3000` | localhost only |
| VS Code Extension | Unix Socket | `/var/run/ignite/ignite.sock` | File permissions + group |
| SDK / API Clients | HTTP | `http://localhost:3000` | localhost only |

### Unix Socket Path Resolution
CLI and VS Code Extension should try fallback paths in order:
1. `/var/run/ignite/ignite.sock` (default, requires root/ignite group)
2. `$XDG_RUNTIME_DIR/ignite.sock` (user-specific)
3. `/tmp/ignite.sock` (development fallback)

### HTTP Server
The daemon runs an HTTP server on port 3000 for:
- Browser dashboard
- SDK/API access
- Development convenience

## Consequences
- Unix Socket: More secure, per ADR-022 privilege model
- HTTP: Simple for dashboard, localhost only by default
- VS Code Extension: Matches CLI architecture per extension roadmap