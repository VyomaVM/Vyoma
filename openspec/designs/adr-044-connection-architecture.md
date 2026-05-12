# ADR-044: Vyoma Connection Architecture

## Status
Accepted

## Context
Clarify how different clients connect to the vyomad daemon:
- CLI (`vyoma`)
- Browser Dashboard (UI)
- VS Code Extension
- Other tools

## Decision

### Connection Matrix

| Client | Protocol | Endpoint | Security |
|--------|----------|----------|----------|
| CLI (`vyoma`) | Unix Socket | `/var/run/vyoma/vyoma.sock` | File permissions + group |
| Browser Dashboard | HTTP | `http://localhost:3000` | localhost only |
| VS Code Extension | Unix Socket | `/var/run/vyoma/vyoma.sock` | File permissions + group |
| SDK / API Clients | HTTP | `http://localhost:3000` | localhost only |

### Unix Socket Path Resolution
CLI and VS Code Extension should try fallback paths in order:
1. `/var/run/vyoma/vyoma.sock` (default, requires root/vyoma group)
2. `$XDG_RUNTIME_DIR/vyoma.sock` (user-specific)
3. `/tmp/vyoma.sock` (development fallback)

### HTTP Server
The daemon runs an HTTP server on port 3000 for:
- Browser dashboard
- SDK/API access
- Development convenience

## Consequences
- Unix Socket: More secure, per ADR-022 privilege model
- HTTP: Simple for dashboard, localhost only by default
- VS Code Extension: Matches CLI architecture per extension roadmap