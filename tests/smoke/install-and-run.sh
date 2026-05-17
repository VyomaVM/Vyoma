#!/bin/bash
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

LOG_FILE="/tmp/vyoma-smoke-test-$(date +%s).log"
exec > >(tee -a "$LOG_FILE") 2>&1

echo "=========================================="
echo "Vyoma Smoke Test - Fresh Install"
echo "=========================================="

cleanup_on_exit() {
    echo -e "${YELLOW}Cleaning up...${NC}"

    if [ -n "$VM_ID" ]; then
        sudo vyoma stop "$VM_ID" 2>/dev/null || true
    fi

    sudo pkill vyomad 2>/dev/null || true

    sleep 2

    echo "Checking for leftover resources..."

    TAP_DEVICES=$(ip link 2>/dev/null | grep -c "tap" || true)
    if [ "$TAP_DEVICES" -gt 0 ]; then
        echo -e "${RED}Warning: Found $TAP_DEVICES leftover TAP devices${NC}"
        ip link show | grep tap || true
    fi

    DM_DEVICES=$(sudo dmsetup ls 2>/dev/null | wc -l || true)
    if [ "$DM_DEVICES" -gt 2 ]; then
        echo -e "${RED}Warning: Found leftover DM devices${NC}"
        sudo dmsetup ls 2>/dev/null || true
    fi

    LOOP_DEVICES=$(losetup -a 2>/dev/null | wc -l || true)
    if [ "$LOOP_DEVICES" -gt 0 ]; then
        echo -e "${RED}Warning: Found leftover loop devices${NC}"
        losetup -a 2>/dev/null || true
    fi
}

trap cleanup_on_exit EXIT

if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root${NC}"
    exit 1
fi

echo -e "${YELLOW}Step 1: Building package...${NC}"
cd /home/subesh/Startup_Projects/Vyoma/vyoma

if [ ! -f "packaging/deb/build.sh" ]; then
    echo -e "${RED}Error: Build script not found${NC}"
    exit 1
fi

chmod +x packaging/deb/build.sh
./packaging/deb/build.sh

if [ ! -d "dist" ]; then
    echo -e "${RED}Error: dist directory not created${NC}"
    exit 1
fi

DEB_FILE=$(ls dist/vyoma_*.deb 2>/dev/null | head -1)
if [ -z "$DEB_FILE" ]; then
    echo -e "${RED}Error: No .deb file found in dist/${NC}"
    exit 1
fi

echo -e "${GREEN}Built: $DEB_FILE${NC}"

echo -e "${YELLOW}Step 2: Installing package...${NC}"
dpkg -i "$DEB_FILE" || {
    echo -e "${YELLOW}Fixing dependencies...${NC}"
    apt-get -f install -y
    dpkg -i "$DEB_FILE"
}

if ! command -v vyoma &> /dev/null; then
    echo -e "${RED}Error: vyoma command not found after install${NC}"
    exit 1
fi

if ! command -v vyomad &> /dev/null; then
    echo -e "${RED}Error: vyomad command not found after install${NC}"
    exit 1
fi

echo -e "${GREEN}Package installed successfully${NC}"

echo -e "${YELLOW}Step 3: Running vyoma doctor...${NC}"
DOCTOR_OUTPUT=$(vyoma doctor 2>&1)
echo "$DOCTOR_OUTPUT"

if echo "$DOCTOR_OUTPUT" | grep -qi "failed\|error\|missing\|not found"; then
    echo -e "${RED}Error: Doctor check failed${NC}"
    exit 1
fi

if echo "$DOCTOR_OUTPUT" | grep -q "All checks passed\|Ready to use"; then
    echo -e "${GREEN}Doctor check passed${NC}"
else
    DOCTOR_FAILED=0
    if echo "$DOCTOR_OUTPUT" | grep -q "KVM"; then
        echo -e "${RED}Error: KVM not available${NC}"
        DOCTOR_FAILED=1
    fi
    if echo "$DOCTOR_OUTPUT" | grep -q "CNI"; then
        echo -e "${RED}Error: CNI plugins missing${NC}"
        DOCTOR_FAILED=1
    fi
    if [ "$DOCTOR_FAILED" -eq 1 ]; then
        exit 1
    fi
