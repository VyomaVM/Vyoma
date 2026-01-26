#!/bin/bash
set -e

VERSION="1.0.0"
WORK_DIR="target/rpm"
mkdir -p "${WORK_DIR}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# 1. Build Binaries
cargo build --release --bin ignited --bin ign

# 2. Copy Binaries to "SOURCES" (Simulating source tarball or just direct file access)
# For simplicity, we will copy binaries in %install phase from absolute paths or current dir.
# But rpmbuild usually expects SOURCES.
# Let's create a tarball of the binaries to be 'clean'.
tar -czf "${WORK_DIR}/SOURCES/ignite-${VERSION}.tar.gz" -C target/release ign ignited -C ../../packaging/systemd ignited.service

# 3. Create SPEC File
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
mkdir -p %{buildroot}/etc/systemd/system
install -m 755 ignited %{buildroot}/usr/bin/ignited
install -m 755 ign %{buildroot}/usr/bin/ign
install -m 644 ignited.service %{buildroot}/etc/systemd/system/ignited.service

%files
/usr/bin/ignited
/usr/bin/ign
/etc/systemd/system/ignited.service

%post
systemctl daemon-reload
systemctl enable ignited
systemctl start ignited

%changelog
* Mon Jan 26 2026 Subeshrock <subesh.rock.3@gmail.com> - 0.9.0-1
- Initial release
EOF

# 4. Build RPM
# Define _topdir to point to our local target/rpm
rpmbuild --define "_topdir $(pwd)/${WORK_DIR}" -bb "${WORK_DIR}/SPECS/ignite.spec"

# 5. Move to dist
mkdir -p dist
mv ${WORK_DIR}/RPMS/x86_64/*.rpm dist/
echo "RPM created in dist/"
