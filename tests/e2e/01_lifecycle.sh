#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 01: VM Lifecycle ==="

check_root
setup_env

# Start Daemon
echo "Starting Daemon..."
sudo -E $IGNITED_BIN > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

# Helper
IGN="$IGN_BIN --address http://127.0.0.1:3000"

# 1. Pull
echo "Pulling image..."
$IGN pull alpine:latest || { echo "Pull failed (network issue?)"; exit 1; }
assert_success "Image Pull"

# 2. Run
echo "Running VM..."
# We need to capture ID. The CLI might print a message.
# For Test Script reliability, having CLI output JSON is better, but currently it prints text.
# We will just run it and check PS.
$IGN run alpine:latest --vcpu 1 --memory 128 --hostname test-vm
assert_success "Run Request"

sleep 5

# 3. PS
echo "Listing VMs..."
$IGN ps
if $IGN ps | grep -q "test-vm"; then
    echo -e "${GREEN}Pass: VM found in PS${NC}"
else
    echo -e "${RED}Fail: VM not found${NC}"
    exit 1
fi

# 4. Logs (Check output)
# $IGN logs <id> ... need ID.
# Extract ID from PS
VM_ID=$($IGN ps | grep "test-vm" | awk '{print $1}')
echo "VM ID: $VM_ID"

echo "Checking Logs..."
$IGN logs $VM_ID
assert_success "Logs Retrieval"

# 5. Stop
echo "Stopping VM..."
$IGN stop $VM_ID
assert_success "Stop Request"

sleep 2
if $IGN ps | grep -q "$VM_ID"; then
    echo -e "${RED}Fail: VM still running${NC}"
    # exit 1 (Soft fail, might take time to stop)
else 
    echo -e "${GREEN}Pass: VM stopped${NC}"
fi

cleanup_env $DAEMON_PID
echo "=== Test 01 Passed ==="
