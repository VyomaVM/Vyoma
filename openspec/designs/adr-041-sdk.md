# ADR-041: Ignite SDK for Client Applications

## Status
Accepted

## Context
The ignite-agent provides a gRPC interface for remote VM management. We need a client SDK that makes it easy for developers to build applications that interact with MicroVMs using Ignite.

## Decision
We will create an `ignite-sdk` crate that provides:

1. **gRPC Client** - Type-safe wrapper around the gRPC service
2. **VM Operations** - Start, stop, pause, resume, delete VMs
3. **Execution API** - Run commands inside VMs
4. **File Transfer** - Upload/download files to/from VMs
5. **Streaming Logs** - Real-time log streaming from VM console

### Architecture
```
┌─────────────┐     gRPC      ┌─────────────┐
│  SDK Client │ ───────────── │ ignite-agent │
└─────────────┘               └─────────────┘
       │                            │
       ▼                            ▼
┌─────────────┐               ┌─────────────┐
│  User App  │               │  MicroVMs   │
└─────────────┘               └─────────────┘
```

### API Design
```rust
// Connection
let client = IgniteClient::connect("localhost:9000").await?;

// VM lifecycle
let vms = client.list_vms().await?;
let vm = client.get_vm(id).await?;
client.start_vm(id).await?;
client.stop_vm(id).await?;

// Execute commands
let output = client.exec(id, "ls -la").await?;

// Stream logs
let mut stream = client.logs(id).await?;
while let Some(log) = stream.next().await {
    println!("{}", log);
}
```

## Consequences
- Positive: Simple, idiomatic Rust API for VM management
- Positive: Reuses existing gRPC definitions from ignite-proto
- Need: Publish SDK to crates.io for external users
