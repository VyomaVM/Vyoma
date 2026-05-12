# ADR-043: In-VM Agent (vyoma-agent-vm)

## Status
Accepted

## Context
Phase 6.5 of the technical spec calls for an in-VM binary that runs inside each MicroVM to communicate with the host daemon. This provides guest introspection capabilities.

## Decision
Implement `vyoma-agent-vm` - a binary that runs inside each MicroVM:

1. **Communication**: TCP server (vsock for actual VM environments)
2. **Port**: 9999 default
3. **Commands**:
   - `ProcessList` - list all processes in the VM
   - `GetMetrics` - CPU, memory, process count
   - `FileRead` - read files from VM filesystem
   - `ExecCommand` - execute commands in VM

### Architecture
```
┌─────────────┐     vsock/TCP      ┌─────────────┐
│  MicroVM   │ ◄─────────────────► │   vyomad   │
│ (agent-vm) │    port 9999        │  (host)     │
└─────────────┘                     └─────────────┘
```

### Usage
```bash
# Run in TCP mode (development)
vyoma-agent-vm --mode tcp --port 9999

# Run in vsock mode (production VM)
vyoma-agent-vm --mode vsock
```

## Consequences
- Positive: Guest introspection from host
- Positive: Process monitoring inside VM
- Positive: File access for debugging
- Note: vsock requires actual VM environment to test
