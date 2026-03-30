#!/bin/bash
set -e

VERSION="1.0.0" # Match Cargo.toml
ARCH="amd64"
PKG_NAME="ignite"
WORK_DIR="target/debian/${PKG_NAME}_${VERSION}_${ARCH}"

echo "Building Ignite v${VERSION} for Debian..."

# 1. Build Binaries
cargo build --release --bin ignited --bin ign

# 2. Prepare Directory Structure
mkdir -p "${WORK_DIR}/usr/bin"
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

if [ ! -f "virtiofsd" ]; then
    echo "Fetching virtiofsd..."
    wget -q -O virtiofsd.zip "https://gitlab.com/virtio-fs/virtiofsd/-/releases/v1.11.1/downloads/virtiofsd-v1.11.1-x86_64-musl.zip"
    unzip -q virtiofsd.zip
    mv virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
fi

# Virtiofsd (Assuming availability or skipping for now if complex - but v1.0 needs it for volumes)
# For MVP packaging, we stick to firecracker. User can install plugins via 'ign doctor --fix' later?
# A true usage needs CNI plugins too.
# Let's bundling 'firecracker' at least.

# 4. Copy Assets
mkdir -p "${WORK_DIR}/usr/libexec/ignite"
cp target/release/ignited "${WORK_DIR}/usr/bin/"
cp target/release/ign "${WORK_DIR}/usr/bin/"
cp firecracker "${WORK_DIR}/usr/bin/firecracker"
cp virtiofsd_bin "${WORK_DIR}/usr/libexec/ignite/virtiofsd"
cp packaging/systemd/ignited.service "${WORK_DIR}/etc/systemd/system/"

# 4. Create Control File
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
 Includes Daemon, CLI, and Web UI.
EOF

# 5. Create Post-Install Script
cat <<EOF > "${WORK_DIR}/DEBIAN/postinst"
#!/bin/bash
# Create group and user (ADR-022)
# Add ignite to kvm group for /dev/kvm access (ADR-029)
if getent group kvm > /dev/null 2>&1; then
    usermod -aG kvm ignite 2>/dev/null || true
fi
# Fix /dev/kvm permissions if it exists
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

if ! getent group ignite > /dev/null 2>&1; then
    groupadd --system ignite
fi
if ! getent passwd ignite > /dev/null 2>&1; then
    useradd --system --shell /usr/sbin/nologin --gid ignite ignite
fi

# Ensure sudoers bypass for ignited commands as ignite user
cat <<'SUDOERS' > /etc/sudoers.d/ignite
ignite ALL=(ALL) NOPASSWD: /usr/bin/mount, /usr/bin/umount, /usr/bin/ip, /usr/sbin/losetup, /usr/sbin/dmsetup, /usr/bin/debugfs
SUDOERS
chmod 0440 /etc/sudoers.d/ignite

systemctl daemon-reload
systemctl enable ignited
systemctl start ignited
echo "Ignite installed! Run 'sudo usermod -aG ignite \$USER' to use the CLI without root."
EOF
chmod 755 "${WORK_DIR}/DEBIAN/postinst"

# 6. Build Package
mkdir -p dist
dpkg-deb --build "${WORK_DIR}" "dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"

# 7. Cleanup
rm -f firecracker firecracker.tgz virtiofsd.zip virtiofsd_bin
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "Package created at dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"
