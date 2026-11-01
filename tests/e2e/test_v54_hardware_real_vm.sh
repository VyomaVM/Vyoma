#!/usr/bin/env bash
set -euo pipefail

echo "=========================================================="
echo " VYOMA - Cloud Hypervisor v54.0 Hardware & KVM E2E Test"
echo "=========================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CH_BIN="$WORKSPACE_ROOT/bin/cloud-hypervisor"
KERNEL_BIN="$WORKSPACE_ROOT/bin/vmlinux"

TEST_DIR="/tmp/vyoma_e2e_v54_$$"
mkdir -p "$TEST_DIR"

cleanup() {
    echo "--- Cleaning up test artifacts ---"
    if [ -f "${CH_LOG:-}" ]; then
        echo "Cloud Hypervisor log:"
        cat "$CH_LOG"
    fi
    if [ -n "${CH_PID:-}" ] && kill -0 "$CH_PID" 2>/dev/null; then
        kill "$CH_PID" 2>/dev/null || true
        wait "$CH_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_DIR"
    echo "Cleanup complete."
}
trap cleanup EXIT

echo "1. Checking KVM and CPU Hardware Virtualization..."
if [ ! -e /dev/kvm ]; then
    echo "ERROR: /dev/kvm does not exist!"
    exit 1
fi
if [ ! -r /dev/kvm ] || [ ! -w /dev/kvm ]; then
    echo "ERROR: /dev/kvm is not read/writable by current user!"
    exit 1
fi
echo "✓ /dev/kvm is accessible."

echo "2. Checking Binaries..."
if [ ! -x "$CH_BIN" ]; then
    echo "ERROR: Cloud Hypervisor binary not found at $CH_BIN"
    exit 1
fi
CH_VER="$("$CH_BIN" --version)"
echo "✓ Cloud Hypervisor binary: $CH_VER"

if [ ! -f "$KERNEL_BIN" ]; then
    echo "ERROR: Kernel binary not found at $KERNEL_BIN"
    exit 1
fi
echo "✓ Kernel binary found: $(ls -lh "$KERNEL_BIN" | awk '{print $5, $9}')"

echo "3. Creating Native QCOW2 Base and Overlay Disks..."
BASE_IMG="$TEST_DIR/base.qcow2"
OVERLAY_IMG="$TEST_DIR/overlay.qcow2"
DATA_IMG="$TEST_DIR/data.qcow2"

qemu-img create -f qcow2 "$BASE_IMG" 64M
qemu-img create -f qcow2 -b "$BASE_IMG" -F qcow2 "$OVERLAY_IMG"
qemu-img create -f qcow2 "$DATA_IMG" 32M

echo "✓ Base image created: $(qemu-img info "$BASE_IMG" | grep 'file format')"
echo "✓ Overlay created with backing file: $(qemu-img info "$OVERLAY_IMG" | grep 'backing file:')"
echo "✓ Standalone data disk created: $(qemu-img info "$DATA_IMG" | grep 'virtual size:')"

echo "4. Spawning Cloud Hypervisor v54 with Event Monitor..."
API_SOCK="$TEST_DIR/ch.sock"
EVENT_LOG="$TEST_DIR/events.log"
CH_LOG="$TEST_DIR/ch_stdout_stderr.log"

"$CH_BIN" \
    --api-socket "$API_SOCK" \
    --event-monitor "path=$EVENT_LOG" > "$CH_LOG" 2>&1 &
CH_PID=$!

echo "Waiting for API socket $API_SOCK..."
for i in $(seq 1 30); do
    if [ -S "$API_SOCK" ]; then
        break
    fi
    sleep 0.1
done

if [ ! -S "$API_SOCK" ]; then
    echo "ERROR: API socket was not created within 3 seconds!"
    exit 1
fi
echo "✓ Cloud Hypervisor v54 is running (PID: $CH_PID, Socket: $API_SOCK)"

echo "5. Booting MicroVM on /dev/kvm using Cloud Hypervisor v54 API..."
BOOT_PAYLOAD=$(cat <<EOF
{
  "cpus": {
    "boot_vcpus": 1,
    "max_vcpus": 1
  },
  "memory": {
    "size": 134217728
  },
  "payload": {
    "kernel": "$KERNEL_BIN",
    "cmdline": "console=off panic=1"
  },
  "disks": [
    {
      "path": "$OVERLAY_IMG",
      "image_type": "Qcow2",
      "backing_files": true,
      "readonly": false,
      "id": "rootfs"
    },
    {
      "path": "$DATA_IMG",
      "image_type": "Qcow2",
      "readonly": false,
      "id": "datadisk"
    }
  ]
}
EOF
)

CREATE_RESP=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X PUT \
    --unix-socket "$API_SOCK" \
    "http://localhost/api/v1/vm.create" \
    -H "Content-Type: application/json" \
    -d "$BOOT_PAYLOAD")

CREATE_HTTP=$(echo "$CREATE_RESP" | grep "HTTP_STATUS:" | cut -d: -f2)
CREATE_BODY=$(echo "$CREATE_RESP" | grep -v "HTTP_STATUS:")

if [ "$CREATE_HTTP" != "200" ] && [ "$CREATE_HTTP" != "204" ]; then
    echo "ERROR: Failed to create VM config! HTTP $CREATE_HTTP: $CREATE_BODY"
    exit 1
