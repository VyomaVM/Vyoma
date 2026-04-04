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
    sudo pkill ignited || true
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

echo "Suite Completed."
