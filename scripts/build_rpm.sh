#!/bin/bash
set -e

VERSION="2.1.0"
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

# virtiofsd is optional
if [ ! -f "virtiofsd_bin" ]; then
    echo "Skipping virtiofsd (optional dependency)"
fi

# 3. Create Source Tarball - only include files that exist
TAR_SOURCES="-C target/release ign ignited -C ../../packaging/systemd ignited.service -C ../../ firecracker"
if [ -f "virtiofsd_bin" ]; then
    TAR_SOURCES="$TAR_SOURCES virtiofsd_bin"
fi
tar -czf "${WORK_DIR}/SOURCES/ignite-${VERSION}.tar.gz" $TAR_SOURCES

# 4. Create SPEC File - start with base content
SPEC_CONTENT="Name:       ignite
Version:    ${VERSION}
Release:    1%{?dist}
Summary:    MicroVM Ecosystem
License:    MIT
URL:        https://github.com/Subeshrock/micro-vm-ecosystem
Source0:    ignite-${VERSION}.tar.gz

%description
Docker-like experience for Firecracker MicroVMs.

%prep
%setup -c

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/libexec/ignite
mkdir -p %{buildroot}/etc/systemd/system
install -m 755 ignited %{buildroot}/usr/bin/ignited
install -m 755 ign %{buildroot}/usr/bin/ign
install -m 755 firecracker %{buildroot}/usr/bin/firecracker
install -m 644 ignited.service %{buildroot}/etc/systemd/system/ignited.service"

# Add virtiofsd install if it exists
if [ -f "virtiofsd_bin" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
install -m 755 virtiofsd_bin %{buildroot}/usr/libexec/ignite/virtiofsd"
fi

SPEC_CONTENT="$SPEC_CONTENT

%files
/usr/bin/ignited
/usr/bin/ign
/usr/bin/firecracker
/etc/systemd/system/ignited.service"

# Add virtiofsd to files if it exists
if [ -f "virtiofsd_bin" ]; then
    SPEC_CONTENT="$SPEC_CONTENT
/usr/libexec/ignite/virtiofsd"
fi

# Add postinstall section
SPEC_CONTENT="$SPEC_CONTENT

%post
if ! getent group ignite > /dev/null 2\&1; then
    groupadd -r ignite
fi
if ! getent passwd ignite > /dev/null 2\&1; then
    useradd -r -s /sbin/nologin -g ignite ignite
    if getent group kvm > /dev/null 2\&1; then
        usermod -aG kvm ignite 2>/dev/null || true
    fi
    chmod 0660 /dev/kvm 2>/dev/null || true
    chown root:kvm /dev/kvm 2>/dev/null || true
fi
systemctl daemon-reload
systemctl enable ignited
systemctl start ignited

%changelog
* Tue Mar 31 2026 Subeshrock <subesh.rock.3@gmail.com> - 2.1.0-1
- Release v2.1.0
"

echo "$SPEC_CONTENT" > "${WORK_DIR}/SPECS/ignite.spec"

# 5. Build RPM
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/ignite.spec"

# 6. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/

# 7. Cleanup
rm -f firecracker firecracker.tgz virtiofsd_bin
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "RPM created in dist/"