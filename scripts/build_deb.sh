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

# 3. Copy Assets
cp target/release/ignited "${WORK_DIR}/usr/bin/"
cp target/release/ign "${WORK_DIR}/usr/bin/"
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
systemctl daemon-reload
systemctl enable ignited
systemctl start ignited
echo "Ignite installed! Run 'ign doctor' to verify."
EOF
chmod 755 "${WORK_DIR}/DEBIAN/postinst"

# 6. Build Package
mkdir -p dist
dpkg-deb --build "${WORK_DIR}" "dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"

echo "Package created at dist/${PKG_NAME}_${VERSION}_${ARCH}.deb"
