#!/bin/bash
# Ignite Debian Package Build Script

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"
PACKAGE_DIR="$BUILD_DIR/ignite_2.1.1_amd64"
VERSION="2.1.1"

echo "Building Ignite Debian package..."
echo "Project root: $PROJECT_ROOT"

# Clean previous build
rm -rf "$BUILD_DIR"
mkdir -p "$PACKAGE_DIR"

# Build Rust binaries
echo "Building Rust binaries..."
cd "$PROJECT_ROOT"
cargo build --release

# Build UI (optional - requires Node.js)
echo "Building UI..."
if command -v npm &> /dev/null; then
    cd "$PROJECT_ROOT/ui"
    npm install
    npm run build
else
    echo "Skipping UI build (npm not found)"
fi

# Fetch virtiofsd with fallback URLs
VIRTIOFSD_VERSION="1.11.1"
echo "Fetching virtiofsd..."
if [ ! -f "virtiofsd_bin" ]; then
    # Primary: GitLab releases
    PRIMARY_URL="https://gitlab.com/virtio-fs/virtiofsd/-/releases/${VIRTIOFSD_VERSION}/downloads/virtiofsd-v${VIRTIOFSD_VERSION}-x86_64-musl.zip"
    if wget -q -O virtiofsd.zip "$PRIMARY_URL" 2>/dev/null; then
        unzip -q -o virtiofsd.zip
        mv virtiofsd virtiofsd_bin 2>/dev/null || true
        chmod +x virtiofsd_bin 2>/dev/null || true
    fi
fi

# Fallback 1
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL1="https://github.com/qemu/qemu/raw/main/contrib/virtiofsd/virtiofsd-x86_64"
    if wget -q -O virtiofsd "$FALLBACK_URL1" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Fallback 2
if [ ! -f "virtiofsd_bin" ]; then
    FALLBACK_URL2="https://raw.githubusercontent.com/qemu/qemu/master/contrib/virtiofsd/virtiofsd-x86_64"
    if wget -q -O virtiofsd "$FALLBACK_URL2" 2>/dev/null; then
        mv virtiofsd virtiofsd_bin
        chmod +x virtiofsd_bin
    fi
fi

# Create package structure
echo "Creating package structure..."
mkdir -p "$PACKAGE_DIR/usr/bin"
mkdir -p "$PACKAGE_DIR/usr/lib/ignite"
mkdir -p "$PACKAGE_DIR/usr/share/applications"
mkdir -p "$PACKAGE_DIR/usr/share/icons/hicolor/64x64/apps"
mkdir -p "$PACKAGE_DIR/usr/share/doc/ignite"
mkdir -p "$PACKAGE_DIR/DEBIAN"

# Copy binaries
cp "$PROJECT_ROOT/target/release/ign" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true
cp "$PROJECT_ROOT/target/release/ignited" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true

# Copy ignite binaries
cp "$PROJECT_ROOT/target/release/ignite-agent" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true

# Copy UI dist
if [ -d "$PROJECT_ROOT/ui/dist" ]; then
    cp -r "$PROJECT_ROOT/ui/dist" "$PACKAGE_DIR/usr/lib/ignite/ui"
fi

# Copy firecracker (if exists)
if [ -d "$PROJECT_ROOT/bin" ]; then
    cp -r "$PROJECT_ROOT/bin" "$PACKAGE_DIR/usr/lib/ignite/"
fi

# Copy virtiofsd if available
if [ -f "virtiofsd_bin" ]; then
    cp virtiofsd_bin "$PACKAGE_DIR/usr/lib/ignite/virtiofsd"
    chmod +x "$PACKAGE_DIR/usr/lib/ignite/virtiofsd"
    echo "virtiofsd bundled successfully"
else
    echo "Warning: virtiofsd not bundled - volume mounts may not work"
fi

# Copy favicon as icon
cp "$PROJECT_ROOT/ui/public/favicon.svg" "$PACKAGE_DIR/usr/share/icons/hicolor/64x64/apps/ignite.svg" 2>/dev/null || true

