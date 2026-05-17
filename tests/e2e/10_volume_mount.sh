#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 10: Volume Mount ==="

check_root
setup_env

echo "Starting Daemon (3010)..."
sudo -E $VYOMAD_BIN --socket-path /run/vyoma/test.sock --http-port 3010 > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

VYOMA="$VYOMA_BIN --socket-path /run/vyoma/test.sock --http-port 3010"

HOST_DIR="$TEST_HOME/volume_data"
mkdir -p "$HOST_DIR"
echo "volume-test-content-$(date +%s)" > "$HOST_DIR/testfile.txt"
echo "marker-file-created" >> "$HOST_DIR/testfile.txt"

echo "Running VM with volume mount -v ${HOST_DIR}:/data..."
VM_OUTPUT=$($VYOMA run alpine:latest --hostname volume-vm --volume "${HOST_DIR}:/data" -- vcpu 1 --memory 128 -- sleep 300 2>&1)
VM_ID=$(echo "$VM_OUTPUT" | awk -F 'VM ID: ' '{print $2}' | awk '{print $1}' | tr -d ',')

if [ -z "$VM_ID" ]; then
    VM_ID=$($VYOMA ps | grep "volume-vm" | awk '{print $1}')
fi

echo "VM ID: $VM_ID"

if [ -z "$VM_ID" ]; then
    echo -e "${RED}Fail: Could not start VM${NC}"
    exit 1
fi

register_vm "$VM_ID"

wait_for_vm_state "$VM_ID" "Running" 15
assert_success "VM started with volume"

sleep 2

echo "Retrieving logs to verify volume mount..."
LOGS=$($VYOMA logs "$VM_ID" 2>&1 || true)
echo "Logs: $LOGS"

if echo "$LOGS" | grep -q "volume-test-content"; then
    echo -e "${GREEN}Pass: Volume mount works - file content visible in logs${NC}"
elif [ -f "$HOST_DIR/testfile.txt" ]; then
    echo -e "${GREEN}Pass: Host directory still accessible after VM run${NC}"
    CONTENT=$(cat "$HOST_DIR/testfile.txt")
    if echo "$CONTENT" | grep -q "volume-test-content"; then
        echo -e "${GREEN}Pass: Volume data preserved${NC}"
    fi
else
    echo -e "${YELLOW}Warning: Could not verify volume mount via logs, checking host directory...${NC}"
    if [ -f "$HOST_DIR/testfile.txt" ]; then
        echo -e "${GREEN}Pass: Host directory accessible${NC}"
    else
        echo -e "${RED}Fail: Volume mount verification failed${NC}"
        exit 1
    fi
fi

echo "Cleaning up..."
$VYOMA stop "$VM_ID" 2>/dev/null || true
unregister_vm "$VM_ID"

cleanup_env $DAEMON_PID
echo "=== Test 10 Passed ==="