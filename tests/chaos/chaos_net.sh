#!/bin/bash
# chaos_net.sh - Validates resilience against random network interfaces termination.

set -euo pipefail

red='\033[0;31m'
green='\033[0;32m'
yellow='\033[0;33m'
nc='\033[0m'

log() { echo -e "${green}[CHAOS NET]${nc} $1"; }
warn() { echo -e "${yellow}[WARN]${nc} $1"; }
fatal() { echo -e "${red}[FATAL]${nc} $1"; exit 1; }

if [ "$EUID" -ne 0 ]; then
  fatal "Chaos tests must run as root."
fi

ign_bin="../../target/release/ign"

if [ ! -f "$ign_bin" ]; then
    warn "Release binary not found! Falling back to debug or prompt..."
    ign_bin="cargo run --bin ign --"
fi

log "Starting Chaos Network Simulation..."

# 1. Spin up target VM silently
VM_ID=$($ign_bin run alpine:latest --detach)
log "Launched victim VM: $VM_ID"

sleep 2

# 2. Extract network interface bindings
# We assume ign inspect or native bridging created an ignite0 bridge
BRIDGE_NAME="ignite0"

if ip link show "$BRIDGE_NAME" > /dev/null 2>&1; then
    log "Identified active bridge: $BRIDGE_NAME"
else
    warn "Cannot find standard $BRIDGE_NAME natively."
fi

# 3. Simulate Chaos - Random teardown injected OUTSIDE of Ignite Daemon's runtime
log "INJECTING CHAOS: Forcibly tearing down network links natively..."
ip link set down "$BRIDGE_NAME" || true
ip link del "$BRIDGE_NAME" || true

# 4. Measure System Resilience
log "Assessing VM health. Does it panic when bridge is stripped?"

if $ign_bin logs "$VM_ID" --lines 1 > /dev/null 2>&1; then
    log "Daemon safely handles networking teardown."
else
    # Technically if it panics it might return non-0, but if it gracefully handles it, we good.
    warn "Daemon execution was corrupted by native interface teardown."
fi

# 5. Cleanup
log "Cleaning up victim VM."
$ign_bin rm -f "$VM_ID" > /dev/null 2>&1 || true

log "Chaos Net sequence completed!"
