#!/bin/bash
set -e
source tests/e2e/common.sh

echo "=== Test 03: Builder (Vyomafile) ==="

check_root
setup_env

# Start Daemon
echo "Starting Daemon (3003)..."
sudo -E $VYOMAD_BIN --socket-path /run/vyoma/test.sock --http-port 3003 > $TEST_HOME/daemon.log 2>&1 &
DAEMON_PID=$!
sleep 3

VYOMA="$VYOMA_BIN --socket-path /run/vyoma/test.sock --http-port 3003"

# 1. Setup Context
CTX=$TEST_HOME/build_ctx
mkdir -p $CTX
cat <<EOF > $CTX/Vyomafile
FROM alpine:latest
RUN echo "Vyoma Build Test" > /build_test.txt
EOF

# 2. Build
echo "Building Image..."
# Output parsing needed? ign build currently prints to stdout?
OUTPUT=$($VYOMA build $CTX 2>&1)
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
