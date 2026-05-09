#!/bin/bash
set -e

OVMF_DIR="${OVMF_DIR:-/var/lib/vyoma/firmware}"
FIRMWARE_VERSION="${FIRMWARE_VERSION:-1.0}"

mkdir -p "$OVMF_DIR"

echo "=== Vyoma OVMF Firmware Setup ==="
echo "Installing to: $OVMF_DIR"

SYSTEM_OVMF=""
for path in /usr/share/ovmf/x64/OVMF_CODE.fd \
            /usr/share/edk2/ovmf/OVMF_CODE.fd \
            /usr/share/qemu/ovmf-x86_64-code.bin; do
    if [ -f "$path" ]; then
        SYSTEM_OVMF="$path"
        break
    fi
done

if [ -z "$SYSTEM_OVMF" ]; then
    echo "ERROR: OVMF not found. Install edk2-ovmf or qemu-ovmf-x86 package"
    exit 1
fi

echo "Found system OVMF: $SYSTEM_OVMF"

cp "$SYSTEM_OVMF" "$OVMF_DIR/OVMF_CODE.fd"
echo "Copied OVMF_CODE.fd"

if [ -f /usr/share/ovmf/x64/OVMF_VARS.fd ]; then
    cp /usr/share/ovmf/x64/OVMF_VARS.fd "$OVMF_DIR/ovmf_vars.fd"
    echo "Copied OVMF_VARS.fd template"
elif [ -f /usr/share/edk2/ovmf/OVMF_VARS.fd ]; then
    cp /usr/share/edk2/ovmf/OVMF_VARS.fd "$OVMF_DIR/ovmf_vars.fd"
    echo "Copied OVMF_VARS.fd template"
fi

if command -v sbctl &> /dev/null; then
    echo "=== Setting up Secure Boot keys with sbctl ==="
    mkdir -p "$OVMF_DIR/keys"
    cd "$OVMF_DIR/keys"

    if [ ! -f PK/PK.key ]; then
        echo "Generating Secure Boot keys..."
        sbctl create-keys
        sbctl enroll-keys --microsoft
    fi

    echo "Secure Boot keys ready in $OVMF_DIR/keys"
else
    echo "WARNING: sbctl not found. Secure Boot key enrollment not available."
    echo "Install sbctl: apt install sbctl"
fi

cat > "$OVMF_DIR/config.json" << EOF
{
    "version": "$FIRMWARE_VERSION",
    "ovmf_code": "$OVMF_DIR/OVMF_CODE.fd",
    "ovmf_vars_template": "$OVMF_DIR/ovmf_vars.fd",
    "secure_boot_enabled": true,
    "pcr_policy": {
        "0": "firmware",
        "1": "firmware_config", 
        "4": "boot_manager",
        "5": "boot_manager_config",
        "7": "secure_boot_state",
        "9": "kernel",
        "10": "initrd",
        "14": "rootfs"
    }
}
EOF

echo "Created firmware config: $OVMF_DIR/config.json"

echo ""
echo "=== OVMF Setup Complete ==="
ls -la "$OVMF_DIR"
echo ""
echo "To enable Secure Boot in VM, use:"
echo "  vmm.set_firmware('$OVMF_DIR/OVMF_CODE.fd', true, '$OVMF_DIR/ovmf_vars.fd')"