#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VK8S_DIR="$PROJECT_ROOT/vk8s"

VYOMA_CRI_SOCKET="${VYOMA_CRI_SOCKET:-/var/run/vyoma-cri.sock}"
VYOMAD_GRPC="${VYOMAD_GRPC:-localhost:7071}"
VYOMAD_HTTP="${VYOMAD_HTTP:-http://localhost:8080}"
REPORT_DIR="${REPORT_DIR:-/tmp/critest-reports}"
CRICTL_VERSION="${CRICTL_VERSION:-v1.29.0}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }

setup_crictl() {
    log_info "Setting up crictl..."

    if ! command -v crictl &> /dev/null; then
        log_info "Installing crictl $CRICTL_VERSION..."
        curl -sSL "https://github.com/kubernetes-sigs/cri-tools/releases/download/$CRICTL_VERSION/crictl-${CRICTL_VERSION}-linux-amd64.tar.gz" | \
            tar xz -C /usr/local/bin
    fi

    mkdir -p /etc/crictl
    cat > /etc/crictl/crictl.yaml << EOF
runtime-endpoint: unix://$VYOMA_CRI_SOCKET
image-endpoint: unix://$VYOMA_CRI.sock
timeout: 120
debug: false
EOF

    log_info "crictl configured for $VYOMA_CRI_SOCKET"
}

setup_critest() {
    log_info "Setting up critest..."

    if ! command -v critest &> /dev/null; then
        log_info "Installing critest $CRICTL_VERSION..."
        curl -sSL "https://github.com/kubernetes-sigs/cri-tools/releases/download/$CRICTL_VERSION/crictl-${CRICTL_VERSION}-linux-amd64.tar.gz" | \
            tar xz -C /usr/local/bin
    fi

    log_info "critest installed"
}

check_dependencies() {
    log_info "Checking dependencies..."

    if ! command -v protoc &> /dev/null; then
        log_error "protoc not found. Install: apt install protobuf-compiler"
        exit 1
    fi

    if ! command -v curl &> /dev/null; then
        log_error "curl not found"
        exit 1
    fi

    log_info "Dependencies OK"
}

check_vyomad() {
    log_info "Checking vyomad availability..."

    if curl -s --max-time 5 "$VYOMAD_HTTP/health" > /dev/null 2>&1; then
        log_info "vyomad HTTP is responding"
    else
        log_warn "vyomad HTTP not responding at $VYOMAD_HTTP"
        log_warn "Make sure vyomad is running before tests"
    fi

    log_info "vyomad gRPC should be at $VYOMAD_GRPC"
}

check_socket() {
    if [ -S "$VYOMA_CRI_SOCKET" ]; then
        log_info "CRI socket exists: $VYOMA_CRI_SOCKET"
        ls -la "$VYOMA_CRI_SOCKET"
    else
        log_warn "CRI socket not found: $VYOMA_CRI_SOCKET"
        log_warn "Start vk8s server first"
    fi
}

run_crictl_info() {
    log_info "Running crictl info..."
    if crictl info 2>/dev/null; then
        log_info "crictl info OK"
    else
        log_warn "crictl info failed - server may not be running"
    fi
}

run_crictl_ps() {
    log_info "Running crictl ps (list containers)..."
    crictl ps 2>/dev/null || true
}

run_crictl_images() {
    log_info "Running crictl images..."
    crictl images 2>/dev/null || true
}

run_crictl_sandboxes() {
    log_info "Running crictl sandboxes..."
    crictl pods 2>/dev/null || true
}

run_podsandbox_tests() {
    log_info "=== Running PodSandbox Tests ==="
    mkdir -p "$REPORT_DIR/podsandbox"

    critest --runtime-endpoint=unix://$VYOMA_CRI_SOCKET \
            --ginkgo.focus="PodSandbox" \
            --ginkgo.skip="Alpha" \
            --parallel=1 \
            --report-dir="$REPORT_DIR/podsandbox" \
            --timeout=5m \
            2>&1 | tee "$REPORT_DIR/podsandbox/output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -eq 0 ]; then
        log_info "PodSandbox tests PASSED"
    else
        log_error "PodSandbox tests FAILED (exit code: $exit_code)"
    fi
    return $exit_code
}

run_container_tests() {
    log_info "=== Running Container Tests ==="
    mkdir -p "$REPORT_DIR/container"

    critest --runtime-endpoint=unix://$VYOMA_CRI_SOCKET \
            --ginkgo.focus="Container" \
            --ginkgo.skip="Alpha" \
            --parallel=1 \
            --report-dir="$REPORT_DIR/container" \
            --timeout=10m \
            2>&1 | tee "$REPORT_DIR/container/output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -eq 0 ]; then
        log_info "Container tests PASSED"
    else
        log_error "Container tests FAILED (exit code: $exit_code)"
    fi
    return $exit_code
}

run_image_tests() {
    log_info "=== Running Image Tests ==="
    mkdir -p "$REPORT_DIR/image"

    critest --runtime-endpoint=unix://$VYOMA_CRI_SOCKET \
            --ginkgo.focus="Image" \
            --ginkgo.skip="Alpha" \
            --parallel=1 \
            --report-dir="$REPORT_DIR/image" \
            --timeout=5m \
            2>&1 | tee "$REPORT_DIR/image/output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -eq 0 ]; then
        log_info "Image tests PASSED"
    else
        log_error "Image tests FAILED (exit code: $exit_code)"
    fi
    return $exit_code
}

