#!/bin/bash
set -e

VERSION="2.1.1"
WORK_DIR="target/rpm"
mkdir -p "${WORK_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# 1. Build Binaries
cargo build --release --bin ignited --bin ign

# 2. Fetch Dependencies
echo "Fetching dependencies..."
if [ ! -f "firecracker" ]; then
    wget -q -O firecracker.tgz https://github.com/firecracker-microvm/firecracker/releases/download/v1.7.0/firecracker-v1.7.0-x86_64.tgz
    tar -xzf firecracker.tgz
    mv release-v1.7.0-x86_64/firecracker-v1.7.0-x86_64 firecracker
    chmod +x firecracker
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

if [ -f "virtiofsd_bin" ]; then
    echo "virtiofsd fetched successfully"
else
    echo "Warning: virtiofsd not available - volume mounts may not work"
fi

# 3. Create Source Tarball
TAR_SOURCES="-C target/release ign ignited -C ../../packaging/systemd ignited.service -C ../../ firecracker"
if [ -f "virtiofsd_bin" ]; then
    TAR_SOURCES="$TAR_SOURCES virtiofsd_bin"
fi
tar -czf "${WORK_DIR}/SOURCES/ignite-${VERSION}.tar.gz" $TAR_SOURCES

# 4. Create SPEC File
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
Includes CLI, Daemon, Web UI, and virtiofsd for volume mounts.

%prep
%setup -c

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/lib/ignite
mkdir -p %{buildroot}/etc/systemd/system
install -m 755 ignited %{buildroot}/usr/bin/ignited
install -m 755 ign %{buildroot}/usr/bin/ign
install -m 755 firecracker %{buildroot}/usr/bin/firecracker
install -m 644 ignited.service %{buildroot}/etc/systemd/system/ignited.service"

if [ -f "virtiofsd_bin" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
install -m 755 virtiofsd_bin %{buildroot}/usr/lib/ignite/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%files
/usr/bin/ignited
/usr/bin/ign
/usr/bin/firecracker
/etc/systemd/system/ignited.service"

if [ -f "virtiofsd_bin" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/lib/ignite/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%post
# Create ignite user if not exists
if ! getent passwd ignite > /dev/null 2>&1; then
    useradd -r -s /sbin/nologin -c \"Ignite MicroVM Daemon\" -d /var/lib/ignite ignite 2>/dev/null || true
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

# Ensure sudoers bypass for ignited commands
if [ ! -f /etc/sudoers.d/ignite ]; then
    cat <<'SUDOERS' > /etc/sudoers.d/ignite
ignite ALL=(ALL) NOPASSWD: /usr/bin/mount, /usr/bin/umount, /usr/bin/ip, /usr/sbin/losetup, /usr/sbin/dmsetup, /usr/bin/debugfs
SUDOERS
    chmod 0440 /etc/sudoers.d/ignite
fi

systemctl daemon-reload 2>/dev/null || true
systemctl enable ignited.service 2>/dev/null || true
systemctl start ignited.service 2>/dev/null || true

echo \"Ignite v${VERSION} installed successfully!\"
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
* Tue Mar 31 2026 Subeshrock <subesh.rock.3@gmail.com> - ${VERSION}-1
- Bundle virtiofsd for volume mounts
- Fix KVM permissions and socket ownership
- Add ignite user to kvm group
- Improve post-install setup
"

echo "$SPEC_CONTENT" > "${WORK_DIR}/SPECS/ignite.spec"

# 5. Build RPM
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/ignite.spec"

# 6. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/

# 7. Cleanup
rm -f firecracker firecracker.tgz virtiofsd.zip virtiofsd virtiofsd_bin
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "RPM created in dist/"
