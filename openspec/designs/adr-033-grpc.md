# ADR-033: gRPC Interface - ignite-proto Crate

**Status**: Accepted | Phase 3.4 (v1.5)

## Summary
Implement gRPC interface for Ignite to support Kubernetes CRI (Container Runtime Interface) integration in Phase 4.

## Context
As part of Phase 3, we need to add a gRPC interface to Ignite. This enables:
- Kubernetes CRI integration in Phase 4
- Better programmatic access to Ignite API
- Streaming support for exec and logs

## Decision
Implement gRPC service definitions as per the technical spec:

```protobuf
service VmService {
    rpc CreateVm (CreateVmRequest) returns (CreateVmResponse);
    rpc StartVm  (VmIdRequest) returns (VmStatusResponse);
    rpc StopVm   (VmIdRequest) returns (VmStatusResponse);
    rpc DeleteVm (VmIdRequest) returns (google.protobuf.Empty);
    rpc ListVms  (ListVmsRequest) returns (ListVmsResponse);
    rpc GetVm    (VmIdRequest) returns (VmInfo);
    rpc ExecCommand (ExecRequest) returns (stream ExecOutput);
    rpc StreamLogs  (LogRequest) returns (stream LogLine);
    rpc CreateSnapshot (SnapshotRequest) returns (SnapshotInfo);
    rpc RestoreSnapshot (RestoreRequest) returns (VmInfo);
    rpc MigrateVm (MigrateRequest) returns (stream MigrationProgress);
}
```

## Implementation

### Crate Structure
Create `crates/ignite-proto/` with:
- `proto/vm.proto` - Protocol definitions
- `src/lib.rs` - Generated code wrapper
- `src/server.rs` - gRPC server implementation

### Dependencies
```toml
[dependencies]
tonic = "0.11"
prost = "0.12"
```

## Consequences
- Enables Kubernetes integration in Phase 4
- Provides streaming RPC support
- Adds programmatic API for external tools
