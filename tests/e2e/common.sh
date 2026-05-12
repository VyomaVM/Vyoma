#!/bin/bash
# Common utilities for E2E tests

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

LOG_DIR="/tmp/vyoma-tests-$(date +%s)"
mkdir -p $LOG_DIR

export VYOMAD_BIN="$(pwd)/target/release/vyomad"
export VYOMA_BIN="$(pwd)/target/release/vyoma"
export REAL_HOME=$HOME

if [ ! -f "$VYOMAD_BIN" ]; then
    echo "Error: Binary not found at $VYOMAD_BIN. Run 'cargo build --release' first."
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
    
    # Install CNI Plugins
    mkdir -p $TEST_HOME/.vyoma/cni/bin
    if [ -d "$REAL_HOME/.vyoma/cni/bin" ] && [ "$(ls -A $REAL_HOME/.vyoma/cni/bin)" ]; then
         echo "Copying CNI plugins from $REAL_HOME..."
         cp $REAL_HOME/.vyoma/cni/bin/* $TEST_HOME/.vyoma/cni/bin/
    fi
    
    # Fallback: Download or Copy from System
    if [ -d "/usr/lib/cni" ]; then
        cp /usr/lib/cni/* $TEST_HOME/.vyoma/cni/bin/
    elif [ -d "/opt/cni/bin" ]; then
        cp /opt/cni/bin/* $TEST_HOME/.vyoma/cni/bin/
    else 
        echo -e "${RED}CNI Plugins not found. Downloading...${NC}"
        curl -sL https://github.com/containernetworking/plugins/releases/download/v1.3.0/cni-plugins-linux-amd64-v1.3.0.tgz | tar -xz -C $TEST_HOME/.vyoma/cni/bin
    fi
}

cleanup_env() {
    local pid=$1
    if [ -n "$pid" ]; then
        kill $pid || true
        wait $pid || true
    fi
    pkill -P $$ vyomad || true
    # Cleanup DM and loops
    sudo dmsetup remove_all || true
    losetup -D || true
    rm -rf $TEST_HOME
}

handle_error() {
    echo -e "${RED}Test Error - Cleaning up...${NC}"
    # Try to find daemon pid from var if exported?
    # Hard to get PID from here if not passed.
    # We rely on pkill vyomad in test setup or aggressive cleanup.
    pkill vyomad || true
    rm -rf /tmp/vyoma-tests-*
}
# trap handle_error ERR

assert_success() {
    if [ $? -ne 0 ]; then
        echo -e "${RED}Test Failed: $1${NC}"
        exit 1
    else
        echo -e "${GREEN}Pass: $1${NC}"
    fi
}