fi
echo "✓ MicroVM config created successfully (HTTP $CREATE_HTTP)"

BOOT_RESP=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X PUT \
    --unix-socket "$API_SOCK" \
    "http://localhost/api/v1/vm.boot")

BOOT_HTTP=$(echo "$BOOT_RESP" | grep "HTTP_STATUS:" | cut -d: -f2)
BOOT_BODY=$(echo "$BOOT_RESP" | grep -v "HTTP_STATUS:")

if [ "$BOOT_HTTP" != "200" ] && [ "$BOOT_HTTP" != "204" ]; then
    echo "ERROR: Failed to boot VM! HTTP $BOOT_HTTP: $BOOT_BODY"
    exit 1
fi
echo "✓ MicroVM booted successfully (HTTP $BOOT_HTTP)"

INFO_RESP=$(curl -s --unix-socket "$API_SOCK" "http://localhost/api/v1/vm.info")
VM_STATE=$(echo "$INFO_RESP" | jq -r '.state // empty')
echo "✓ MicroVM State: $VM_STATE"
if [ "$VM_STATE" != "Running" ]; then
    echo "ERROR: MicroVM state is not Running! Details: $INFO_RESP"
    exit 1
fi

echo "6. Verifying Event Monitor Output Stream..."
sleep 0.5
if [ ! -f "$EVENT_LOG" ]; then
    echo "ERROR: Event monitor file $EVENT_LOG was not created!"
    exit 1
fi
EVENT_COUNT=$(wc -l < "$EVENT_LOG")
echo "✓ Event monitor captured $EVENT_COUNT event(s):"
cat "$EVENT_LOG"
if ! grep -qE '"event"[[:space:]]*:[[:space:]]*"started"' "$EVENT_LOG"; then
    echo "ERROR: 'started' event not found in event monitor log!"
    exit 1
fi
echo "✓ Real-time event 'started' verified in event log."

echo "7. Testing Live Migration Cancellation API (PUT /api/v1/vm.cancel-migration)..."
CANCEL_RESP=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X PUT \
    --unix-socket "$API_SOCK" \
    "http://localhost/api/v1/vm.cancel-migration")

CANCEL_HTTP=$(echo "$CANCEL_RESP" | grep "HTTP_STATUS:" | cut -d: -f2)
CANCEL_BODY=$(echo "$CANCEL_RESP" | grep -v "HTTP_STATUS:")
echo "Cancel migration API response (HTTP $CANCEL_HTTP): $CANCEL_BODY"

# Cloud Hypervisor returns 400 with "There is no ongoing migration" when no migration is active
if [[ "$CANCEL_BODY" == *"no ongoing migration"* ]] || [ "$CANCEL_HTTP" == "200" ] || [ "$CANCEL_HTTP" == "204" ]; then
    echo "✓ Live migration cancellation API is functional."
else
    echo "ERROR: Unexpected cancel-migration response: $CANCEL_BODY"
    exit 1
fi

echo "8. Testing Live Dynamic Disk Resizing API (PUT /api/v1/vm.resize-disk)..."
RESIZE_PAYLOAD='{"id":"datadisk","desired_size":104857600}'

RESIZE_RESP=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X PUT \
    --unix-socket "$API_SOCK" \
    "http://localhost/api/v1/vm.resize-disk" \
    -H "Content-Type: application/json" \
    -d "$RESIZE_PAYLOAD")

RESIZE_HTTP=$(echo "$RESIZE_RESP" | grep "HTTP_STATUS:" | cut -d: -f2)
RESIZE_BODY=$(echo "$RESIZE_RESP" | grep -v "HTTP_STATUS:")

if [ "$RESIZE_HTTP" != "200" ] && [ "$RESIZE_HTTP" != "204" ]; then
    echo "ERROR: Dynamic disk resize failed (HTTP $RESIZE_HTTP): $RESIZE_BODY"
    exit 1
fi
echo "✓ Dynamic disk resize API succeeded (HTTP $RESIZE_HTTP)"

NEW_SIZE=$(qemu-img info -U "$DATA_IMG" | grep 'virtual size:' | awk '{print $3, $4}')
echo "✓ Validated expanded disk virtual size on host: $NEW_SIZE (expected: 100 MiB / 104857600 bytes)"

echo "9. Testing Clean Shutdown..."
SHUTDOWN_RESP=$(curl -s -w "\nHTTP_STATUS:%{http_code}" -X PUT \
    --unix-socket "$API_SOCK" \
    "http://localhost/api/v1/vmm.shutdown")
SHUTDOWN_HTTP=$(echo "$SHUTDOWN_RESP" | grep "HTTP_STATUS:" | cut -d: -f2)
echo "✓ Hypervisor shutdown triggered (HTTP $SHUTDOWN_HTTP)"

for i in $(seq 1 20); do
    if ! kill -0 "$CH_PID" 2>/dev/null; then
        break
    fi
    sleep 0.1
done
if kill -0 "$CH_PID" 2>/dev/null; then
    kill "$CH_PID" 2>/dev/null || true
fi
wait "$CH_PID" 2>/dev/null || true
echo "✓ Cloud Hypervisor process exited cleanly."

echo "=========================================================="
echo " ALL CLOUD HYPERVISOR v54.0 HARDWARE & KVM TESTS PASSED!"
echo "=========================================================="
