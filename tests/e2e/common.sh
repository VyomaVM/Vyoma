#!/bin/bash
# Common utilities for E2E tests

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

LOG_DIR="/tmp/ignite-tests-$(date +%s)"
mkdir -p $LOG_DIR

export IGNITED_BIN="$(pwd)/target/release/ignited"
export IGN_BIN="$(pwd)/target/release/ign"

if [ ! -f "$IGNITED_BIN" ]; then
    echo "Error: Binary not found at $IGNITED_BIN. Run 'cargo build --release' first."
    exit 1
fi

check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}Error: Tests must be run as root (for Firecracker/CNI).${NC}"
        # Sudo check
        sudo -n true 2>/dev/null || { echo "Please run with sudo or provide password."; exit 1; }
    fi
}

setup_env() {
    export TEST_HOME=$(mktemp -d)
    export HOME=$TEST_HOME
    echo "Test Environment: $TEST_HOME"
}

cleanup_env() {
    local pid=$1
    if [ -n "$pid" ]; then
        kill $pid || true
        wait $pid || true
    fi
    rm -rf $TEST_HOME
}

assert_success() {
    if [ $? -ne 0 ]; then
        echo -e "${RED}Test Failed: $1${NC}"
        exit 1
    else
        echo -e "${GREEN}Pass: $1${NC}"
    fi
}