fi

echo -e "${YELLOW}Step 4: Starting daemon...${NC}"
sudo -b vyomad --http-port 8080 > /tmp/vyomad.log 2>&1
sleep 3

if ! pgrep -x vyomad > /dev/null; then
    echo -e "${RED}Error: Daemon failed to start${NC}"
    cat /tmp/vyomad.log
    exit 1
fi

echo -e "${GREEN}Daemon started${NC}"

echo -e "${YELLOW}Step 5: Running alpine:latest VM...${NC}"
VM_OUTPUT=$(vyoma run alpine:latest --vcpu 1 --memory 128 2>&1)
echo "$VM_OUTPUT"

VM_ID=$(echo "$VM_OUTPUT" | grep -oP 'VM ID: \K[a-f0-9]+' | head -1)
if [ -z "$VM_ID" ]; then
    VM_ID=$(vyoma ps 2>/dev/null | grep "alpine" | awk '{print $1}' | head -1)
fi

if [ -z "$VM_ID" ]; then
    echo -e "${RED}Error: Could not get VM ID${NC}"
    exit 1
fi

echo "VM ID: $VM_ID"

echo -e "${YELLOW}Step 6: Verifying VM is running...${NC}"
sleep 3

VM_STATE=$(vyoma ps 2>/dev/null | grep "$VM_ID" | awk '{print $NF}' | tr -d '[]')
echo "VM State: $VM_STATE"

if [ "$VM_STATE" != "Running" ]; then
    echo -e "${RED}Error: VM not in Running state${NC}"
    vyoma logs "$VM_ID" 2>/dev/null || true
    exit 1
fi

echo -e "${GREEN}VM is running${NC}"

echo -e "${YELLOW}Step 7: Stopping VM...${NC}"
vyoma stop "$VM_ID"
sleep 2

if vyoma ps 2>/dev/null | grep -q "$VM_ID"; then
    echo -e "${RED}Error: VM still in ps after stop${NC}"
    exit 1
fi

echo -e "${GREEN}VM stopped successfully${NC}"

echo -e "${YELLOW}Step 8: Checking for leftover resources...${NC}"

TAP_COUNT=$(ip link 2>/dev/null | grep -c "tap" || echo "0")
if [ "$TAP_COUNT" -gt 0 ]; then
    echo -e "${RED}Fail: Found $TAP_COUNT TAP devices${NC}"
    ip link show | grep tap
    exit 1
fi
echo -e "${GREEN}No leftover TAP devices${NC}"

DM_COUNT=$(sudo dmsetup ls 2>/dev/null | wc -l)
if [ "$DM_COUNT" -gt 2 ]; then
    echo -e "${RED}Fail: Found DM devices${NC}"
    sudo dmsetup ls
    exit 1
fi
echo -e "${GREEN}No leftover DM devices${NC}"

LOOP_COUNT=$(losetup -a 2>/dev/null | wc -l)
if [ "$LOOP_COUNT" -gt 0 ]; then
    echo -e "${RED}Fail: Found loop devices${NC}"
    losetup -a
    exit 1
fi
echo -e "${GREEN}No leftover loop devices${NC}"

echo -e "${YELLOW}Step 9: Uninstalling package...${NC}"
dpkg -r vyoma || dpkg -r vyomad || true

if command -v vyoma &> /dev/null; then
    echo -e "${RED}Warning: vyoma still in path after uninstall${NC}"
fi

echo -e "${YELLOW}Step 10: Running complete cleanup script to reset system...${NC}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -f "$PROJECT_ROOT/scripts/cleanup-all.sh" ]; then
    echo "Calling cleanup-all.sh..."
    "$PROJECT_ROOT/scripts/cleanup-all.sh"
    echo -e "${GREEN}Cleanup script completed${NC}"
else
    echo -e "${YELLOW}Warning: cleanup-all.sh not found, skipping${NC}"
fi

echo -e "${GREEN}Smoke test passed!${NC}"
echo "=========================================="
echo "Log file: $LOG_FILE"
echo "=========================================="

exit 0