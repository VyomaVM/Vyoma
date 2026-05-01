#!/bin/bash
set -e

VERSION="2.1.2"
WORK_DIR="target/rpm"
SOURCES_DIR="${WORK_DIR}/SOURCES/vyoma-${VERSION}"

# Clean up any previous build
rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
mkdir -p "${SOURCES_DIR}"

# 1. Build Binaries
cargo build --release --bin vyomad --bin ign

# 2. Copy binaries to source dir
cp target/release/vyomad "${SOURCES_DIR}/"
cp target/release/vyoma "${SOURCES_DIR}/"

# 3. Fetch Dependencies
echo "Fetching dependencies..."
if [ ! -f "cloud-hypervisor" ]; then
    wget -q -O cloud-hypervisor https://github.com/cloud-hypervisor/cloud-hypervisor/releases/download/v41.0/cloud-hypervisor
    chmod +x cloud-hypervisor
fi
cp cloud-hypervisor "${SOURCES_DIR}/"
cp packaging/systemd/vyomad.service "${SOURCES_DIR}/"

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
tar -czf "vyoma-${VERSION}.tar.gz" "vyoma-${VERSION}"
cd ../../..

# 5. Create SPEC File
SPEC_CONTENT="Name:       vyoma
Version:    ${VERSION}
Release:    1%{?dist}
Summary:    MicroVM Ecosystem
License:    MIT
URL:        https://github.com/Subeshrock/micro-vm-ecosystem
Source0:    vyoma-${VERSION}.tar.gz

%description
Vyoma is a lightweight MicroVM runtime for running containers
as lightweight virtual machines. Combines Cloud Hypervisor speed with Docker UX.
Includes CLI, Daemon, Web UI, CNI plugins, and virtiofsd for volume mounts.

%prep
%setup -q

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/vyoma
mkdir -p %{buildroot}/etc/systemd/system
install -m 755 vyomad %{buildroot}/usr/bin/vyomad
install -m 755 vyoma %{buildroot}/usr/bin/ign
install -m 755 cloud-hypervisor %{buildroot}/usr/bin/cloud-hypervisor
install -m 644 vyomad.service %{buildroot}/etc/systemd/system/vyomad.service

# Install CNI plugins
cp -r cni %{buildroot}/usr/lib/vyoma/cni

# Install kernel binary
mkdir -p %{buildroot}/var/lib/vyoma/bin
install -m 755 bin/vmlinux %{buildroot}/var/lib/vyoma/bin/vmlinux"

if [ "$UI_AVAILABLE" = true ]; then
    SPEC_CONTENT="$SPEC_CONTENT

# Install UI
cp -r ui %{buildroot}/usr/lib/vyoma/ui"
fi

if [ -f "${SOURCES_DIR}/virtiofsd" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
install -m 755 virtiofsd %{buildroot}/usr/lib/vyoma/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%files
/usr/bin/vyomad
/usr/bin/ign
/usr/bin/cloud-hypervisor
/etc/systemd/system/vyomad.service
/usr/lib/vyoma/cni
/var/lib/vyoma/bin/vmlinux"

if [ "$UI_AVAILABLE" = true ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/lib/vyoma/ui"
fi

if [ -f "${SOURCES_DIR}/virtiofsd" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/lib/vyoma/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%post
# Create vyoma user (for socket ownership)
if ! getent passwd vyoma > /dev/null 2>&1; then
    useradd -r -s /sbin/nologin -c \"Vyoma MicroVM Daemon\" -d /var/lib/vyoma vyoma 2>/dev/null || true
fi

# Add installing user to vyoma and kvm groups
if [ -n \"\$SUDO_USER\" ]; then
    usermod -aG vyoma \$SUDO_USER 2>/dev/null || true
    usermod -aG kvm \$SUDO_USER 2>/dev/null || true
    echo \"Added \$SUDO_USER to vyoma and kvm groups. Log out and back in to use CLI.\"
elif [ -n \"\$USER\" ]; then
    usermod -aG vyoma \$USER 2>/dev/null || true
    usermod -aG kvm \$USER 2>/dev/null || true
    echo \"Added \$USER to vyoma and kvm groups. Log out and back in to use CLI.\"
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
rm -rf /var/lib/vyoma/.vyoma/cni/bin 2>/dev/null || true
ln -sf /usr/lib/vyoma/cni/bin /var/lib/vyoma/.vyoma/cni/bin 2>/dev/null || true

systemctl daemon-reload 2>/dev/null || true
systemctl enable vyomad.service 2>/dev/null || true
systemctl start vyomad.service 2>/dev/null || true

echo \"Vyoma v\${VERSION} installed successfully!\"
echo \"Open http://localhost:3000 for the dashboard\"
echo \"Run 'vyoma run nginx:latest' to start your first VM\"

%postun
if [ \$1 -eq 0 ]; then
    # Package removal - cleanup
    userdel vyoma 2>/dev/null || true
    rm -rf /var/lib/vyoma /run/vyoma 2>/dev/null || true
    systemctl daemon-reload 2>/dev/null || true
fi

%changelog
* Wed Apr 01 2026 Subeshrock <subesh.rock.3@gmail.com> - \${VERSION}-1
- Run daemon as root with capabilities (no sudo needed)
- Remove sudoers configuration
- Fix socket and KVM permissions
- Bundle CNI plugins for networking
- Bundle UI in package
- Fix /run/vyoma directory permissions (0775)
"

echo "$SPEC_CONTENT" > "${WORK_DIR}/SPECS/vyoma.spec"

# 6. Build RPM
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/vyoma.spec"

# 7. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/

# 8. Cleanup
rm -f cloud-hypervisor cloud-hypervisor.tgz virtiofsd.zip virtiofsd virtiofsd_bin cni-plugins.tgz
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "RPM created in dist/"
