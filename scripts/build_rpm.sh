#!/bin/bash
set -e

VERSION="2.1.2"
WORK_DIR="target/rpm"
SOURCES_DIR="${WORK_DIR}/SOURCES/ignite-${VERSION}"

# Clean up any previous build
rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
mkdir -p "${SOURCES_DIR}"

# 1. Build Binaries
cargo build --release --bin ignited --bin ign

# 2. Copy binaries to source dir
cp target/release/ignited "${SOURCES_DIR}/"
cp target/release/ign "${SOURCES_DIR}/"

# 3. Fetch Dependencies
echo "Fetching dependencies..."
if [ ! -f "firecracker" ]; then
    wget -q -O firecracker.tgz https://github.com/firecracker-microvm/firecracker/releases/download/v1.7.0/firecracker-v1.7.0-x86_64.tgz
    tar -xzf firecracker.tgz
    mv release-v1.7.0-x86_64/firecracker-v1.7.0-x86_64 firecracker
    chmod +x firecracker
fi
cp firecracker "${SOURCES_DIR}/"
cp packaging/systemd/ignited.service "${SOURCES_DIR}/"

# Fetch CNI plugins
CNI_VERSION="v1.5.1"
CNI_DIR="${SOURCES_DIR}/cni/bin"
echo "Fetching CNI plugins..."
mkdir -p "${CNI_DIR}"
if [ ! -f "cni-plugins.tgz" ]; then
    wget -q -O cni-plugins.tgz "https://github.com/containernetworking/plugins/releases/download/${CNI_VERSION}/cni-plugins-linux-amd64-${CNI_VERSION}.tgz"
fi
tar -xzf cni-plugins.tgz -C "${CNI_DIR}/"

