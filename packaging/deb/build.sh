#!/bin/bash
set -e

# Locate cargo (works with rustup and system installs)
if command -v cargo >/dev/null 2>&1; then
    CARGO=$(command -v cargo)
elif [ -f "$HOME/.cargo/bin/cargo" ]; then
    CARGO="$HOME/.cargo/bin/cargo"
else
    echo "Error: cargo not found. Install Rust from https://rustup.rs"
    exit 1
fi

# Setup local environment if user has tools in ~/.local or nvm
[ -d "$HOME/.nvm/versions/node/v24.21.0/bin" ] && export PATH="$HOME/.nvm/versions/node/v24.21.0/bin:$PATH"
[ -d "$HOME/.local/bin" ] && export PATH="$HOME/.local/bin:$PATH"
[ -d "$HOME/.local/lib" ] && export LD_LIBRARY_PATH="$HOME/.local/lib:${LD_LIBRARY_PATH:-}"

VERSION="${VERSION:-$(grep -m1 '^version = ' Cargo.toml 2>/dev/null | cut -d '"' -f 2)}"
VERSION="${VERSION:-3.2.0}"
HOST_ARCH="${TARGET_ARCH:-$(uname -m)}"
case "$HOST_ARCH" in
    x86_64|amd64)
        ARCH="amd64"
        RUST_MUSL_TARGET="x86_64-unknown-linux-musl"
        CH_ASSET="cloud-hypervisor-static"
        CNI_ARCH="amd64"
        VIRTIOFSD_ARCH="x86_64"
        KERNEL_ASSET="bzImage-x86_64"
        CLOUD_HYPERVISOR_CHECKSUM="${VYOMA_CH_SHA256:-4f4f98a3d661a40f3e5272627cc895a1b2e525f2f6bedf5f895f9cd21ba5c65b}"
        CNI_PLUGINS_CHECKSUM="${VYOMA_CNI_SHA256:-77baa2f669980a82255ffa2f2717de823992480271ee778aa51a9c60ae89ff9b}"
        ;;
    aarch64|arm64)
        ARCH="arm64"
        RUST_MUSL_TARGET="aarch64-unknown-linux-musl"
        CH_ASSET="cloud-hypervisor-static-aarch64"
        CNI_ARCH="arm64"
        VIRTIOFSD_ARCH="aarch64"
        KERNEL_ASSET="Image"
        CLOUD_HYPERVISOR_CHECKSUM="${VYOMA_CH_SHA256:-e8a6d9786ea678e739f1a386682846e93742dcb9c7b6e3e0c62e3bda6cff0ef3}"
        CNI_PLUGINS_CHECKSUM="${VYOMA_CNI_SHA256:-}"
        ;;
    *)
        echo "Unsupported target architecture: ${HOST_ARCH}"
        exit 1
        ;;
esac
PKG_NAME="vyoma"
WORK_DIR="target/debian/${PKG_NAME}_${VERSION}_${ARCH}"

# Verify checksum of a downloaded file
# Usage: verify_checksum <file> <expected_sha256>
verify_checksum() {
    local file="$1"
    local expected="$2"

    if [ "$VYOMA_SKIP_CHECKSUM" = "1" ]; then
        echo "Skipping checksum verification (VYOMA_SKIP_CHECKSUM=1)"
        return 0
    fi

    if [ -z "$expected" ]; then
        echo "Notice: no pinned SHA-256 checksum for ${file} on ${ARCH}; skipping verification (set VYOMA_CH_SHA256 / VYOMA_CNI_SHA256 to enforce)"
        return 0
    fi

    if [ ! -f "$file" ]; then
        echo "Error: file $file not found for checksum verification"
        return 1
    fi

    local actual
    actual=$(sha256sum "$file" | awk '{print $1}')

    if [ "$actual" != "$expected" ]; then
        echo "CHECKSUM MISMATCH for $file"
        echo "Expected: $expected"
        echo "Actual:   $actual"
        return 1
    fi

    echo "Checksum OK: $file"
    return 0
}

echo "Building Vyoma v${VERSION} for Debian..."

# 0. Build UI (must be before vyomad compilation for RustEmbed)
echo "Building UI..."
mkdir -p ui/dist
if command -v npm &> /dev/null; then
    (cd ui && npm run build) || echo "Notice: npm run build had warnings, using existing ui/dist"
