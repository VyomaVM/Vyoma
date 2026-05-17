#!/bin/bash

# Utility to invoke tests and track partial failures
run_test() {
    local script=$1
    echo "--------------------------------------------------"
    echo "RUNNING: $script"
    echo "--------------------------------------------------"
    if sudo -E $script; then
        echo "✅ PASS: $script"
    else
        echo "❌ FAIL: $script"
    fi
    # Cleanup between tests just in case
    sudo pkill vyomad || true
    echo ""
}

echo "Starting Full Regression Suite..."

run_test ./tests/e2e/01_lifecycle.sh
run_test ./tests/e2e/02_volumes_ports.sh
run_test ./tests/e2e/03_builder.sh
run_test ./tests/e2e/04_compose.sh
run_test ./tests/e2e/05_swarm.sh
run_test ./tests/e2e/06_network.sh
run_test ./tests/e2e/07_snapshot.sh
run_test ./tests/e2e/08_snapshot_restore.sh
run_test ./tests/e2e/09_port_forwarding.sh
run_test ./tests/e2e/10_volume_mount.sh
run_test ./tests/e2e/11_attestation.sh

echo "Suite Completed."
