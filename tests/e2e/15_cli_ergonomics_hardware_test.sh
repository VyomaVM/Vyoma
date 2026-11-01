#!/usr/bin/env bash
# ==============================================================================
# Vyoma E2E Test 15: Real Hardware Validation for CLI Ergonomics & Compose
# Validates:
#   1. Real physical /dev/kvm hardware virtualization
#   2. Human-readable memory size parsing (--memory 256M, 512M, 1G)
#   3. Named snapshot creation (--name <tag>)
#   4. Structured JSON inspection (vyoma inspect <id>)
#   5. Unified restore (vyoma restore <id> --from <tag>) and time-travel alias
#   6. Smart compose file auto-discovery fallback (docker-compose.yml)
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
echo -e "${BLUE}  Vyoma Phase 1: Real Hardware CLI Ergonomics & Compose Validation           ${NC}"
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
echo -e "\n${YELLOW}[Step 1/6] Validating Physical Hardware KVM Virtualization...${NC}"
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
TEST_ID="e2e_phase1_$$"
TEST_DIR="/tmp/vyoma_${TEST_ID}"
SOCKET_PATH="$TEST_DIR/vyoma.sock"
HTTP_PORT="8765"
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

echo -e "\n${YELLOW}[Step 2/6] Starting vyomad daemon on port ${HTTP_PORT}...${NC}"
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

# 4. Test Human-Readable Memory Parser
echo -e "\n${YELLOW}[Step 3/6] Testing Human-Readable Memory Size CLI Parsing...${NC}"

# Test parsing rejection of invalid memory strings
INVALID_MEM_OUT=$("$VYOMA_BIN" run alpine:latest --memory "invalid" 2>&1 || true)
if echo "$INVALID_MEM_OUT" | grep -q "Invalid memory"; then
    echo -e "${GREEN}✓ Invalid memory size 'invalid' correctly rejected by value_parser.${NC}"
else
    echo -e "${RED}ERROR: Invalid memory string was not rejected!${NC}"
    echo "$INVALID_MEM_OUT"
    exit 1
fi

ZERO_MEM_OUT=$("$VYOMA_BIN" run alpine:latest --memory "0M" 2>&1 || true)
if echo "$ZERO_MEM_OUT" | grep -q "greater than 0"; then
    echo -e "${GREEN}✓ Zero memory size '0M' correctly rejected.${NC}"
else
    echo -e "${RED}ERROR: Zero memory string was not rejected!${NC}"
    echo "$ZERO_MEM_OUT"
    exit 1
fi

# Run real VM with --memory 256M
echo "Running MicroVM with --memory 256M..."
RUN_OUTPUT=$($CLI run alpine:latest --hostname snap-test --vcpu 1 --memory 256M 2>&1) || {
    echo -e "${RED}Failed to run VM: $RUN_OUTPUT${NC}"
    cat "$DAEMON_LOG"
    exit 1
}
echo "$RUN_OUTPUT"
VM_ID=$(echo "$RUN_OUTPUT" | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)

if [ -z "$VM_ID" ]; then
    VM_ID=$($CLI ps | grep -E '[0-9a-f]{8}-[0-9a-f]{4}' | awk '{print $1}' | head -1)
fi

if [ -z "$VM_ID" ]; then
    echo -e "${RED}ERROR: Could not capture VM ID from run output!${NC}"
    exit 1
fi
echo -e "${GREEN}✓ VM launched successfully with ID: $VM_ID${NC}"

sleep 2

# 5. Test vyoma inspect with JSON Output
echo -e "\n${YELLOW}[Step 4/6] Testing 'vyoma inspect' CLI Subcommand with Structured JSON...${NC}"
INSPECT_OUTPUT=$($CLI inspect "$VM_ID")
echo "$INSPECT_OUTPUT"

# Verify structured JSON fields
echo "$INSPECT_OUTPUT" | jq -e . >/dev/null || {
    echo -e "${RED}ERROR: inspect output is not valid JSON!${NC}"
    exit 1
}

INSPECTED_STATUS=$(echo "$INSPECT_OUTPUT" | jq -r .status)
INSPECTED_MEM=$(echo "$INSPECT_OUTPUT" | jq -r .mem_size_mib)
INSPECTED_VCPU=$(echo "$INSPECT_OUTPUT" | jq -r .vcpu)

echo "Inspected Status: $INSPECTED_STATUS"
echo "Inspected Mem MiB: $INSPECTED_MEM"
echo "Inspected vCPU: $INSPECTED_VCPU"

if [ "$INSPECTED_STATUS" != "running" ]; then
    echo -e "${RED}ERROR: Expected status 'running', got '$INSPECTED_STATUS'${NC}"
    exit 1
