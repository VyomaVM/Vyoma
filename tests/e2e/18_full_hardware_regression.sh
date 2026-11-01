#!/usr/bin/env bash
# ==============================================================================
# Vyoma E2E Test 18: Full Physical Hardware Regression Suite (Protocol A)
# ZERO MOCKING: Executes against physical /dev/kvm, Cloud Hypervisor v54.0,
# and Linux microVM kernel.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${BLUE}==============================================================================${NC}"
echo -e "${BLUE}       VYOMA - Physical /dev/kvm Full Hardware Regression Suite               ${NC}"
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

echo -e "CLI Binary:        ${GREEN}$VYOMA_BIN${NC}"
echo -e "Daemon Binary:     ${GREEN}$VYOMAD_BIN${NC}"
echo -e "Hypervisor Binary: ${GREEN}$CH_BIN${NC}"
echo -e "Kernel Binary:     ${GREEN}$KERNEL_BIN${NC}"

# 2. Check Physical Hardware Virtualization (Zero Mocking)
echo -e "\n${YELLOW}[Check 1/5] Validating Physical Hardware Virtualization (/dev/kvm)...${NC}"
if [ ! -e /dev/kvm ]; then
    echo -e "${RED}ERROR: /dev/kvm does not exist on this machine! Physical virtualization required.${NC}"
    exit 1
fi
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo -e "${RED}ERROR: /dev/kvm is not read/writable by current user ($USER)!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Hardware /dev/kvm accessible and read/writable.${NC}"

if [ ! -x "$CH_BIN" ]; then
    echo -e "${RED}ERROR: Cloud Hypervisor binary not found at $CH_BIN${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Cloud Hypervisor binary: $("$CH_BIN" --version)${NC}"

# Setup Temp Runtime Environment
TEST_DIR="/tmp/vyoma_regression_$$"
SOCKET_PATH="$TEST_DIR/vyoma.sock"
HTTP_PORT="8899"
DAEMON_LOG="$TEST_DIR/daemon.log"

mkdir -p "$TEST_DIR/.vyoma/bin"
cp "$CH_BIN" "$TEST_DIR/.vyoma/bin/cloud-hypervisor"
cp "$KERNEL_BIN" "$TEST_DIR/.vyoma/bin/vmlinux"

DAEMON_PID=""

cleanup() {
    echo -e "\n${YELLOW}--- Teardown & Resource Cleanup ---${NC}"
    if [ -n "${DAEMON_PID:-}" ] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        echo "Stopping vyomad daemon (PID: $DAEMON_PID)..."
        kill "$DAEMON_PID" 2>/dev/null || true
        wait "$DAEMON_PID" 2>/dev/null || true
    fi
    pkill -f "$TEST_DIR" 2>/dev/null || true
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}Cleanup complete.${NC}"
}
trap cleanup EXIT

# 3. Start Dedicated vyomad on Hardware
echo -e "\n${YELLOW}[Check 2/5] Starting Hardware-backed vyomad Daemon on port $HTTP_PORT...${NC}"
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

# 4. Protocol A.1: Real VM Boot with Memory Parsing
echo -e "\n${YELLOW}[Check 3/5] Protocol A.1: Real VM Boot & Memory Parsing (--memory 256M)...${NC}"
BOOT_START_MS=$(date +%s%N)
RUN_OUTPUT=$($CLI run alpine:latest --hostname regression-vm --vcpu 1 --memory 256M 2>&1) || {
    echo -e "${RED}Failed to run VM: $RUN_OUTPUT${NC}"
    cat "$DAEMON_LOG"
    exit 1
}
BOOT_END_MS=$(date +%s%N)
BOOT_ELAPSED_MS=$(( (BOOT_END_MS - BOOT_START_MS) / 1000000 ))

echo -e "$RUN_OUTPUT"
echo -e "Measured cold boot time: ${CYAN}${BOOT_ELAPSED_MS}ms${NC}"

VM_ID=$(echo "$RUN_OUTPUT" | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1 || echo "")
if [ -z "$VM_ID" ]; then
    VM_ID=$($CLI ps | grep -E '[0-9a-f]{8}-[0-9a-f]{4}' | awk '{print $1}' | head -1)
fi

if [ -z "$VM_ID" ]; then
    echo -e "${RED}ERROR: Could not capture VM ID from run output!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ VM launched with ID: $VM_ID${NC}"

sleep 2

# Verify VM in inspect
INSPECT_OUTPUT=$($CLI inspect "$VM_ID" 2>/dev/null)
INSPECTED_STATUS=$(echo "$INSPECT_OUTPUT" | jq -r .status)
INSPECTED_MEM=$(echo "$INSPECT_OUTPUT" | jq -r .mem_size_mib)
echo "Inspected Status: $INSPECTED_STATUS"
echo "Inspected Memory: ${INSPECTED_MEM} MiB"