# Copy UI (built by workflow step or locally)
echo "Bundling UI..."
UI_AVAILABLE=false
if [ -d "ui/dist" ]; then
    mkdir -p "${SOURCES_DIR}/ui/dist"
    cp -r ui/dist/* "${SOURCES_DIR}/ui/dist/"
    echo "UI bundled successfully"
    UI_AVAILABLE=true
else
    echo "Warning: UI not found - dashboard will not be available"
fi

# Copy kernel binary
if [ -f "bin/vmlinux" ]; then
    mkdir -p "${SOURCES_DIR}/bin"
    cp bin/vmlinux "${SOURCES_DIR}/bin/vmlinux"
    echo "Kernel binary bundled"
else
    echo "Warning: bin/vmlinux not found - VMs will not start"
fi

# Bundle virtiofsd (from system or project bin)
if [ -f "bin/virtiofsd" ] && [ ! -L "bin/virtiofsd" ]; then
    cp bin/virtiofsd "${SOURCES_DIR}/virtiofsd"
    chmod +x "${SOURCES_DIR}/virtiofsd"
    echo "virtiofsd bundled from project bin"
elif [ -f "/usr/libexec/virtiofsd" ]; then
    cp /usr/libexec/virtiofsd "${SOURCES_DIR}/virtiofsd"
    chmod +x "${SOURCES_DIR}/virtiofsd"
    echo "virtiofsd bundled from system"
elif [ -f "/usr/bin/virtiofsd" ]; then
    cp /usr/bin/virtiofsd "${SOURCES_DIR}/virtiofsd"
    chmod +x "${SOURCES_DIR}/virtiofsd"
    echo "virtiofsd bundled from /usr/bin"
else
    echo "Warning: virtiofsd not found - volume mounts may not work"
fi

# 4. Create Source Tarball
cd "${WORK_DIR}/SOURCES"
tar -czf "ignite-${VERSION}.tar.gz" "ignite-${VERSION}"
cd ../../..

# 5. Create SPEC File
SPEC_CONTENT="Name:       ignite
Version:    ${VERSION}
Release:    1%{?dist}
Summary:    MicroVM Ecosystem
License:    MIT
URL:        https://github.com/Subeshrock/micro-vm-ecosystem
Source0:    ignite-${VERSION}.tar.gz

%description
Ignite is a lightweight MicroVM runtime for running containers
as lightweight virtual machines. Combines Firecracker speed with Docker UX.
Includes CLI, Daemon, Web UI, CNI plugins, and virtiofsd for volume mounts.

%prep
%setup -q

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/ignite
mkdir -p %{buildroot}/etc/systemd/system
install -m 755 ignited %{buildroot}/usr/bin/ignited
install -m 755 ign %{buildroot}/usr/bin/ign
install -m 755 firecracker %{buildroot}/usr/bin/firecracker
install -m 644 ignited.service %{buildroot}/etc/systemd/system/ignited.service

# Install CNI plugins
cp -r cni %{buildroot}/usr/lib/ignite/cni

# Install kernel binary
mkdir -p %{buildroot}/var/lib/ignite/bin
install -m 755 bin/vmlinux %{buildroot}/var/lib/ignite/bin/vmlinux"

if [ "$UI_AVAILABLE" = true ]; then
    SPEC_CONTENT="$SPEC_CONTENT

# Install UI
cp -r ui %{buildroot}/usr/lib/ignite/ui"
fi

if [ -f "${SOURCES_DIR}/virtiofsd" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
install -m 755 virtiofsd %{buildroot}/usr/lib/ignite/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%files
/usr/bin/ignited
/usr/bin/ign
/usr/bin/firecracker
/etc/systemd/system/ignited.service
/usr/lib/ignite/cni
/var/lib/ignite/bin/vmlinux"

if [ "$UI_AVAILABLE" = true ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/lib/ignite/ui"
fi

if [ -f "${SOURCES_DIR}/virtiofsd" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/lib/ignite/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%post
# Create ignite user (for socket ownership)
if ! getent passwd ignite > /dev/null 2>&1; then
    useradd -r -s /sbin/nologin -c \"Ignite MicroVM Daemon\" -d /var/lib/ignite ignite 2>/dev/null || true
fi

# Add installing user to ignite and kvm groups
if [ -n \"\$SUDO_USER\" ]; then
    usermod -aG ignite \$SUDO_USER 2>/dev/null || true
    usermod -aG kvm \$SUDO_USER 2>/dev/null || true
    echo \"Added \$SUDO_USER to ignite and kvm groups. Log out and back in to use CLI.\"
elif [ -n \"\$USER\" ]; then
    usermod -aG ignite \$USER 2>/dev/null || true
    usermod -aG kvm \$USER 2>/dev/null || true
    echo \"Added \$USER to ignite and kvm groups. Log out and back in to use CLI.\"
fi

# Add ignite daemon user to kvm group (for /dev/kvm access)
if getent group kvm > /dev/null 2>&1; then
    usermod -aG kvm ignite 2>/dev/null || true
fi

# Fix /dev/kvm permissions
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true

# Create runtime directory - owned by root:ignite, group writable (0775)
mkdir -p /run/ignite
chown root:ignite /run/ignite 2>/dev/null || true
chmod 0775 /run/ignite 2>/dev/null || true

# Setup CNI plugins directory (symlink from system data dir to package location)
rm -rf /var/lib/ignite/.ignite/cni/bin 2>/dev/null || true
ln -sf /usr/lib/ignite/cni/bin /var/lib/ignite/.ignite/cni/bin 2>/dev/null || true

systemctl daemon-reload 2>/dev/null || true
systemctl enable ignited.service 2>/dev/null || true
systemctl start ignited.service 2>/dev/null || true

echo \"Ignite v\${VERSION} installed successfully!\"
echo \"Open http://localhost:3000 for the dashboard\"
echo \"Run 'ign run nginx:latest' to start your first VM\"

%postun
if [ \$1 -eq 0 ]; then
    # Package removal - cleanup
    userdel ignite 2>/dev/null || true
    rm -rf /var/lib/ignite /run/ignite 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
fi

%changelog
* Wed Apr 01 2026 Subeshrock <subesh.rock.3@gmail.com> - \${VERSION}-1
- Run daemon as root with capabilities (no sudo needed)
- Remove sudoers configuration
- Fix socket and KVM permissions
- Bundle CNI plugins for networking
- Bundle UI in package
- Fix /run/ignite directory permissions (0775)
"

echo "$SPEC_CONTENT" > "${WORK_DIR}/SPECS/ignite.spec"

# 6. Build RPM
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/ignite.spec"

# 7. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/

# 8. Cleanup
rm -f firecracker firecracker.tgz virtiofsd.zip virtiofsd virtiofsd_bin cni-plugins.tgz
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "RPM created in dist/"
