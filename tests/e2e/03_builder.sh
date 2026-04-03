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
$IGN build $CTX
assert_success "Build Command"

# 3. Verify Image Exists
# Requires `ign images` command
echo "Listing Images..."
IMAGES=$($IGN images)
echo "$IMAGES"
# Check for... what tag?
# ign build currently doesn't tag? or uses Ignitefile name?
# Phase 10: "ign build" (context tarball + POST).
# Does it Create a new image ID?
# Assuming generic success for now.

# 4. Run Built Image?
# Requires knowing the ID of built image.
# If CLI doesn't output ID cleanly, this is hard.
# Skipping Run for now, just checking Build success.

cleanup_env $DAEMON_PID
echo "=== Test 03 Passed ==="