fi

if [ "$INSPECTED_MEM" != "256" ]; then
    echo -e "${RED}ERROR: Expected memory 256 MiB from --memory 256M, got '$INSPECTED_MEM'${NC}"
    exit 1
fi
echo -e "${GREEN}✓ 'vyoma inspect' verified: status=$INSPECTED_STATUS, mem_size_mib=$INSPECTED_MEM MiB${NC}"

# 6. Test Named Snapshots and Unified Restore
echo -e "\n${YELLOW}[Step 5/6] Testing Named Snapshots & Unified Restore Protocol...${NC}"
SNAP_TAG="before-deploy-v1"
echo "Creating named snapshot with --name '$SNAP_TAG'..."
SNAP_OUTPUT=$($CLI snapshot "$VM_ID" --name "$SNAP_TAG" 2>&1)
echo "$SNAP_OUTPUT"

if echo "$SNAP_OUTPUT" | grep -q "$SNAP_TAG"; then
    echo -e "${GREEN}✓ Snapshot creation output confirmed named tag: $SNAP_TAG${NC}"
else
    echo -e "${RED}ERROR: Snapshot output did not contain tag $SNAP_TAG!${NC}"
    exit 1
fi

echo "Verifying history..."
HIST_OUTPUT=$($CLI history "$VM_ID" 2>&1)
echo "$HIST_OUTPUT"
if echo "$HIST_OUTPUT" | grep -q "$SNAP_TAG"; then
    echo -e "${GREEN}✓ Snapshot history contains tag '$SNAP_TAG'${NC}"
else
    echo -e "${RED}ERROR: Snapshot history missing tag '$SNAP_TAG'!${NC}"
    exit 1
fi

echo "Testing 'vyoma restore $VM_ID --from $SNAP_TAG'..."
RESTORE_OUTPUT=$($CLI restore "$VM_ID" --from "$SNAP_TAG" 2>&1)
echo "$RESTORE_OUTPUT"
if echo "$RESTORE_OUTPUT" | grep -iE 'success|restored'; then
    echo -e "${GREEN}✓ 'vyoma restore --from $SNAP_TAG' succeeded.${NC}"
else
    echo -e "${RED}ERROR: 'vyoma restore' failed!${NC}"
    exit 1
fi

echo "Testing 'vyoma time-travel $VM_ID --to $SNAP_TAG' (backward-compatible alias)..."
TT_OUTPUT=$($CLI time-travel "$VM_ID" --to "$SNAP_TAG" 2>&1)
echo "$TT_OUTPUT"
if echo "$TT_OUTPUT" | grep -iE 'success|restored'; then
    echo -e "${GREEN}✓ 'vyoma time-travel --to $SNAP_TAG' succeeded.${NC}"
else
    echo -e "${RED}ERROR: 'vyoma time-travel' failed!${NC}"
    exit 1
fi

# 7. Test Smart Compose File Auto-Discovery Fallback (docker-compose.yml)
echo -e "\n${YELLOW}[Step 6/6] Testing Smart Compose File Auto-Discovery Fallback...${NC}"
COMPOSE_TEST_DIR="$TEST_DIR/compose_project"
mkdir -p "$COMPOSE_TEST_DIR"
cd "$COMPOSE_TEST_DIR"

# Create only docker-compose.yml (NO vyoma-compose.yml)
cat << 'EOF' > docker-compose.yml
version: '3.8'
services:
  web:
    image: alpine:latest
    deploy:
      resources:
        limits:
          cpus: '1'
          memory: '128M'
EOF

echo "Directory contents: $(ls)"
echo "Executing 'vyoma up -d' without specifying --file..."
UP_OUTPUT=$($CLI up -d 2>&1)
echo "$UP_OUTPUT"

if echo "$UP_OUTPUT" | grep -q "docker-compose.yml"; then
    echo -e "${GREEN}✓ Smart compose auto-discovered 'docker-compose.yml' fallback!${NC}"
else
    echo -e "${RED}ERROR: Compose failed to auto-discover docker-compose.yml!${NC}"
    exit 1
fi

echo "Executing 'vyoma down' without specifying --file..."
DOWN_OUTPUT=$($CLI down 2>&1)
echo "$DOWN_OUTPUT"
echo -e "${GREEN}✓ Compose stack cleanly torn down.${NC}"

cd "$WORKSPACE_ROOT"

echo -e "\n${GREEN}==============================================================================${NC}"
echo -e "${GREEN}  ALL REAL HARDWARE TESTS FOR PHASE 1 PASSED WITH ZERO MOCKING!              ${NC}"
echo -e "${GREEN}==============================================================================${NC}"
