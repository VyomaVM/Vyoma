#!/bin/bash
set -e

VERSION="2.1.1"
ARCH="amd64"
PKG_NAME="ignite"
WORK_DIR="target/debian/${PKG_NAME}_${VERSION}_${ARCH}"

echo "Building Ignite v${VERSION} for Debian..."

# 1. Build Binaries
cargo build --release --bin ignited --bin ign

# 2. Prepare Directory Structure
mkdir -p "${WORK_DIR}/usr/bin"
mkdir -p "${WORK_DIR}/usr/lib/ignite"
mkdir -p "${WORK_DIR}/etc/systemd/system"
mkdir -p "${WORK_DIR}/DEBIAN"

# 3. Fetch & Copy Dependencies (Firecracker, Virtiofsd)
echo "Fetching dependencies..."
if [ ! -f "firecracker" ]; then
    wget -q -O firecracker.tgz https://github.com/firecracker-microvm/firecracker/releases/download/v1.7.0/firecracker-v1.7.0-x86_64.tgz
    tar -xzf firecracker.tgz
    mv release-v1.7.0-x86_64/firecracker-v1.7.0-x86_64 firecracker
    chmod +x firecracker
fi

# Fetch virtiofsd with fallback URLs
VIRTIOFSD_VERSION="1.11.1"
VIRTIOFSD_DIR="${WORK_DIR}/usr/lib/ignite"

echo "Fetching virtiofsd..."
if [ ! -f "virtiofsd" ]; then
    # Primary: GitLab releases
    PRIMARY_URL="https://gitlab.com/virtio-fs/virtiofsd/-/releases/${VIRTIOFSD_VERSION}/downloads/virtiofsd-v${VIRTIOFSD_VERSION}-x86_64-musl.zip"
    if wget -q -O virtiofsd.zip "$PRIMARY_URL" 2>/dev/null; then
        unzip -q -o virtiofsd.zip
        mv virtiofsd virtiofsd_bin 2>/dev/null || true
        chmod +x virtiofsd_bin 2>/dev/null || true
    fi
fi

# Fallback 1: Try QEMU GitHub raw
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL1="https://github.com/qemu/qemu/raw/main/contrib/virtiofsd/virtiofsd-x86_64"
    if wget -q -O virtiofsd "$FALLBACK_URL1" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Fallback 2: Try another mirror
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL2="https://raw.githubusercontent.com/qemu/qemu/master/contrib/virtiofsd/virtiofsd-x86_64"
    if wget -q -O virtiofsd "$FALLBACK_URL2" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Check if we got virtiofsd
if [ -f "virtiofsd_bin" ]; then
    echo "virtiofsd fetched successfully"
    cp virtiofsd_bin "${VIRTIOFSD_DIR}/virtiofsd"
    chmod +x "${VIRTIOFSD_DIR}/virtiofsd"
else
    echo "Warning: virtiofsd not available - volume mounts may not work"
fi

# 4. Copy Assets
cp target/release/ignited "${WORK_DIR}/usr/bin/"
cp target/release/ign "${WORK_DIR}/usr/bin/"
cp firecracker "${WORK_DIR}/usr/bin/firecracker"
cp packaging/systemd/ignited.service "${WORK_DIR}/etc/systemd/system/"

# 5. Create Control File
cat <<EOF > "${WORK_DIR}/DEBIAN/control"
Package: ${PKG_NAME}
Version: ${VERSION}
Section: admin
Priority: optional
Architecture: ${ARCH}
Depends: libc6, openssl, ca-certificates
Maintainer: Subeshrock <subesh.rock.3@gmail.com>
Description: Ignite - MicroVM Ecosystem
 Docker-like experience for Firecracker MicroVMs.
 Includes Daemon, CLI, Web UI, and virtiofsd for volume mounts.
EOF

# 6. Create Post-Install Script
cat <<'POSTINST' > "${WORK_DIR}/DEBIAN/postinst"
#!/bin/bash
set -e

# Create ignite user if not exists
if ! id ignite >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin --comment "Ignite MicroVM Daemon" ignite 2>/dev/null || true
fi

# Add ignite daemon user to kvm group (for /dev/kvm access)
if getent group kvm > /dev/null 2>&1; then
    usermod -aG kvm ignite 2>/dev/null || true
fi

# Fix /dev/kvm permissions
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

# Create runtime directory
mkdir -p /run/ignite
chown root:ignite /run/ignite 2>/dev/null || true
chmod 0755 /run/ignite 2>/dev/null || true

# Set socket group ownership (will be created by daemon)
chown root:ignite /run/ignite/ignite.sock 2>/dev/null || true
chmod 0660 /run/ignite/ignite.sock 2>/dev/null || true

# Ensure sudoers bypass for ignited commands as ignite user
if [ ! -f /etc/sudoers.d/ignite ]; then
    cat <<'SUDOERS' > /etc/sudoers.d/ignite
ignite ALL=(ALL) NOPASSWD: /usr/bin/mount, /usr/bin/umount, /usr/bin/ip, /usr/sbin/losetup, /usr/sbin/dmsetup, /usr/bin/debugfs
SUDOERS
    chmod 0440 /etc/sudoers.d/ignite
fi

# Enable and start service
systemctl daemon-reload 2>/dev/null || true
systemctl enable ignited.service 2>/dev/null || true
systemctl start ignited.service 2>/dev/null || true

echo "Ignite v2.1.1 installed successfully!"
echo "Open http://localhost:3000 for the dashboard"
echo "Run 'ign run nginx:latest' to start your first VM"
POSTINST
chmod 755 "${WORK_DIR}/DEBIAN/postinst"

# 7. Create Post-Remove Script
cat <<'POSTRM' > "${WORK_DIR}/DEBIAN/postrm"
#!/bin/bash
set -e
systemctl daemon-reload 2>/dev/null || true
if [ "$1" = "purge" ]; then
    userdel ignite 2>/dev/null || true
    rm -rf /var/lib/ignite /run/ignite 2>/dev/null || true
fi
POSTRM
chmod 755 "${WORK_DIR}/DEBIAN/postrm"

# 8. Build Package
mkdir -p dist
dpkg-deb --build "${WORK_DIR}" "dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"

# 9. Cleanup
rm -f firecracker firecracker.tgz virtiofsd.zip virtiofsd virtiofsd_bin
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "Package created at dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"
