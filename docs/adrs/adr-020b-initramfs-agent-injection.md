# ADR-020: Initramfs-based Agent Injection for Cloud Hypervisor

**Date**: 2026-05-10  
**Status**: Accepted  
**Supersedes**: N/A (new approach)

---

## Context

The original VM boot process used mount-based injection to deploy the init script and agent binary into the VM rootfs:
1. Mount the COW device
2. Copy init script and agent binary to `/sbin/`
3. Set permissions
4. Unmount

This approach had several problems:
- **Race conditions**: Concurrent VM boots could cause mount conflicts
- **Security**: Required root/sudo access for mount operations
- **Performance**: Mount/unmount adds ~500ms per VM boot
- **Complexity**: Error handling across multiple failure modes

---

## Decision

Replace mount-based injection with a **gzipped cpio initramfs** approach:

1. **Create initramfs at VM startup**: Generate `vm_dir/initramfs.cpio.gz` containing:
   - `/sbin/vyoma-init`: The init script
   - `/sbin/vyoma-agent-vm`: Agent binary (if available)
   - `/init`: Wrapper that execs `/sbin/vyoma-init`

2. **Pass to Cloud Hypervisor**: Set `PayloadConfig::initramfs` field to the initramfs path

3. **Kernel command line**: Keep `init=/sbin/vyoma-init` - kernel will use the initramfs

---

## Implementation

### Module: `vyoma-core/src/initramfs.rs`

```rust
pub fn create_initramfs(
    init_script: &str,
    agent_path: Option<&Path>,
    output_path: &Path,
) -> Result<PathBuf> {
    // Create gzip-compressed cpio archive
    let file = std::fs::File::create(output_path)?;
    let gz = GzEncoder::new(file, Compression::default());
    
    // Add entries using cpio::newc::Builder
    write_cpio_entry(&mut output, "sbin/vyoma-init", init_script.as_bytes(), 0o755)?;
    
    if let Some(path) = agent_path {
        if path.exists() {
            let agent_bytes = std::fs::read(path)?;
            write_cpio_entry(&mut output, "sbin/vyoma-agent-vm", &agent_bytes, 0o755)?;
        }
    }
    
    let init_wrapper = "#!/bin/sh\nexec /sbin/vyoma-init\n";
    write_cpio_entry(&mut output, "init", init_wrapper.as_bytes(), 0o755)?;
    
    cpio::newc::trailer(&mut output)?;
    gz.finish()?;
    Ok(output_path.to_path_buf())
}
```

### Integration in `vm_service/agent.rs`

```rust
pub async fn prepare_agent(
    _state: &AppState,
    _dm_path: &str,
    vm_dir: &Path,
    _config: &vyoma_core::oci::OciImageConfig,
) -> Result<AgentConfig> {
    let initramfs_path = vm_dir.join("initramfs.cpio.gz");
    let init_script = generate_init_script();

    let agent_binary = PathBuf::from("/usr/bin/vyoma-agent-vm");
    let agent_path = if agent_binary.exists() {
        Some(&agent_binary as &Path)
    } else {
        None
    };

    vyoma_core::initramfs::create_initramfs(&init_script, agent_path, &initramfs_path)
        .context("Failed to create initramfs")?;

    Ok(AgentConfig {
        initramfs_path: Some(initramfs_path),
        cmd: vec!["/sbin/init".to_string()],
        workdir: "/".to_string(),
        envs: vec![],
    })
}
```

### Cloud Hypervisor Configuration

In `vm_service/config.rs`:
```rust
let initramfs_path = agent_config.initramfs_path.as_ref()
    .map(|p| p.to_string_lossy().to_string());

ChConfig {
    // ...
    initramfs_path,
}
```

In `boot.rs`:
```rust
vmm.set_boot_source(&ch_config.kernel_path, &ch_config.boot_args, ch_config.initramfs_path.as_deref()).await?;
```

---

## Consequences

### Positive
- **No mount races**: Each VM gets its own initramfs file
- **No root required for injection**: Initramfs is passed to CH, not written to mounted filesystem
- **Atomic creation**: File is written once, no partial state
- **Faster boot**: Eliminates mount/unmount (~500ms savings)
- **Simpler cleanup**: Initramfs lives in `vm_dir/`, cleaned up with the VM

### Negative
- **Larger memory footprint**: Initramfs decompressed into RAM by kernel
- **Agent binary size**: ~400KB per VM (acceptable for initramfs)
- **Initramfs regeneration**: Each VM boot regenerates (milliseconds, acceptable)

### Risks Mitigated
- **Cloud Hypervisor format**: Tested with `.cpio.gz` extension, CH accepts gzip-compressed cpio
- **Missing agent binary**: Gracefully handled - initramfs created without agent, VM boots

---

## Files Changed

| File | Change |
|------|--------|
| `crates/vyoma-core/Cargo.toml` | Added `cpio` dependency |
| `crates/vyoma-core/src/initramfs.rs` | New module for initramfs creation |
| `crates/vyoma-core/src/lib.rs` | Export initramfs module |
| `crates/vyomad/src/vm_service/agent.rs` | Replace mount logic with initramfs creation |
| `crates/vyomad/src/vm_service/config.rs` | Add initramfs to ChConfig |
| `crates/vyomad/src/vm_service/boot.rs` | Pass initramfs to set_boot_source |
| `crates/vyomad/src/vm_service/types.rs` | Add initramfs_path to AgentConfig/ChConfig |
| `tests/integration/initramfs.rs` | Integration tests for initramfs |

---

## Testing

### Unit Tests
- `test_create_initramfs`: Basic creation without agent
- `test_create_initramfs_with_agent`: With agent binary included
- `test_prepare_agent_without_agent`: Graceful handling when agent missing

### Integration Tests
- `test_initramfs_roundtrip_extract`: Verify cpio format validity
- `test_initramfs_contains_required_files`: Verify expected files present

---

## Future Considerations

1. **Caching initramfs**: If agent binary rarely changes, cache initramfs and regenerate only when agent updates
2. **Compression levels**: Tune gzip compression for size vs. generation time tradeoff
3. **Snapshot restore**: Extend initramfs approach to snapshot restore path (currently uses mount for resolv.conf injection)

---

## Related ADRs

- ADR-002: CLI Wrapper Strategy (mount approach was using CLI wrappers)
- ADR-019: Privileged Service Model (initramfs approach supports rootless better)