#!/bin/bash
# Ignite Debian Package Build Script

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"
PACKAGE_DIR="$BUILD_DIR/ignite_2.1.0_amd64"

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

# Copy favicon as icon
cp "$PROJECT_ROOT/ui/public/favicon.svg" "$PACKAGE_DIR/usr/share/icons/hicolor/64x64/apps/ignite.svg"

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
cat > "$PACKAGE_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
# Post-installation script

# Update icons cache
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true

# Create ignite user if not exists
id ignite >/dev/null 2>&1 || useradd -r -s /sbin/nologin -d /var/lib/ignite ignite 2>/dev/null || true

# Create data directory
mkdir -p /var/lib/ignite
chown ignite:ignite /var/lib/ignite 2>/dev/null || true

# Create runtime directory
mkdir -p /run/ignite
chown ignite:ignite /run/ignite 2>/dev/null || true

# Add installing user to ignite group (must logout/login to take effect)
if [ -n "$SUDO_USER" ]; then
    usermod -aG ignite "$SUDO_USER" 2>/dev/null || true
    echo "Added $SUDO_USER to ignite group. Log out and back in to use CLI."
fi

# Enable and start systemd service automatically
if command -v systemctl &> /dev/null; then
    systemctl daemon-reload
    systemctl enable ignited.service 2>/dev/null || true
    systemctl start ignited.service 2>/dev/null || true
    echo "Ignite daemon auto-started"
fi

echo "Ignite installed successfully!"
echo "Open http://localhost:3000 for the dashboard"
EOF
chmod +x "$PACKAGE_DIR/DEBIAN/postinst"

# Create postrm script
cat > "$PACKAGE_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/bash
# Post-removal script
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
EOF
chmod +x "$PACKAGE_DIR/DEBIAN/postrm"

# Create control file
cat > "$PACKAGE_DIR/DEBIAN/control" << 'EOF'
Package: ignite
Version: 2.1.0
Section: utils
Priority: optional
Architecture: amd64
Depends: libc6 (>= 2.34), libstdc++6 (>= 6)
Maintainer: Subeshrock <subesh.rock.3@gmail.com>
Description: Lightweight MicroVM runtime
 Ignite is a lightweight MicroVM runtime for running containers
 as lightweight virtual machines. It combines the security of
 virtualization with the speed of containers.
EOF

# Build package
echo "Building .deb package..."
dpkg-deb --build "$PACKAGE_DIR" "$BUILD_DIR/ignite_2.1.0_amd64.deb"

echo "Done! Package created at: $BUILD_DIR/ignite_2.1.0_amd64.deb"
