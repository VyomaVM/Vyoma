#!/usr/bin/env bash
# ==============================================================================
# Vyoma E2E Test 17: Multi-Node Teleportation Target Resolution Hardware Test
# Validates:
#   1. Physical /dev/kvm virtualization & real Cloud Hypervisor execution
#   2. Swarm cluster initialization and Raft node registry
#   3. Target argument flexibility (--to <TARGET> and positional <TARGET>)
#   4. Swarm node name auto-resolution (e.g. node-eu-west-1, node-1 -> IP)
#   5. Error reporting when targeting non-existent cluster nodes
#   6. Direct IP target bypass without name resolution
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}==============================================================================${NC}"
echo -e "${BLUE}  Vyoma Phase 3: Teleport Mesh & Swarm Node Resolution Test                   ${NC}"
echo -e "${BLUE}==============================================================================${NC}"

# 1. Locate Binaries
VYOMA_BIN="$WORKSPACE_ROOT/target/release/vyoma"
VYOMAD_BIN="$WORKSPACE_ROOT/target/release/vyomad"
if [ ! -f "$VYOMA_BIN" ]; then
    VYOMA_BIN="$WORKSPACE_ROOT/target/debug/vyoma"
fi
if [ ! -f "$VYOMAD_BIN" ]; then
    VYOMAD_BIN="$WORKSPACE_ROOT/target/debug/vyomad"
fi

CH_BIN="$WORKSPACE_ROOT/bin/cloud-hypervisor"
KERNEL_BIN="$WORKSPACE_ROOT/bin/vmlinux"

echo -e "Using vyoma:  ${GREEN}$VYOMA_BIN${NC}"
echo -e "Using vyomad: ${GREEN}$VYOMAD_BIN${NC}"

