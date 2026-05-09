#!/bin/bash
set -e

VM_ID="${1:-test-vm}"
TPM_SOCKET="${2:-/var/lib/ignite/vms/$VM_ID/tpm/swtpm.sov}"

echo "=== Vyoma PCR Value Computation ==="
echo "VM: $VM_ID"
echo "TPM Socket: $TPM_SOCKET"

if [ ! -e "$TPM_SOCKET" ]; then
    echo "ERROR: TPM socket not found: $TPM_SOCKET"
    echo "Start VM with vTPM first"
    exit 1
fi

if ! command -v tpm2_pcrread &> /dev/null; then
    echo "Installing tpm2-tools..."
    apt-get update && apt-get install -y tpm2-tools
fi

echo ""
echo "=== Reading PCR values ==="
tpm2_pcrread -T socket --tcti=swtpm:path="$TPM_SOCKET" sha256

echo ""
echo "=== Standard PCR Mapping ==="
echo "PCR 0  - Firmware (OVMF)"
echo "PCR 1  - Firmware Config"
echo "PCR 4  - Boot Manager (shim)"
echo "PCR 5  - Boot Manager Config"
echo "PCR 7  - Secure Boot State"
echo "PCR 9  - Kernel"
echo "PCR 10 - Initrd"
echo "PCR 14 - Root Filesystem (dm-verity)"

echo ""
echo "=== Generating VMIF Expected Values ==="

OUTPUT_FILE="/var/lib/ignite/vms/$VM_ID/pcr_values.json"

tpm2_pcrread -T socket --tcti=swtpm:path="$TPM_SOCKET" sha256 | grep -E "^PCR-[0-9]+:" | \
    sed 's/PCR-\([0-9]*\): /\1: /' | jq -s 'map({key: (.split(": ")[0] | tonumber), value: .split(": ")[1]}) | from_entries' > "$OUTPUT_FILE"

echo "Saved to: $OUTPUT_FILE"
cat "$OUTPUT_FILE"