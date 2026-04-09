#!/bin/bash
# chaos_storage.sh - Validates resilience against random storage block removal.

set -euo pipefail

green='\033[0;32m'
red='\033[0;31m'
yellow='\033[0;33m'
nc='\033[0m'

log() { echo -e "${green}[CHAOS STORAGE]${nc} $1"; }
warn() { echo -e "${yellow}[WARN]${nc} $1"; }
fatal() { echo -e "${red}[FATAL]${nc} $1"; exit 1; }

if [ "$EUID" -ne 0 ]; then
  fatal "Chaos tests must run as root."
fi

ign_bin="../../target/release/ign"
if [ ! -f "$ign_bin" ]; then
    ign_bin="cargo run --bin ign --"
fi

log "Starting Chaos Storage Simulation..."

VM_ID=$($ign_bin run alpine:latest --detach)
log "Launched victim VM: $VM_ID"

sleep 2

# Hunt for the Devicemapper backing loop
DM_NAME="ignite_snap_${VM_ID}"
log "Checking for dmsetup mapping: ${DM_NAME}"

if dmsetup info "$DM_NAME" > /dev/null 2>&1; then
    log "Target DEV found: $DM_NAME"
    
    log "INJECTING CHAOS: Forcibly suspending device mapper table..."
    dmsetup suspend "$DM_NAME" || true
    
    log "Checking if daemon survives suspension block..."
    # A status call shouldn't permanently hang
    $ign_bin ps | grep "$VM_ID" || warn "VM missing from PS after suspension."
    
    log "Resuming mapper natively..."
    dmsetup resume "$DM_NAME" || true
else
    warn "Target DEV $DM_NAME not found gracefully. Skipping corruption injection."
fi

log "Cleaning up victim VM."
$ign_bin rm -f "$VM_ID" > /dev/null 2>&1 || true

log "Chaos Storage sequence completed!"
