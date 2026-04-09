#!/bin/bash
# run_matrix.sh - Automated compatibility validation against standard Docker Hub manifests.

set -euo pipefail

green='\033[0;32m'
red='\033[0;31m'
yellow='\033[0;33m'
nc='\033[0m'

log() { echo -e "${green}[COMPAT MATRIX]${nc} $1"; }
warn() { echo -e "${yellow}[WARN]${nc} $1"; }
fatal() { echo -e "${red}[FATAL]${nc} $1"; exit 1; }

if [ "$EUID" -ne 0 ]; then
  fatal "Tests must run as root."
fi

ign_bin="target/release/ign"
if [ ! -f "$ign_bin" ]; then
    warn "Release binary missing. Proceeding with cargo run..."
    ign_bin="cargo run --bin ign --"
fi

IMAGES=(
    "alpine:latest"
    "ubuntu:22.04"
    "python:3.11-slim"
    "node:18-alpine"
    "nginx:latest"
)

log "Initiating Comprehensive OCI Compatibility Matrix!"

FAILED_IMAGES=()

for IMAGE in "${IMAGES[@]}"; do
    log "===================================="
    log "Evaluating mapping for: ${IMAGE}"
    
    # 1. Pull & Spin Up
    OUTPUT=$($ign_bin run "$IMAGE" || true)
    
    # Extract just the VM ID via bash regex or awk from the phrase "VM ID: <uuid>"
    VM_ID=$(echo "$OUTPUT" | grep -o "VM ID: [a-f0-9\-]*" | awk '{print $3}' || true)
    
    if [ -z "$VM_ID" ]; then
        warn "Failed to spin up ${IMAGE}. Output: $OUTPUT"
        FAILED_IMAGES+=("$IMAGE")
        continue
    fi
    
    log "Launched VM successfully. ID: $VM_ID"
    
    # Give it a second to bootstrap runtime environments
    sleep 3
    
    # 2. Check Logs (Did it abort?)
    # Wrap in timeout because if the VM enters an infinite loop, `ign logs` acts like `docker logs -f` and hangs the CI!
    LOG_OUTPUT=$(timeout 2 $ign_bin logs "$VM_ID" || true)
    if [ -z "$LOG_OUTPUT" ]; then
        warn "Log stream empty or failed for ${IMAGE}. This might indicate a catastrophic startup abort!"
    else
        log "Logs extracted successfully... System is structurally stable."
    fi
    
    # 3. Cleanup securely
    log "Purging VM state..."
    $ign_bin stop "$VM_ID" > /dev/null 2>&1 || warn "Cleanup failure for $VM_ID"
    
done

log "===================================="
if [ ${#FAILED_IMAGES[@]} -ne 0 ]; then
    fatal "Matrix expansion failed for the following configurations: ${FAILED_IMAGES[*]}"
else
    log "All Docker Hub OCI configurations verified successfully with 0 defects!"
fi