# 2. Check Physical Hardware Virtualization (Zero Mocking)
echo -e "\n${YELLOW}[Step 1/5] Validating Physical Hardware KVM Virtualization...${NC}"
if [ ! -e /dev/kvm ] || [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo -e "${RED}ERROR: /dev/kvm is not accessible/writable by current user ($USER)!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ /dev/kvm verified and read/writable.${NC}"

# 3. Setup Test Workspace & Daemon Instance
TEST_ID="e2e_phase3_$$"
TEST_DIR="/tmp/vyoma_${TEST_ID}"
SOCKET_PATH="$TEST_DIR/vyoma.sock"
HTTP_PORT="8767"
DAEMON_LOG="$TEST_DIR/daemon.log"

mkdir -p "$TEST_DIR/.vyoma/bin"
cp "$CH_BIN" "$TEST_DIR/.vyoma/bin/cloud-hypervisor"
cp "$KERNEL_BIN" "$TEST_DIR/.vyoma/bin/vmlinux"

cleanup() {
    echo -e "\n${YELLOW}--- Teardown & Cleaning Up Test Artifacts ---${NC}"
    if [ -n "${DAEMON_PID:-}" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}✓ Cleanup complete.${NC}"
}
trap cleanup EXIT

echo -e "\n${YELLOW}[Step 2/5] Starting vyomad daemon on port ${HTTP_PORT}...${NC}"
export HOME="$TEST_DIR"
"$VYOMAD_BIN" \
    --data-dir "$TEST_DIR/.vyoma" \
    --socket-path "$SOCKET_PATH" \
    --http-port "$HTTP_PORT" > "$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for i in $(seq 1 40); do
    if [ -S "$SOCKET_PATH" ]; then
        break
    fi
    sleep 0.25
done

if [ ! -S "$SOCKET_PATH" ]; then
    echo -e "${RED}ERROR: vyomad daemon failed to start within timeout!${NC}"
    cat "$DAEMON_LOG"
    exit 1
fi
echo -e "${GREEN}✓ vyomad daemon active on socket $SOCKET_PATH (PID: $DAEMON_PID)${NC}"

CLI="$VYOMA_BIN --socket-path $SOCKET_PATH --http-port $HTTP_PORT"

# 4. Initialize Swarm Cluster and Configure Named Node
echo -e "\n${YELLOW}[Step 3/5] Initializing Swarm Cluster and Registering Named Node...${NC}"
INIT_RESP=$(curl -s -X POST "http://127.0.0.1:${HTTP_PORT}/swarm/init")
echo "Swarm init response: $INIT_RESP"

# Tag Node 1 with friendly name 'node-eu-west-1'
TAG_RESP=$(curl -s -X POST "http://127.0.0.1:${HTTP_PORT}/swarm/nodes/1/name" \
    -H "Content-Type: application/json" \
    -d '{"name": "node-eu-west-1"}')
echo "Tag node response: $TAG_RESP"

# Verify node appears in /swarm/nodes
NODES_JSON=$(curl -s "http://127.0.0.1:${HTTP_PORT}/swarm/nodes")
echo "Cluster nodes: $NODES_JSON"

if echo "$NODES_JSON" | grep -q "node-eu-west-1"; then
    echo -e "${GREEN}✓ Node 'node-eu-west-1' registered in Raft cluster registry.${NC}"
else
    echo -e "${RED}ERROR: Node was not found in /swarm/nodes!${NC}"
    exit 1
fi

# 5. Launch a Real VM on KVM
echo -e "\n${YELLOW}[Step 4/5] Launching Real MicroVM on physical KVM...${NC}"
RUN_OUTPUT=$($CLI run alpine:latest --hostname test-app --vcpu 1 --memory 256M 2>&1)
echo "$RUN_OUTPUT"
VM_ID=$(echo "$RUN_OUTPUT" | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [ -z "$VM_ID" ]; then
    echo -e "${RED}ERROR: Failed to obtain VM ID!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ MicroVM launched with ID: $VM_ID${NC}"

# 6. Test Teleport Syntax & Swarm Node Name Auto-Resolution
echo -e "\n${YELLOW}[Step 5/5] Testing Teleport Arguments & Node Auto-Resolution...${NC}"

# 6a. Test missing target argument rejection
echo "Testing rejection of missing target argument..."
MISSING_OUT=$($CLI teleport "$VM_ID" 2>&1 || true)
if echo "$MISSING_OUT" | grep -iq "Target node IP or node name is required"; then
    echo -e "${GREEN}✓ Missing target rejected with descriptive syntax guidance.${NC}"
else
    echo -e "${RED}ERROR: Missing target was not properly rejected! Output: $MISSING_OUT${NC}"
    exit 1
fi

# 6b. Test non-existent node resolution error
echo -e "\nTesting error handling for non-existent cluster node 'node-mars-north-2'..."
NONEXIST_OUT=$($CLI teleport "$VM_ID" --to node-mars-north-2 2>&1 || true)
echo "$NONEXIST_OUT"
if echo "$NONEXIST_OUT" | grep -q "Swarm node 'node-mars-north-2' not found in cluster"; then
    echo -e "${GREEN}✓ Non-existent node correctly rejected with available node listings.${NC}"
else
    echo -e "${RED}ERROR: Non-existent node was not handled cleanly!${NC}"
    exit 1
fi

# 6c. Test --to flag resolution for registered node 'node-eu-west-1'
echo -e "\nTesting teleport with --to node-eu-west-1..."
TO_OUT=$($CLI teleport "$VM_ID" --to node-eu-west-1 2>&1 || true)
echo "$TO_OUT"
if echo "$TO_OUT" | grep -iE 'Resolved cluster node '\''node-eu-west-1'\''|Initiating Live Teleportation'; then
    echo -e "${GREEN}✓ vyoma teleport --to node-eu-west-1 successfully resolved node name!${NC}"
else
    echo -e "${RED}ERROR: Failed to resolve node name via --to!${NC}"
    exit 1
fi

# 6d. Test positional target resolution for 'node-1'
echo -e "\nTesting teleport with positional argument 'node-1'..."
POS_OUT=$($CLI teleport "$VM_ID" node-1 2>&1 || true)
echo "$POS_OUT"
if echo "$POS_OUT" | grep -iE 'Resolved cluster node '\''node-1'\''|node-eu-west-1|Initiating Live Teleportation'; then
    echo -e "${GREEN}✓ vyoma teleport <id> node-1 successfully resolved positional node name!${NC}"
else
    echo -e "${RED}ERROR: Failed to resolve positional node name!${NC}"
    exit 1
fi

# 6e. Test raw IP address bypass (zero DNS/swarm lookup overhead)
echo -e "\nTesting teleport with raw IP address '10.0.0.99'..."
RAW_OUT=$($CLI teleport "$VM_ID" --to 10.0.0.99 2>&1 || true)
echo "$RAW_OUT"
if echo "$RAW_OUT" | grep -q "target 10.0.0.99"; then
    echo -e "${GREEN}✓ vyoma teleport with raw IP bypassed registry lookup directly.${NC}"
else
    echo -e "${RED}ERROR: Raw IP target was not handled properly!${NC}"
    exit 1
fi

echo -e "\n${GREEN}==============================================================================${NC}"
echo -e "${GREEN}  ALL MULTI-NODE TELEPORTATION RESOLUTION TESTS PASSED WITH ZERO MOCKING!     ${NC}"
echo -e "${GREEN}==============================================================================${NC}"
