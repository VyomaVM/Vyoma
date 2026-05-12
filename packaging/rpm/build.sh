#!/bin/bash
# Vyoma RPM Package Build Script

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"
PACKAGE_DIR="$BUILD_DIR/vyoma_2.1.1_x86_64"
VERSION="2.1.2"

echo "Building Vyoma RPM package..."
echo "Project root: $PROJECT_ROOT"

# Check for rpmbuild
if ! command -v rpmbuild &> /dev/null; then
    echo "rpmbuild not found. Install rpm-build package:"
    echo "  Fedora/RHEL: sudo dnf install rpm-build"
    echo "  Debian/Ubuntu: sudo apt install rpm"
    exit 1
fi

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
mkdir -p "$PACKAGE_DIR/usr/lib/vyoma"
mkdir -p "$PACKAGE_DIR/usr/share/applications"
mkdir -p "$PACKAGE_DIR/usr/share/icons/hicolor/64x64/apps"
mkdir -p "$PACKAGE_DIR/usr/share/doc/ignite"
mkdir -p "$PACKAGE_DIR/SOURCES"

# Copy binaries
cp "$PROJECT_ROOT/target/release/ign" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true
cp "$PROJECT_ROOT/target/release/vyomad" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true
cp "$PROJECT_ROOT/target/release/ignite-agent" "$PACKAGE_DIR/usr/bin/" 2>/dev/null || true

# Copy UI dist
if [ -d "$PROJECT_ROOT/ui/dist" ]; then
    cp -r "$PROJECT_ROOT/ui/dist" "$PACKAGE_DIR/usr/lib/vyoma/ui"
fi

# Copy firecracker (if exists)
if [ -d "$PROJECT_ROOT/bin" ]; then
    cp -r "$PROJECT_ROOT/bin" "$PACKAGE_DIR/usr/lib/vyoma/"
fi

# Copy virtiofsd if available
if [ -f "virtiofsd_bin" ]; then
    cp virtiofsd_bin "$PACKAGE_DIR/usr/lib/vyoma/virtiofsd"
    chmod +x "$PACKAGE_DIR/usr/lib/vyoma/virtiofsd"
    echo "virtiofsd bundled successfully"
else
    echo "Warning: virtiofsd not bundled - volume mounts may not work"
fi

# Copy favicon as icon
if [ -f "$PROJECT_ROOT/ui/public/favicon.svg" ]; then
    cp "$PROJECT_ROOT/ui/public/favicon.svg" "$PACKAGE_DIR/usr/share/icons/hicolor/64x64/apps/ignite.svg"
fi

# Create desktop file
cat > "$PACKAGE_DIR/usr/share/applications/vyoma.desktop" << 'EOF'
[Desktop Entry]
Name=Vyoma
Comment=MicroVM Management Dashboard
Exec=/usr/bin/vyomad
Icon=ignite
Terminal=false
Type=Application
Categories=System;Virtualization;
Keywords=microvm;virtualization;docker;
EOF

# Copy systemd service
mkdir -p "$PACKAGE_DIR/lib/systemd/system"
cp "$PROJECT_ROOT/packaging/systemd/vyomad.service" "$PACKAGE_DIR/lib/systemd/system/"

# Create RPM SPEC file
cat > "$BUILD_DIR/ignite.spec" << SPECFILE
Name:           ignite
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Lightweight MicroVM runtime
License:        MIT
URL:            https://github.com/Subeshrock/micro-vm-ecosystem
BuildArch:      x86_64

Requires:       libc >= 2.34, libstdc++ >= 6.0
Requires:       kvm
Requires(post): systemd
Requires(post): systemd-sysv

%description
Vyoma is a lightweight MicroVM runtime for running containers
as lightweight virtual machines. Combines Firecracker speed with Docker UX.
Includes CLI, Daemon, Web UI, and virtiofsd for volume mounts.

%prep
# Nothing to prep

%build
# Already built by cargo

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/vyoma
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/icons/hicolor/64x64/apps
mkdir -p %{buildroot}/usr/share/doc/ignite
mkdir -p %{buildroot}/lib/systemd/system