else
    echo "Skipping UI build (npm not found)"
fi

# 1. Build Binaries
"$CARGO" build --release --bin vyomad --bin vyoma
rustup target add "${RUST_MUSL_TARGET}"
"$CARGO" build --release --target "${RUST_MUSL_TARGET}" --bin vyoma-agent-vm

# 2. Prepare Directory Structure
mkdir -p "${WORK_DIR}/usr/bin"
mkdir -p "${WORK_DIR}/usr/lib/vyoma"
mkdir -p "${WORK_DIR}/etc/systemd/system"
mkdir -p "${WORK_DIR}/DEBIAN"

# 3. Fetch & Copy Dependencies (Cloud Hypervisor, Virtiofsd, CNI plugins)
echo "Fetching dependencies..."

# Fetch Cloud Hypervisor with checksum verification
if [ ! -f "cloud-hypervisor" ]; then
    echo "Downloading cloud-hypervisor v54.0 static binary (${CH_ASSET})..."
    wget -q -O cloud-hypervisor "https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/v54.0/${CH_ASSET}"
    chmod +x cloud-hypervisor
fi

echo "Verifying cloud-hypervisor checksum..."
if ! verify_checksum "cloud-hypervisor" "$CLOUD_HYPERVISOR_CHECKSUM"; then
    echo "ERROR: cloud-hypervisor checksum verification failed!"
    rm -f cloud-hypervisor
    exit 1
fi

# Fetch CNI plugins with checksum verification
CNI_VERSION="v1.5.1"
CNI_DIR="${WORK_DIR}/usr/lib/vyoma/cni"
echo "Fetching CNI plugins..."
mkdir -p "${CNI_DIR}/bin"
if [ ! -s "cni-plugins.tgz" ]; then
    echo "Downloading CNI plugins ${CNI_VERSION} (${CNI_ARCH})..."
    curl -fL -o cni-plugins.tgz "https://github.com/containernetworking/plugins/releases/download/${CNI_VERSION}/cni-plugins-linux-${CNI_ARCH}-${CNI_VERSION}.tgz" || wget -q -O cni-plugins.tgz "https://github.com/containernetworking/plugins/releases/download/${CNI_VERSION}/cni-plugins-linux-${CNI_ARCH}-${CNI_VERSION}.tgz"
fi

echo "Verifying CNI plugins checksum..."
if ! verify_checksum "cni-plugins.tgz" "$CNI_PLUGINS_CHECKSUM"; then
    echo "ERROR: CNI plugins checksum verification failed!"
    rm -f cni-plugins.tgz
    exit 1
fi

tar -xzf cni-plugins.tgz -C "${CNI_DIR}/bin/"

# Fetch virtiofsd with fallback URLs (no checksum - sources unreliable)
VIRTIOFSD_VERSION="1.11.1"
VIRTIOFSD_DIR="${WORK_DIR}/usr/lib/vyoma"

echo "Fetching virtiofsd..."

# Bundle UI to data directory
mkdir -p "${WORK_DIR}/usr/lib/vyoma"
cp -r ui/dist "${WORK_DIR}/usr/lib/vyoma/ui"
echo "UI bundled successfully"

# Check if virtiofsd is already available on system or project bin
if [ -f "bin/virtiofsd" ] && [ ! -L "bin/virtiofsd" ]; then
    cp bin/virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
elif [ -f "/usr/libexec/virtiofsd" ]; then
    cp /usr/libexec/virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
elif [ -f "/usr/lib/qemu/virtiofsd" ]; then
    cp /usr/lib/qemu/virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
elif [ -f "/usr/bin/virtiofsd" ]; then
    cp /usr/bin/virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
fi

if [ ! -f "virtiofsd_bin" ]; then
    # Primary: GitLab releases
    PRIMARY_URL="https://gitlab.com/virtio-fs/virtiofsd/-/releases/${VIRTIOFSD_VERSION}/downloads/virtiofsd-v${VIRTIOFSD_VERSION}-${VIRTIOFSD_ARCH}-musl.zip"
    if curl -fL -m 5 -o virtiofsd.zip "$PRIMARY_URL" 2>/dev/null || wget -q --timeout=5 -O virtiofsd.zip "$PRIMARY_URL" 2>/dev/null; then
        unzip -q -o virtiofsd.zip
        mv virtiofsd virtiofsd_bin 2>/dev/null || true
        chmod +x virtiofsd_bin 2>/dev/null || true
    fi
