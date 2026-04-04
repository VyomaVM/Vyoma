#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 03: Builder (Ignitefile) ==="

check_root
setup_env

# Start Daemon
echo "Starting Daemon (3003)..."
sudo -E $IGNITED_BIN --socket-path /run/ignite/test.sock --http-port 3003 > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

IGN="$IGN_BIN --socket-path /run/ignite/test.sock --http-port 3003"

# 1. Setup Context
CTX=$TEST_HOME/build_ctx
mkdir -p $CTX
cat <<EOF > $CTX/Ignitefile
FROM alpine:latest
RUN echo "Ignite Build Test" > /build_test.txt
EOF

# 2. Build
echo "Building Image..."
# Output parsing needed? ign build currently prints to stdout?
OUTPUT=$($IGN build $CTX 2>&1)
echo "$OUTPUT"
assert_success "Build Command"

# 3. Verify Build Succeeded
# Check output contains "Build complete" or image ID
if echo "$OUTPUT" | grep -q "Build complete"; then
    echo -e "${GREEN}Pass: Build completed successfully${NC}"
else
    echo -e "${RED}Fail: Build did not complete${NC}"
    exit 1
fi

cleanup_env $DAEMON_PID
echo "=== Test 03 Passed ==="