# Copy binaries
cp -r usr/bin/* %{buildroot}/usr/bin/

# Copy UI
if [ -d usr/lib/ignite/ui ]; then
    cp -r usr/lib/ignite/ui %{buildroot}/usr/lib/vyoma/
fi

# Copy firecracker binaries
if [ -d usr/lib/ignite/bin ]; then
    cp -r usr/lib/ignite/bin %{buildroot}/usr/lib/vyoma/
fi

# Copy virtiofsd
if [ -f usr/lib/ignite/virtiofsd ]; then
    cp usr/lib/ignite/virtiofsd %{buildroot}/usr/lib/vyoma/
fi

# Copy icon
if [ -f usr/share/icons/hicolor/64x64/apps/ignite.svg ]; then
    cp usr/share/icons/hicolor/64x64/apps/ignite.svg %{buildroot}/usr/share/icons/hicolor/64x64/apps/
fi

# Copy desktop file
cp usr/share/applications/vyoma.desktop %{buildroot}/usr/share/applications/

# Copy systemd service
cp lib/systemd/system/vyomad.service %{buildroot}/lib/systemd/system/

%files
%defattr(-,root,root,-)
/usr/bin/ign
/usr/bin/vyomad
/usr/bin/ignite-agent
%dir /usr/lib/vyoma
%if exists(usr/lib/ignite/ui)
/usr/lib/vyoma/ui
%endif
%if exists(usr/lib/ignite/bin)
/usr/lib/vyoma/bin
%endif
%if exists(usr/lib/ignite/virtiofsd)
/usr/lib/vyoma/virtiofsd
%endif
/usr/share/applications/vyoma.desktop
/usr/share/icons/hicolor/64x64/apps/ignite.svg
/lib/systemd/system/vyomad.service

%post
# Create ignite user (for socket ownership)
if ! getent passwd ignite > /dev/null 2>&1; then
    useradd -r -s /sbin/nologin -c "Vyoma MicroVM Daemon" -d /var/lib/vyoma ignite 2>/dev/null || true
fi

# Add ignite daemon user to kvm group (for /dev/kvm access)
if getent group kvm > /dev/null 2>&1; then
    usermod vyoma 2>/dev/null || true
fi

# Fix /dev/kvm permissions
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

# Create data directory
mkdir -p /var/lib/vyoma
chown ignite:ignite /var/lib/vyoma 2>/dev/null || true

# Create runtime directory
mkdir -p /run/ignite
chown root:ignite /run/ignite 2>/dev/null || true
chmod 0755 /run/ignite 2>/dev/null || true

# Add installing user to ignite and kvm groups
if [ -n "$USER" ]; then
    usermod vyoma $USER 2>/dev/null || true
    usermod -aG kvm $USER 2>/dev/null || true
fi

# Enable and start systemd service
systemctl daemon-reload 2>/dev/null || true
systemctl enable vyomad.service 2>/dev/null || true
systemctl start vyomad.service 2>/dev/null || true

echo "Vyoma v${VERSION} installed successfully!"
echo "Open http://localhost:3000 for the dashboard"
echo "Run 'ign run nginx:latest' to start your first VM"

%postun
# Reload systemd on removal
if [ \$1 -eq 0 ]; then
    userdel ignite 2>/dev/null || true
    rm -rf /var/lib/vyoma /run/ignite 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
fi

%changelog
* Tue Mar 31 2026 Subeshrock <subesh.rock.3@gmail.com> - ${VERSION}-1
- Bundle virtiofsd for volume mounts
- Fix KVM permissions and socket ownership
- Add ignite user to kvm group
- Improve post-install setup
SPECFILE

# Change to build directory and build RPM
cd "$BUILD_DIR"
cp -r "$PACKAGE_DIR" SOURCES

echo "Building RPM package..."
rpmbuild --define "_topdir $BUILD_DIR" \
         --define "_rpmdir $BUILD_DIR/RPMS" \
         -bb "$BUILD_DIR/ignite.spec"

# Cleanup virtiofsd temp files
rm -f virtiofsd.zip virtiofsd virtiofsd_bin 2>/dev/null || true

echo "Done! Package created at: $BUILD_DIR/RPMS/x86_64/ignite-${VERSION}-1.x86_64.rpm"
