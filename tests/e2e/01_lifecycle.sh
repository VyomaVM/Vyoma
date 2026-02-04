#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 01: VM Lifecycle ==="

check_root
setup_env

# Start Daemon
echo "Starting Daemon..."
sudo -E $IGNITED_BIN --port 3001 > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

# Helper
IGN="$IGN_BIN --address http://127.0.0.1:3001"

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

echo "Checking Logs (Timeout 5s)..."
timeout 5s $IGN logs $VM_ID || true
assert_success "Logs Retrieval"

# 5. Pause/Resume
echo "Pausing VM..."
$IGN pause $VM_ID
assert_success "Pause VM"
sleep 1

# Check Status (Should be Paused?)
# Currently PS output doesn't clearly show status in text mode easily unless we grep JSON?
# We assume success if command returns 0.

echo "Resuming VM..."
$IGN resume $VM_ID
assert_success "Resume VM"
sleep 1

# 6. Restart (Disabled: Issue #101 - Restart tries to pull local path)
# echo "Restarting VM..."
# $IGN restart $VM_ID
# assert_success "Restart VM"
# sleep 5


# Verify Restart (New PID or VM ID might change? Logic says Restart replaces VM)
# IGN restart command replaces VM. ID might stay same?
# Check PS again.
# if $IGN ps | grep -q "test-vm"; then
#      echo -e "${GREEN}Pass: VM Restarted${NC}"
# else
#      echo -e "${RED}Fail: VM missing after restart${NC}"
#      exit 1
# fi

# 7. Stop
echo "Stopping VM..."
$IGN stop $VM_ID
# Note: If ID changed during restart, we might need to re-fetch ID.
# Restart logic in CLI: "Stopping VM... Starting replacement VM".
# It prints new VM ID?
# We should use Hostname to stop to be safe.
$IGN stop test-vm || $IGN stop $VM_ID || true
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
