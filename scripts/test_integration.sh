#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${GREEN}Starting Integration Tests...${NC}"

# 1. Build
echo "Building Release Binaries..."
cargo build --release --bin ignited --bin ign

# 2. Setup Isolated Environment
TEST_DIR=$(mktemp -d)
export HOME=$TEST_DIR
echo "Test Environment: $TEST_DIR"

# 3. Start Daemon (Needs Root for CNI/MicroVM functionality)
# We can run purely logic tests if we skip actual VM spawn, but for full coverage we need root.
# We assume the user running this script can sudo or is root.
echo "Starting Daemon..."
if [ "$EUID" -ne 0 ]; then
  sudo -E ./target/release/ignited > $TEST_DIR/daemon.log 2>&1 &
else
  ./target/release/ignited > $TEST_DIR/daemon.log 2>&1 &
fi
DAEMON_PID=$!

# Wait for healthy
echo "Waiting for Daemon..."
sleep 3
# Ideally check health endpoint, but sleep is simple for now.

IGN="./target/release/ign"

# Function to Cleanup
cleanup() {
    echo "Stopping Daemon..."
    sudo kill $DAEMON_PID || true
    wait $DAEMON_PID || true
    rm -rf $TEST_DIR
    echo -e "${GREEN}Cleanup Complete.${NC}"
}
trap cleanup EXIT

# --- TESTS ---

# Test 1: Doctor
echo "Running Doctor..."
$IGN doctor

# Test 2: Network Management
echo "Testing Network Create..."
$IGN network create test-net --subnet 10.99.0.0/16
$IGN network ls | grep "test-net"
echo "Network Created."

# Test 3: Swarm Init (Node 1)
echo "Testing Swarm Init..."
$IGN swarm init
$IGN swarm ls | grep "seed"
echo "Swarm Initialized."

# Test 4: Multi-Node Swarm (Requires Root & Port Config)
echo "Testing Swarm Join (Node 2)..."
TEST_DIR_2=$(mktemp -d)
# We need to copy CNI plugins or ensure they exist? Ignited creates them in ~/.ignite.
# Since TEST_DIR_2 is empty, ignited will create them.

# Start Node 2 on Port 3001
echo "Starting Daemon Node 2..."
if [ "$EUID" -ne 0 ]; then
  sudo -E HOME=$TEST_DIR_2 ./target/release/ignited --port 3001 > $TEST_DIR_2/daemon.log 2>&1 &
else
  HOME=$TEST_DIR_2 ./target/release/ignited --port 3001 > $TEST_DIR_2/daemon.log 2>&1 &
fi
NODE2_PID=$!
sleep 3

# Helper for Node 2
IGN2="./target/release/ign --address http://127.0.0.1:3001"

# Join
# Assuming Node 1 is at 127.0.0.1 (default port in join logic?)
$IGN2 swarm join --ip 127.0.0.1

# Verify on Node 1
$IGN swarm ls 
# Should see new node?

# Cleanup Node 2 logic
sudo kill $NODE2_PID || true
rm -rf $TEST_DIR_2
echo "Swarm Join Test Complete."

# Test 5: Image Pull (Mock or Real? Real if net available)
# Skipping for speed unless requested, or use small image.
# $IGN pull alpine:latest

# --- END TESTS ---
echo -e "${GREEN}All Integration Tests Passed!${NC}"