fi

# Fallback 1: Try QEMU GitHub raw
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL1="https://github.com/qemu/qemu/raw/main/contrib/virtiofsd/virtiofsd-${VIRTIOFSD_ARCH}"
    if curl -fL -m 5 -o virtiofsd "$FALLBACK_URL1" 2>/dev/null || wget -q --timeout=5 -O virtiofsd "$FALLBACK_URL1" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Fallback 2: Try another mirror
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL2="https://raw.githubusercontent.com/qemu/qemu/master/contrib/virtiofsd/virtiofsd-${VIRTIOFSD_ARCH}"
    if curl -fL -m 5 -o virtiofsd "$FALLBACK_URL2" 2>/dev/null || wget -q --timeout=5 -O virtiofsd "$FALLBACK_URL2" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Check if we got virtiofsd
if [ -f "virtiofsd_bin" ]; then
    echo "virtiofsd fetched successfully"
    cp virtiofsd_bin "${VIRTIOFSD_DIR}/virtiofsd"
    chmod 755 "${VIRTIOFSD_DIR}/virtiofsd"
    ln -sf /usr/lib/vyoma/virtiofsd "${WORK_DIR}/usr/bin/virtiofsd"
    echo "virtiofsd linked to /usr/bin"
else
    echo "Warning: virtiofsd not available - volume mounts may not work"
fi

# 4. Copy Assets
cp target/release/vyomad "${WORK_DIR}/usr/bin/"
chmod 755 "${WORK_DIR}/usr/bin/vyomad"
cp target/release/vyoma "${WORK_DIR}/usr/bin/"
chmod 755 "${WORK_DIR}/usr/bin/vyoma"
if [ -f "bin/vyoma-cri" ]; then
    cp bin/vyoma-cri "${WORK_DIR}/usr/bin/vyoma-cri"
    chmod 755 "${WORK_DIR}/usr/bin/vyoma-cri"
fi
cp "target/${RUST_MUSL_TARGET}/release/vyoma-agent-vm" "${WORK_DIR}/usr/bin/vyoma-agent-vm"
chmod 755 "${WORK_DIR}/usr/bin/vyoma-agent-vm"
cp cloud-hypervisor "${WORK_DIR}/usr/bin/cloud-hypervisor"
chmod 755 "${WORK_DIR}/usr/bin/cloud-hypervisor"
cp packaging/systemd/vyomad.service "${WORK_DIR}/etc/systemd/system/"
cp packaging/systemd/vyoma-net.service "${WORK_DIR}/etc/systemd/system/"

# Copy kernel and cloud-hypervisor to data directory
mkdir -p "${WORK_DIR}/var/lib/vyoma/bin"
KERNEL_URL="https://github.com/cloud-hypervisor/linux/releases/download/ch-release-v6.16.9-20260508/${KERNEL_ASSET}"
KERNEL_DEST="${WORK_DIR}/var/lib/vyoma/bin/vmlinux"
if [ ! -f "kernel.bzimage" ]; then
    if [ -f "/tmp/vyoma-bzImage" ]; then
        echo "Using locally compiled custom kernel from /tmp/vyoma-bzImage..."
        cp /tmp/vyoma-bzImage kernel.bzimage
    else
        echo "Downloading custom Vyoma kernel..."
        wget -q -O kernel.bzimage "$KERNEL_URL" || curl -L -o kernel.bzimage "$KERNEL_URL"
    fi
fi
cp kernel.bzimage "$KERNEL_DEST"
chmod 644 "$KERNEL_DEST"
echo "Kernel binary bundled"

# Link Cloud Hypervisor to where the daemon expects it without duplicating binary size
ln -sf /usr/bin/cloud-hypervisor "${WORK_DIR}/var/lib/vyoma/bin/cloud-hypervisor"
echo "Cloud Hypervisor linked to /var/lib/vyoma/bin"

# 5. Create Control File
cat <<EOF > "${WORK_DIR}/DEBIAN/control"
Package: ${PKG_NAME}
Version: ${VERSION}
Section: admin
Priority: optional
Architecture: ${ARCH}
Depends: libc6, openssl, ca-certificates, iptables, kmod
Recommends: qemu-utils
Maintainer: Subeshrock <subesh.rock.3@gmail.com>
Description: Vyoma - MicroVM Ecosystem
 Docker-like experience for Cloud Hypervisor MicroVMs.
 Includes Daemon, CLI, Web UI, and virtiofsd for volume mounts.
