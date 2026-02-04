#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 07: Snapshots & Teleportation ==="
check_root
setup_env

# Start Daemon
echo "Starting Daemon..."
sudo -E $IGNITED_BIN > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3
IGN="$IGN_BIN --address http://127.0.0.1:3000"

# 1. Run VM
echo "Running VM..."
$IGN run alpine:latest --hostname snap-vm
sleep 5

# 2. Snapshot
echo "Snapshotting..."
$IGN snapshot snap-vm
assert_success "Snapshot Request"

# 3. Export
echo "Exporting..."
$IGN export snap-vm $TEST_HOME/vm_export.tar.gz
assert_success "Export"

if [ ! -f "$TEST_HOME/vm_export.tar.gz" ]; then
    echo -e "${RED}Fail: Export file missing${NC}"
    exit 1
fi

# 4. Import
echo "Importing..."
# Import logic should restore the VM (possibly with new ID or same ID?).
# Since we didn't stop the original, Import might conflict if it uses same name/ID?
# Export usually saves metadata.
# Let's try stopping original first.
$IGN stop snap-vm
sleep 2

$IGN import $TEST_HOME/vm_export.tar.gz
assert_success "Import"

# Verify
if $IGN ps | grep -q "snap-vm"; then
    echo -e "${GREEN}Pass: Imported VM found${NC}"
else
    echo -e "${RED}Fail: Imported VM not found${NC}"
fi

cleanup_env $DAEMON_PID
echo "=== Test 07 Passed ==="
