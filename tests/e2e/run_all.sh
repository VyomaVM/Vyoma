#!/bin/bash
set -e

# Run all tests in sequence
echo "Running All E2E Tests..."

sudo ./tests/e2e/01_lifecycle.sh
sudo ./tests/e2e/02_volumes_ports.sh
sudo ./tests/e2e/03_builder.sh
sudo ./tests/e2e/04_compose.sh
sudo ./tests/e2e/05_swarm.sh
sudo ./tests/e2e/06_network.sh
sudo ./tests/e2e/07_snapshot.sh

echo "All Tests Passed Successfully!"
