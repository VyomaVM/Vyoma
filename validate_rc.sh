#!/bin/bash
set -e

# Vyoma Release Candidate Validation Script
# Assumes 'vyoma' and 'vyomad' are in target/release/ or available in PATH if configured.
# This script starts the daemon, runs a VM, checks it, and shuts down.

echo ">>> Setting up environment..."
export RUST_LOG=info
DAEMON_BIN="./target/release/vyomad"
CLI_BIN="./target/release/vyoma"
PID_FILE="vyomad.pid"

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

echo ">>> Starting Daemon Check..."
# Check if port 3000 is open
if nc -z localhost 3000; then
    echo ">>> Daemon appears to be running on port 3000. Skipping autostart."
else
    echo ">>> Daemon NOT running. Starting with sudo..."
    sudo $DAEMON_BIN > daemon.log 2>&1 &
    echo $! > "$PID_FILE"
    sleep 2 # Wait for startup
fi

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

echo ">>> Testing Volume Mounts (Phase 9 Verification)..."
mkdir -p test_vol
echo "Hello from Host" > test_vol/hello.txt

# Run VM with volume
echo "Running VM with -v $(pwd)/test_vol:/mnt"
$CLI_BIN run alpine:latest --volumes "$(pwd)/test_vol:/mnt" > run_vol.out
cat run_vol.out
VM_ID_VOL=$(grep "VM ID:" run_vol.out | awk '{print $3}')

if [ -z "$VM_ID_VOL" ]; then
    echo "Volume run failed. Check run_vol.out"
    exit 1
fi

echo "Waiting for VM $VM_ID_VOL to stabilize..."
sleep 3

# Check if virtiofsd is running
if pgrep -f "virtiofsd" > /dev/null; then
    echo "SUCCESS: VirtioFS Daemon is running!"
else
    echo "FAILURE: VirtioFS Daemon NOT found!"
    $CLI_BIN stop $VM_ID_VOL
    exit 1
fi

# Cleanup Volume VM
echo "Stopping Volume VM..."
$CLI_BIN stop $VM_ID_VOL
rm -rf test_vol run_vol.out

echo ">>> All Verification Checks Completed Successfully!"
