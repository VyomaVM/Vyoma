# ADR 021: Bundled virtiofsd Implementation

## Status
Proposed -> Approved

## Context
Ignite relies on `virtiofsd` (specifically the Rust version) to share the host's ext4 or CoW devices with the micro-VM without needing a complex block storage attach. Previously, the user had to install it manually and ensure it was available in `$PATH`. This leads to poor DX and inconsistent versions causing instability.

## Decision
We will bundle a pre-compiled, statically linked version of `virtiofsd` into the Ignite ecosystem.

### Target Path Resolution
The Ignite daemon (`ignited`) will resolve the `virtiofsd` binary in the following priority order:
1. `/opt/ignite/bin/virtiofsd` (ideal for standalone tarball deployments)
2. `/usr/libexec/ignite/virtiofsd` (standard for `.deb`/`.rpm` packaging)
3. `virtiofsd` in `$PATH` (development fallback)

### Packaging Scripts
The `.deb` and `.rpm` packaging scripts will download the appropriate release binary from the `rust-vmm/vhost-device` GitHub repository during the package build process and place it in `/usr/libexec/ignite/virtiofsd`.

### Validation
The `ign doctor` sub-command will explicitly probe these locations and print the resolved `virtiofsd` version to assist with debugging.

## Consequences
- Requires packaging scripts to have internet access to download the binary during build, or requires pre-fetching it into a dist folder.
- Improves "Time to First VM" (TTFVM) significantly since the user does not need to configure Rust toolchains or install dependencies to run Ignite.
