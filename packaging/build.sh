#!/bin/bash
# Ignite Unified Package Build Script
# Builds both DEB and RPM packages

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="$PROJECT_ROOT/build"

usage() {
    echo "Usage: $0 [deb|rpm|all]"
    echo "  deb   - Build Debian package"
    echo "  rpm   - Build RPM package"  
    echo "  all   - Build both packages (default)"
    exit 1
}

BUILD_TYPE="${1:-all}"

case "$BUILD_TYPE" in
    deb)
        "$PROJECT_ROOT/packaging/deb/build.sh"
        ;;
    rpm)
        "$PROJECT_ROOT/packaging/rpm/build.sh"
        ;;
    all)
        "$PROJECT_ROOT/packaging/deb/build.sh"
        "$PROJECT_ROOT/packaging/rpm/build.sh"
        ;;
    *)
        usage
        ;;
esac

echo ""
echo "=== Build Summary ==="
ls -lh "$BUILD_DIR"/*.deb "$BUILD_DIR"/RPMS/x86_64/*.rpm 2>/dev/null || true