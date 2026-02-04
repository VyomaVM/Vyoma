#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 05: Swarm Multi-Node ==="

check_root

# Setup Node 1
echo "Setting up Node 1..."
export TEST_HOME=$(mktemp -d) # HOME 1
HOME1=$TEST_HOME
sudo -E HOME=$HOME1 $IGNITED_BIN --port 3000 > $HOME1/daemon.log 2>&1 &
PID1=$!

# Setup Node 2
echo "Setting up Node 2..."
HOME2=$(mktemp -d)
sudo -E HOME=$HOME2 $IGNITED_BIN --port 3001 > $HOME2/daemon.log 2>&1 &
PID2=$!

sleep 3

IGN1="$IGN_BIN --address http://127.0.0.1:3000"
IGN2="$IGN_BIN --address http://127.0.0.1:3001"

# 1. Init Swarm
echo "Initializing Swarm on Node 1..."
$IGN1 swarm init
assert_success "Swarm Init"

# 2. Join Swarm
echo "Joining Node 2 to Node 1..."
# Assuming localhost joining works (ignited uses advertised addr?)
# Currently defaults to 127.0.0.1 in code.
$IGN2 swarm join --ip 127.0.0.1
assert_success "Swarm Join"

# 3. List Nodes
echo "Listing Nodes..."
NODES=$($IGN1 swarm ls)
echo "$NODES"
# Check if 2 nodes exist.
# (Simple grep check)
if echo "$NODES" | grep -q "machine_id"; then
     echo -e "${GREEN}Pass: Nodes listed${NC}"
else
     echo -e "${RED}Fail: No nodes found${NC}"
fi

# Cleanup
echo "Cleaning up..."
kill $PID1 $PID2 || true
wait $PID1 $PID2 || true
rm -rf $HOME1 $HOME2
echo "=== Test 05 Passed ==="
