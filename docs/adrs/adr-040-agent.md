# ADR-040: vyoma-agent - In-VM Binary

**Status**: Accepted | Phase 4.5 (v1.9)

## Summary
Implement vyoma-agent - an in-VM binary that runs inside each MicroVM to communicate with the host daemon via vsock.

## Context
As part of Phase 4, we need an in-VM agent to enable host-VM communication. This allows:
- Process listing and management
- Metrics collection
- File operations
- Command execution

## Decision
Implement vyoma-agent as per the technical spec:

### Architecture
- Static musl binary (~400KB) injected into each VMIF image
- Runs as PID 2 (init process)
- Communicates via vsock with host

### Communication
- vsock listener on port 9999
- JSON-based protocol
- Handles: ProcessList, ExecCommand, GetMetrics, FileRead

## Implementation

### Location
- `crates/vyoma-agent/` - Rust binary

### Key Components
```rust
enum AgentRequest {
    ProcessList,
    ExecCommand { cmd, env, workdir },
    GetMetrics,
    FileRead { path },
}

enum AgentResponse {
    ProcessList(Vec<ProcessInfo>),
    ExecStarted(exec_id),
    Metrics(VmMetrics),
    FileContent(Vec<u8>),
}
```

## Consequences
- Host-VM communication channel
- In-VM observability
- Process management
- File system access from host
