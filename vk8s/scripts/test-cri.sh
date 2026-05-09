#!/bin/bash
set -e

SOCKET_PATH="${VYOMA_CRI_SOCKET:-/var/run/vyoma-cri.sock}"
VYOMAD_ADDR="${VYOMAD_ADDR:-localhost:7071}"
VYOMAD_HTTP="${VYOMAD_HTTP:-http://localhost:8080}"

echo "=== Vyoma CRI Integration Tests ==="
echo "Socket: $SOCKET_PATH"
echo "Vyomad gRPC: $VYOMAD_ADDR"
echo "Vyomad HTTP: $VYOMAD_HTTP"
echo ""

check_crictl() {
    if ! command -v crictl &> /dev/null; then
        echo "crictl not found. Installing..."
        curl -sSL https://github.com/kubernetes-sigs/cri-tools/releases/download/v1.29.0/crictl-v1.29.0-linux-amd64.tar.gz | tar xz -C /usr/local/bin
    fi
    crictl --version
}

check_critest() {
    if ! command -v critest &> /dev/null; then
        echo "critest not found. Installing..."
        curl -sSL https://github.com/kubernetes-sigs/cri-tools/releases/download/v1.29.0/critest-v1.29.0-linux-amd64.tar.gz | tar xz -C /usr/local/bin
    fi
    critest --version
}

setup_crictl_config() {
    cat > /etc/crictl.yaml << EOF
runtime-endpoint: unix://$SOCKET_PATH
image-endpoint: unix://$SOCKET_PATH
timeout: 120
debug: true
EOF
    echo "Created /etc/crictl.yaml"
}

check_vyomad() {
    echo "Checking vyomad availability..."
    if curl -s "$VYOMAD_HTTP/health" > /dev/null; then
        echo "vyomad is running"
    else
        echo "Warning: vyomad HTTP not responding at $VYOMAD_HTTP"
    fi
}

run_crictl_tests() {
    echo ""
    echo "=== Running crictl tests ==="
    
    echo "Info:"
    crictl info || true
    
    echo ""
    echo "List sandboxes:"
    crictl ps -s || true
    
    echo ""
    echo "List images:"
    crictl images || true
}

run_critest() {
    echo ""
    echo "=== Running critest (CRI conformance) ==="
    
    critest --runtime-endpoint=unix://$SOCKET_PATH \
            --ginkgo.focus="PodSandbox" \
            --ginkgo.skip="Alpha" \
            --parallel=1 \
            --report-dir=/tmp/critest-report || true
    
    echo ""
    echo "Full critest run (may take time):"
    read -p "Run full critest suite? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        critest --runtime-endpoint=unix://$SOCKET_PATH \
                --report-dir=/tmp/critest-report-full
    fi
}

main() {
    check_crictl
    check_critest
    setup_crictl_config
    check_vyomad
    run_crictl_tests
    run_critest
    
    echo ""
    echo "=== Tests complete ==="
    echo "Reports available at /tmp/critest-report/"
}

main "$@"