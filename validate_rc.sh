#!/bin/bash
set -e

# Ignite Release Candidate Validation Script
# Assumes 'ign' and 'ignited' are in target/release/ or available in PATH if configured.
# This script starts the daemon, runs a VM, checks it, and shuts down.

echo ">>> Setting up environment..."
export RUST_LOG=info
DAEMON_BIN="./target/release/ignited"
CLI_BIN="./target/release/ign"
PID_FILE="ignited.pid"

# Cleanup function
cleanup() {
    echo ">>> Cleaning up..."
    if [ -f "$PID_FILE" ]; then
        PID=$(cat "$PID_FILE")
        echo "Killing daemon PID $PID"
        sudo kill $PID || true
        rm "$PID_FILE"
    fi
}
trap cleanup EXIT

# Add local bin to PATH for doctor
export PATH="$PATH:$PWD/bin"

echo ">>> Starting Daemon (sudo required)..."
sudo $DAEMON_BIN > daemon.log 2>&1 &
echo $! > "$PID_FILE"
sleep 2 # Wait for startup

echo ">>> Checking Doctor..."
$CLI_BIN doctor

echo ">>> Pulling Image (alpine:latest)..."
$CLI_BIN pull alpine:latest

echo ">>> Running VM..."
# Capture output to get ID? Or just list
$CLI_BIN run alpine:latest --vcpu 1 --memory 128

echo ">>> Listing VMs..."
$CLI_BIN ps

# We need the ID to stop it. 
# Let's parse it from 'ps' output (assuming 1 running vm)
# Output format: ID            IP        Status
# Skip header
VM_ID=$($CLI_BIN ps | tail -n 1 | awk '{print $1}')

if [ -z "$VM_ID" ] || [ "$VM_ID" == "VM" ]; then
    echo "!!! Failed to get VM ID. PS output:"
    $CLI_BIN ps
    exit 1
fi

echo ">>> Details for VM: $VM_ID"

echo ">>> Stopping VM..."
$CLI_BIN stop $VM_ID

sleep 1

echo ">>> Final PS (Should be empty or stopped)..."
$CLI_BIN ps

echo ">>> Validation Passed!"