# Create desktop file
cat > "$PACKAGE_DIR/usr/share/applications/ignite.desktop" << 'EOF'
[Desktop Entry]
Name=Ignite
Comment=MicroVM Management Dashboard
Exec=/usr/bin/ignited
Icon=ignite
Terminal=false
Type=Application
Categories=System;Virtualization;
Keywords=microvm;virtualization;docker;
EOF

# Copy systemd service
mkdir -p "$PACKAGE_DIR/lib/systemd/system"
cp "$PROJECT_ROOT/packaging/systemd/ignited.service" "$PACKAGE_DIR/lib/systemd/system/"

# Create postinst script
cat <<'POSTINST' > "$PACKAGE_DIR/DEBIAN/postinst"
#!/bin/bash
set -e

# Update icons cache
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true

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

# Create data directory
mkdir -p /var/lib/ignite
chown ignite:ignite /var/lib/ignite 2>/dev/null || true

# Create runtime directory
mkdir -p /run/ignite
chown root:ignite /run/ignite 2>/dev/null || true
chmod 0755 /run/ignite 2>/dev/null || true

# Set socket group ownership (will be created by daemon)
chown root:ignite /run/ignite/ignite.sock 2>/dev/null || true
chmod 0660 /run/ignite/ignite.sock 2>/dev/null || true

# Add installing user to ignite group
if [ -n "$SUDO_USER" ]; then
    usermod -aG ignite "$SUDO_USER" 2>/dev/null || true
    usermod -aG kvm "$SUDO_USER" 2>/dev/null || true
    echo "Added $SUDO_USER to ignite and kvm groups. Log out and back in to use CLI."
fi

# Ensure sudoers bypass for ignited commands
if [ ! -f /etc/sudoers.d/ignite ]; then
    cat <<'SUDOERS' > /etc/sudoers.d/ignite
ignite ALL=(ALL) NOPASSWD: /usr/bin/mount, /usr/bin/umount, /usr/bin/ip, /usr/sbin/losetup, /usr/sbin/dmsetup, /usr/bin/debugfs
SUDOERS
    chmod 0440 /etc/sudoers.d/ignite
fi

# Enable and start systemd service automatically
if command -v systemctl &> /dev/null; then
    systemctl daemon-reload 2>/dev/null || true
    systemctl enable ignited.service 2>/dev/null || true
    systemctl start ignited.service 2>/dev/null || true
    echo "Ignite daemon auto-started"
fi

echo "Ignite v2.1.1 installed successfully!"
echo "Open http://localhost:3000 for the dashboard"
echo "Run 'ign run nginx:latest' to start your first VM"
POSTINST
chmod +x "$PACKAGE_DIR/DEBIAN/postinst"

# Create postrm script
cat <<'POSTRM' > "$PACKAGE_DIR/DEBIAN/postrm"
#!/bin/bash
set -e
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
if [ "$1" = "purge" ]; then
    userdel ignite 2>/dev/null || true
    rm -rf /var/lib/ignite /run/ignite 2>/dev/null || true
fi
POSTRM
chmod +x "$PACKAGE_DIR/DEBIAN/postrm"

# Create control file
cat > "$PACKAGE_DIR/DEBIAN/control" << EOF
Package: ignite
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Depends: libc6 (>= 2.34), libstdc++6 (>= 6)
Maintainer: Subeshrock <subesh.rock.3@gmail.com>
Description: Lightweight MicroVM runtime
 Ignite is a lightweight MicroVM runtime for running containers
 as lightweight virtual machines. Combines Firecracker speed with Docker UX.
 Includes CLI, Daemon, Web UI, and virtiofsd for volume mounts.
EOF

# Build package
echo "Building .deb package..."
dpkg-deb --build "$PACKAGE_DIR" "$BUILD_DIR/ignite_${VERSION}_amd64.deb"

# Cleanup virtiofsd temp files
rm -f virtiofsd.zip virtiofsd virtiofsd_bin 2>/dev/null || true

echo "Done! Package created at: $BUILD_DIR/ignite_${VERSION}_amd64.deb"
