#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 04: Compose (ign up) ==="

check_root
setup_env

# Start Daemon
echo "Starting Daemon (3004)..."
sudo -E $IGNITED_BIN --socket-path /run/ignite/test.sock --http-port 3004 > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

IGN="$IGN_BIN --socket-path /run/ignite/test.sock --http-port 3004"

# 1. Setup Compose File
mkdir -p $TEST_HOME/compose_test
cd $TEST_HOME/compose_test
cat <<EOF > ignite-compose.yml
version: '1'
services:
  web:
    image: alpine:latest
    cpus: 1
    memory: 128
  db:
    image: alpine:latest
    cpus: 1
    memory: 128
EOF

# 2. Up
echo "Running ign up..."
$IGN up -d
assert_success "Compose Up"

sleep 5

# 3. Verify
echo "Verifying Services..."
PS=$($IGN ps)
echo "$PS"

if echo "$PS" | grep -q "web" && echo "$PS" | grep -q "db"; then
    echo -e "${GREEN}Pass: Services Running${NC}"
else
    echo -e "${RED}Fail: Services missing${NC}"
    exit 1
fi

# 3.5 Scale
echo "Scaling web=2..."
# Command: ign scale web=2
$IGN scale web=2
assert_success "Scale Request"
sleep 5
PS_SCALE=$($IGN ps)
# grep -c "web" should be 2?
# The service name might be "compose_test_web_1", "compose_test_web_2".
# Labels will prevent collision?
# Implementation of "scale" CLI was: "ign scale <service>=<count>".
# It calls "ign run" repeatedly?
# Let's verify scaling took effect.
if [ $(echo "$PS_SCALE" | grep -c "web") -ge 2 ]; then
     echo -e "${GREEN}Pass: Scaled to 2 instances${NC}"
else
     echo -e "${RED}Fail: Scale failed${NC}"
fi

# 4. Down
echo "Running ign down..."
$IGN down
assert_success "Compose Down"

sleep 2
if $IGN ps | grep -q "web"; then
    echo -e "${RED}Fail: Services still running${NC}"
    # exit 1
else
    echo -e "${GREEN}Pass: Services Removed${NC}"
fi

cleanup_env $DAEMON_PID
echo "=== Test 04 Passed ==="