run_streaming_tests() {
    log_info "=== Running Streaming Tests ==="
    mkdir -p "$REPORT_DIR/streaming"

    critest --runtime-endpoint=unix://$VYOMA_CRI_SOCKET \
            --ginkgo.focus="Exec|Attach|PortForward" \
            --ginkgo.skip="Alpha" \
            --parallel=1 \
            --report-dir="$REPORT_DIR/streaming" \
            --timeout=10m \
            2>&1 | tee "$REPORT_DIR/streaming/output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -eq 0 ]; then
        log_info "Streaming tests PASSED"
    else
        log_error "Streaming tests FAILED (exit code: $exit_code)"
    fi
    return $exit_code
}

run_full_suite() {
    log_info "=== Running Full CRI Conformance Suite ==="
    mkdir -p "$REPORT_DIR/full"

    critest --runtime-endpoint=unix://$VYOMA_CRI_SOCKET \
            --ginkgo.skip="Alpha" \
            --parallel=4 \
            --report-dir="$REPORT_DIR/full" \
            --timeout=30m \
            2>&1 | tee "$REPORT_DIR/full/output.log"

    local exit_code=${PIPESTATUS[0]}
    if [ $exit_code -eq 0 ]; then
        log_info "Full CRI conformance suite PASSED"
    else
        log_error "Full CRI conformance suite FAILED"
    fi
    return $exit_code
}

generate_report() {
    log_info "=== Generating Test Report ==="
    local report_file="$REPORT_DIR/summary.html"

    cat > "$report_file" << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <title>CRI Conformance Test Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        h1 { color: #333; }
        .test-group { margin: 20px 0; padding: 15px; border: 1px solid #ddd; border-radius: 5px; }
        .passed { background-color: #d4edda; border-color: #28a745; }
        .failed { background-color: #f8d7da; border-color: #dc3545; }
        .skipped { background-color: #fff3cd; border-color: #ffc107; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 8px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background-color: #f8f9fa; }
    </style>
</head>
<body>
    <h1>CRI Conformance Test Report</h1>
    <div id="content"></div>
    <script>
        const fs = require('fs');
        const reports = ['podsandbox', 'container', 'image', 'streaming', 'full'];
        let html = '';
        reports.forEach(name => {
            html += `<div class="test-group"><h2>${name.toUpperCase()}</h2>`;
            try {
                const data = fs.readFileSync('/tmp/critest-reports/' + name + '/output.log', 'utf8');
                if (data.includes('SUCCESS') || data.includes('passed')) {
                    html += '<p class="passed">PASSED</p>';
                } else if (data.includes('FAILED')) {
                    html += '<p class="failed">FAILED</p>';
                }
                html += '<pre>' + data.slice(-2000) + '</pre>';
            } catch(e) {
                html += '<p>No results available</p>';
            }
            html += '</div>';
        });
        document.getElementById('content').innerHTML = html;
    </script>
</body>
</html>
EOF

    log_info "Report generated at $report_file"
}

usage() {
    cat << EOF
Usage: $0 [COMMAND] [OPTIONS]

Commands:
    setup        Setup crictl and critest
    check        Check dependencies and environment
    info         Run crictl info
    podsandbox   Run PodSandbox conformance tests
    container    Run Container conformance tests
    image        Run Image conformance tests
    streaming    Run Streaming conformance tests
    full         Run full CRI conformance suite
    report       Generate test report
    all          Run all tests sequentially

Environment Variables:
    VYOMA_CRI_SOCKET   CRI socket path (default: /var/run/vyoma-cri.sock)
    VYOMAD_GRPC        vyomad gRPC address (default: localhost:7071)
    VYOMAD_HTTP        vyomad HTTP address (default: http://localhost:8080)
    REPORT_DIR         Report output directory (default: /tmp/critest-reports)

Examples:
    $0 setup                    # Setup test tools
    $0 check                    # Check environment
    $0 podsandbox               # Run PodSandbox tests
    $0 all                      # Run all tests
EOF
}

main() {
    mkdir -p "$REPORT_DIR"

    case "${1:-all}" in
        setup)
            check_dependencies
            setup_crictl
            setup_critest
            ;;
        check)
            check_dependencies
            check_vyomad
            check_socket
            run_crictl_info
            ;;
        info)
            run_crictl_info
            ;;
        ps)
            run_crictl_ps
            ;;
        images)
            run_crictl_images
            ;;
        sandboxes)
            run_crictl_sandboxes
            ;;
        podsandbox)
            run_podsandbox_tests
            ;;
        container)
            run_container_tests
            ;;
        image)
            run_image_tests
            ;;
        streaming)
            run_streaming_tests
            ;;
        full)
            run_full_suite
            ;;
        report)
            generate_report
            ;;
        all)
            check_dependencies
            setup_crictl
            setup_critest
            check_vyomad
            check_socket
            run_crictl_info

            local failed=0
            run_podsandbox_tests || failed=1
            run_container_tests || failed=1
            run_image_tests || failed=1
            run_streaming_tests || failed=1

            generate_report

            if [ $failed -eq 0 ]; then
                log_info "All tests PASSED"
            else
                log_error "Some tests FAILED"
                exit 1
            fi
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            log_error "Unknown command: $1"
            usage
            exit 1
            ;;
    esac
}

main "$@"