EOF

# 6. Create Post-Install Script
# $2 = previous version (empty if fresh install)
cat <<EOF > "${WORK_DIR}/DEBIAN/postinst"
#!/bin/bash
VERSION="${VERSION}"
EOF
cat <<'POSTINST' >> "${WORK_DIR}/DEBIAN/postinst"
set -e

# Create vyoma user (for socket ownership) - idempotent
if ! id vyoma >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin --comment "Vyoma MicroVM Daemon" vyoma 2>/dev/null || true
fi

# Add installing user to vyoma and kvm groups
if [ -n "$SUDO_USER" ]; then
    usermod -aG vyoma "$SUDO_USER" 2>/dev/null || true
    usermod -aG kvm "$SUDO_USER" 2>/dev/null || true
    echo "Added $SUDO_USER to vyoma and kvm groups. Log out and back in to use CLI."
fi

# Add vyoma daemon user to kvm group (for /dev/kvm access)
if getent group kvm > /dev/null 2>&1; then
    usermod -aG kvm vyoma 2>/dev/null || true
fi

# Fix /dev/kvm permissions
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

# Create runtime directory - owned by root:vyoma, group writable (0775)
mkdir -p /run/vyoma
chown root:vyoma /run/vyoma 2>/dev/null || true
chmod 0775 /run/vyoma 2>/dev/null || true

# Setup CNI plugins directory (symlink from system data dir to package location)
mkdir -p /var/lib/vyoma/.vyoma/cni
rm -rf /var/lib/vyoma/.vyoma/cni/bin 2>/dev/null || true
ln -sf /usr/lib/vyoma/cni/bin /var/lib/vyoma/.vyoma/cni/bin

# Ensure /var/lib/vyoma is owned by vyoma:vyoma so daemon can write state and CNI configs
chown -R vyoma:vyoma /var/lib/vyoma 2>/dev/null || true
chmod 0755 /var/lib/vyoma 2>/dev/null || true

# Setup Vyoma network bridge (for VM networking)
if ! ip link show vyoma0 >/dev/null 2>&1; then
    ip link add vyoma0 type bridge
    ip addr add 172.16.0.1/24 dev vyoma0
    ip link set vyoma0 up
fi

# Enable service (idempotent - won't fail if already enabled)
systemctl daemon-reload 2>/dev/null || true
systemctl enable vyomad.service 2>/dev/null || true

# Only start on fresh install ($2 = previous version, empty if first install)
# On upgrade, use try-restart to restart only if already running
if [ -z "$2" ]; then
    # Fresh install - start the service
    systemctl start vyomad.service 2>/dev/null || true
else
    # Upgrade - restart only if already running (won't disrupt if stopped)
    systemctl try-restart vyomad.service 2>/dev/null || true
fi

if systemctl is-active --quiet vyomad.service; then
    echo "vyomad daemon is running."
else
    echo "Notice: vyomad.service is not active yet. Check 'systemctl status vyomad' or verify /dev/kvm permissions."
fi

echo "Vyoma v${VERSION} installed successfully!"
echo "Open http://localhost:8080 for the dashboard"
echo "Run 'vyoma run nginx:latest' to start your first VM"
POSTINST
chmod 755 "${WORK_DIR}/DEBIAN/postinst"

# 7. Create Post-Remove Script
cat <<'POSTRM' > "${WORK_DIR}/DEBIAN/postrm"
#!/bin/bash
set -e
systemctl daemon-reload 2>/dev/null || true
if [ "$1" = "purge" ]; then
    userdel vyoma 2>/dev/null || true
    rm -rf /var/lib/vyoma /run/vyoma 2>/dev/null || true
    ip link del vyoma0 2>/dev/null || true
fi
POSTRM
chmod 755 "${WORK_DIR}/DEBIAN/postrm"

# 8. Build Package
mkdir -p dist
dpkg-deb --build "${WORK_DIR}" "dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"

# 9. Cleanup
rm -f cloud-hypervisor cloud-hypervisor.tgz virtiofsd.zip virtiofsd virtiofsd_bin cni-plugins.tgz
rm -rf "${WORK_DIR}"

echo "Package created at dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"