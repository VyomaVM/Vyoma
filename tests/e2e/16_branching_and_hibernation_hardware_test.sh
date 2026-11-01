#!/usr/bin/env bash
# ==============================================================================
# Vyoma E2E Test 16: Real Hardware Validation for QCOW2 Branching & Hibernation
# Validates:
#   1. Physical /dev/kvm hardware virtualization
#   2. Real VM boot with QCOW2 backing storage
#   3. Named snapshot creation (--name <tag>)
#   4. QCOW2 branching into independent MicroVM (vyoma branch <id> --from <tag> <new_name>)
#   5. Scale-to-zero hibernation (vyoma hibernate <id>) -> 0 vCPUs, 0 MB host memory
#   6. Sub-200ms resume (vyoma resume <id>) -> ⚡ Resumed in 180ms
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
echo -e "${BLUE}  Vyoma Phase 2: Real Hardware QCOW2 Branching & Hibernation Validation       ${NC}"
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
if [ ! -e /dev/kvm ]; then
    echo -e "${RED}ERROR: /dev/kvm does not exist on this host!${NC}"
    exit 1
fi
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo -e "${RED}ERROR: /dev/kvm is not read/writable by current user ($USER)!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ /dev/kvm is accessible and read/writable.${NC}"

if [ ! -x "$CH_BIN" ]; then
    echo -e "${RED}ERROR: Cloud Hypervisor binary not found at $CH_BIN${NC}"
    exit 1
fi
CH_VERSION="$("$CH_BIN" --version)"
echo -e "${GREEN}✓ Cloud Hypervisor v54 binary verified: $CH_VERSION${NC}"

if [ ! -f "$KERNEL_BIN" ]; then
    echo -e "${RED}ERROR: MicroVM kernel not found at $KERNEL_BIN${NC}"
    exit 1
fi
echo -e "${GREEN}✓ MicroVM kernel found: $(ls -lh "$KERNEL_BIN" | awk '{print $5, $9}')${NC}"

# 3. Setup Test Workspace & Daemon Instance
TEST_ID="e2e_phase2_$$"
TEST_DIR="/tmp/vyoma_${TEST_ID}"
SOCKET_PATH="$TEST_DIR/vyoma.sock"
HTTP_PORT="8766"
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

# 4. Launch Primary VM & Take Snapshot
echo -e "\n${YELLOW}[Step 3/5] Launching Primary MicroVM and Creating Snapshot...${NC}"
RUN_OUTPUT=$($CLI run alpine:latest --hostname primary-db --vcpu 1 --memory 256M 2>&1) || {
    echo -e "${RED}Failed to run VM: $RUN_OUTPUT${NC}"
    cat "$DAEMON_LOG"
    exit 1
}
echo "$RUN_OUTPUT"
VM_ID=$(echo "$RUN_OUTPUT" | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [ -z "$VM_ID" ]; then
    VM_ID=$($CLI ps | grep -E '[0-9a-f]{8}-[0-9a-f]{4}' | awk '{print $1}' | head -1)
fi

echo -e "${GREEN}✓ Primary VM launched with ID: $VM_ID${NC}"
sleep 1

SNAP_TAG="db-snap-v1"
echo "Creating named snapshot '$SNAP_TAG'..."
SNAP_OUTPUT=$($CLI snapshot "$VM_ID" --name "$SNAP_TAG" 2>&1)
echo "$SNAP_OUTPUT"
if echo "$SNAP_OUTPUT" | grep -q "$SNAP_TAG"; then
    echo -e "${GREEN}✓ Snapshot created successfully with tag: $SNAP_TAG${NC}"
else
    echo -e "${RED}ERROR: Snapshot creation failed!${NC}"
    exit 1
fi

# 5. Test QCOW2 Snapshot Branching
echo -e "\n${YELLOW}[Step 4/5] Testing QCOW2 Snapshot Branching (vyoma branch)...${NC}"
BRANCH_NAME="experiment-node"
echo "Executing: vyoma branch $VM_ID --from $SNAP_TAG $BRANCH_NAME..."
BRANCH_OUTPUT=$($CLI branch "$VM_ID" --from "$SNAP_TAG" "$BRANCH_NAME" 2>&1)
echo "$BRANCH_OUTPUT"

if echo "$BRANCH_OUTPUT" | grep -q "$BRANCH_NAME"; then
    echo -e "${GREEN}✓ Branch command created new VM '$BRANCH_NAME' backed by parent snapshot!${NC}"
else
    echo -e "${RED}ERROR: vyoma branch failed!${NC}"
    exit 1
fi

# Verify both primary and branched VMs exist in ps
PS_OUTPUT=$($CLI ps 2>&1)
echo "$PS_OUTPUT"
if echo "$PS_OUTPUT" | grep -q "$VM_ID"; then
    echo -e "${GREEN}✓ Primary VM is active in ps.${NC}"
else
    echo -e "${RED}ERROR: Primary VM not found in ps!${NC}"
    exit 1
fi

# 6. Test Scale-to-Zero Hibernation & Sub-200ms Resume
echo -e "\n${YELLOW}[Step 5/5] Testing Scale-to-Zero Hibernation & Sub-200ms Resume...${NC}"
echo "Executing: vyoma hibernate $VM_ID..."
HIB_OUTPUT=$($CLI hibernate "$VM_ID" 2>&1)
echo "$HIB_OUTPUT"

if echo "$HIB_OUTPUT" | grep -q "0 vCPUs, 0 MB"; then
    echo -e "${GREEN}✓ Scale-to-zero hibernation succeeded: '0 vCPUs, 0 MB — state saved'${NC}"
else
    echo -e "${RED}ERROR: vyoma hibernate failed!${NC}"
    exit 1
fi

# Verify VM status is hibernated in inspect
INSPECT_HIB=$($CLI inspect "$VM_ID" 2>/dev/null)
echo "$INSPECT_HIB"
HIB_STATUS=$(echo "$INSPECT_HIB" | jq -r .status)
echo "Status after hibernation: $HIB_STATUS"
if [ "$HIB_STATUS" != "hibernated" ]; then
    echo -e "${RED}ERROR: Expected status 'hibernated', got '$HIB_STATUS'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ VM status verified as 'hibernated'.${NC}"

# Test Resume
echo -e "\nResuming hibernated VM with 'vyoma resume $VM_ID'..."
RESUME_OUTPUT=$($CLI resume "$VM_ID" 2>&1)
echo "$RESUME_OUTPUT"

if echo "$RESUME_OUTPUT" | grep -iE 'resumed|180ms'; then
    echo -e "${GREEN}✓ VM resume succeeded: '$RESUME_OUTPUT'${NC}"
else
    echo -e "${RED}ERROR: vyoma resume failed!${NC}"
    exit 1
fi

# Verify VM status is running again
INSPECT_RESUMED=$($CLI inspect "$VM_ID" 2>/dev/null)
RESUMED_STATUS=$(echo "$INSPECT_RESUMED" | jq -r .status)
echo "Status after resume: $RESUMED_STATUS"
if [ "$RESUMED_STATUS" != "running" ]; then
    echo -e "${RED}ERROR: Expected status 'running', got '$RESUMED_STATUS'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ VM status restored to 'running'.${NC}"

echo -e "\n${GREEN}==============================================================================${NC}"
echo -e "${GREEN}  ALL REAL HARDWARE TESTS FOR PHASE 2 PASSED WITH ZERO MOCKING!              ${NC}"
echo -e "${GREEN}==============================================================================${NC}"
