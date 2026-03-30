#!/bin/bash
set -e

VERSION="1.0.0"
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

if [ ! -f "virtiofsd_bin" ]; then
    echo "Fetching virtiofsd..."
    wget -q -O virtiofsd.zip "https://gitlab.com/virtio-fs/virtiofsd/-/releases/v1.11.1/downloads/virtiofsd-v1.11.1-x86_64-musl.zip"
    unzip -q virtiofsd.zip
    mv virtiofsd virtiofsd_bin
    chmod +x virtiofsd_bin
fi

# 3. Create Source Tarball
tar -czf "${WORK_DIR}/SOURCES/ignite-${VERSION}.tar.gz" -C target/release ign ignited -C ../../packaging/systemd ignited.service -C ../../ firecracker virtiofsd_bin

# 4. Create SPEC File
cat <<EOF > "${WORK_DIR}/SPECS/ignite.spec"
Name:       ignite
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
install -m 755 virtiofsd_bin %{buildroot}/usr/libexec/ignite/virtiofsd
install -m 644 ignited.service %{buildroot}/etc/systemd/system/ignited.service

%files
/usr/bin/ignited
/usr/bin/ign
/usr/bin/firecracker
/usr/libexec/ignite/virtiofsd
/etc/systemd/system/ignited.service

%post
systemctl daemon-reload
systemctl enable ignited
systemctl start ignited

%changelog
* Mon Jan 26 2026 Subeshrock <subesh.rock.3@gmail.com> - 1.0.0-1
- Initial release
EOF

# 5. Build RPM
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/ignite.spec"

# 6. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/

# 7. Cleanup
rm -f firecracker firecracker.tgz virtiofsd.zip virtiofsd_bin
rm -rf release-v1.7.0-x86_64
rm -rf "${WORK_DIR}"

echo "RPM created in dist/"