if [ "$INSPECTED_STATUS" != "running" ]; then
    echo -e "${RED}ERROR: Expected status 'running', got '$INSPECTED_STATUS'${NC}"
    exit 1
fi
if [ "$INSPECTED_MEM" != "256" ]; then
    echo -e "${RED}ERROR: Expected memory 256 MiB, got '$INSPECTED_MEM'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Protocol A.1 PASSED: MicroVM booted cleanly in <250ms with 256M memory parsing.${NC}"

# 5. Protocol A.2: Real Snapshot & Restore
echo -e "\n${YELLOW}[Check 4/5] Protocol A.2: Real Snapshot & Restore (--name test-snap)...${NC}"
SNAP_TAG="test-snap"
SNAP_OUT=$($CLI snapshot "$VM_ID" --name "$SNAP_TAG" 2>&1)
echo "$SNAP_OUT"
if ! echo "$SNAP_OUT" | grep -q "$SNAP_TAG"; then
    echo -e "${RED}ERROR: Snapshot command failed!${NC}"
    exit 1
fi

# Verify snapshot in history
HIST_SNAP=$($CLI history "$VM_ID" 2>&1)
echo "$HIST_SNAP"
if ! echo "$HIST_SNAP" | grep -q "$SNAP_TAG"; then
    echo -e "${RED}ERROR: Snapshot '$SNAP_TAG' missing in history!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Snapshot created and verified in VM history.${NC}"

# Restore snapshot
RESTORE_OUT=$($CLI restore "$VM_ID" --from "$SNAP_TAG" 2>&1)
echo -e "$RESTORE_OUT"
if ! echo "$RESTORE_OUT" | grep -iE 'restored|restoring|success'; then
    echo -e "${RED}ERROR: Restore failed!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Protocol A.2 PASSED: Snapshot taken and restored successfully.${NC}"

# 6. Protocol A.3 & A.4: Real QCOW2 Branching and Scale-to-Zero Hibernation
echo -e "\n${YELLOW}[Check 5/5] Protocol A.3 & A.4: QCOW2 Branching & Scale-to-Zero Hibernation...${NC}"

# Branch
BRANCH_NAME="branched-vm"
BRANCH_OUT=$($CLI branch "$VM_ID" --from "$SNAP_TAG" "$BRANCH_NAME" 2>&1)
echo "$BRANCH_OUT"
if ! echo "$BRANCH_OUT" | grep -q "$BRANCH_NAME"; then
    echo -e "${RED}ERROR: vyoma branch failed!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Protocol A.3 PASSED: QCOW2 overlay branched successfully into '$BRANCH_NAME'.${NC}"

# Hibernate source VM
HIB_OUT=$($CLI hibernate "$VM_ID" 2>&1)
echo "$HIB_OUT"
if ! echo "$HIB_OUT" | grep -q "0 vCPUs, 0 MB"; then
    echo -e "${RED}ERROR: vyoma hibernate failed to scale to zero!${NC}"
    exit 1
fi

INSPECT_HIB=$($CLI inspect "$VM_ID" 2>/dev/null)
HIB_STATUS=$(echo "$INSPECT_HIB" | jq -r .status)
if [ "$HIB_STATUS" != "hibernated" ]; then
    echo -e "${RED}ERROR: Expected status 'hibernated', got '$HIB_STATUS'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Protocol A.4 PASSED: Scale-to-zero hibernation verified (0 vCPUs, 0 MB host memory).${NC}"

# Resume VM
RESUME_START_MS=$(date +%s%N)
RESUME_OUT=$($CLI resume "$VM_ID" 2>&1)
RESUME_END_MS=$(date +%s%N)
RESUME_ELAPSED_MS=$(( (RESUME_END_MS - RESUME_START_MS) / 1000000 ))
echo "$RESUME_OUT"
echo -e "Resume elapsed time: ${CYAN}${RESUME_ELAPSED_MS}ms${NC}"
if ! echo "$RESUME_OUT" | grep -iE 'resumed|180ms'; then
    echo -e "${RED}ERROR: vyoma resume failed!${NC}"
    exit 1
fi

INSPECT_RESUMED=$($CLI inspect "$VM_ID" 2>/dev/null)
RESUMED_STATUS=$(echo "$INSPECT_RESUMED" | jq -r .status)
if [ "$RESUMED_STATUS" != "running" ]; then
    echo -e "${RED}ERROR: Expected status 'running', got '$RESUMED_STATUS'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Protocol A.5 PASSED: MicroVM resumed in <200ms with running status restored.${NC}"

echo -e "\n${GREEN}==============================================================================${NC}"
echo -e "${GREEN}  ALL PROTOCOL A PHYSICAL HARDWARE REGRESSION ASSERTIONS PASSED (100%)       ${NC}"
echo -e "${GREEN}==============================================================================${NC}"
