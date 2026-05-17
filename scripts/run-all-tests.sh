#!/bin/bash
# Vyoma Master Test Runner
# Runs all test suites: E2E, Chaos, Smoke, Compatibility Matrix
#
# Usage:
#   ./run-all-tests.sh              # Run all tests
#   ./run-all-tests.sh --clean     # Run with pre-test cleanup
#   ./run-all-tests.sh --e2e       # Run only E2E tests
#   ./run-all-tests.sh --chaos     # Run only Chaos tests
#   ./run-all-tests.sh --smoke     # Run only Smoke test
#   ./run-all-tests.sh --compat    # Run only Compatibility matrix

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CLEANUP_SCRIPT="$PROJECT_ROOT/scripts/cleanup-all.sh"

E2E_RUN=0
CHAOS_RUN=0
SMOKE_RUN=0
COMPAT_RUN=0
CLEAN_BEFORE=0

TOTAL_PASSED=0
TOTAL_FAILED=0

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --clean    Run cleanup script before tests"
    echo "  --e2e      Run E2E tests"
    echo "  --chaos    Run Chaos tests"
    echo "  --smoke    Run Smoke test"
    echo "  --compat   Run Compatibility matrix"
    echo "  --all      Run all tests (default)"
    echo "  -h, --help Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0                   # Run all tests"
    echo "  $0 --clean --e2e    # Clean, then run E2E only"
    echo "  $0 --smoke --compat # Run smoke and compat tests"
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --clean)
            CLEAN_BEFORE=1
            shift
            ;;
        --e2e)
            E2E_RUN=1
            shift
            ;;
        --chaos)
            CHAOS_RUN=1
            shift
            ;;
        --smoke)
            SMOKE_RUN=1
            shift
            ;;
        --compat)
            COMPAT_RUN=1
            shift
            ;;
        --all)
            E2E_RUN=1
            CHAOS_RUN=1
            SMOKE_RUN=1
            COMPAT_RUN=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

if [ $E2E_RUN -eq 0 ] && [ $CHAOS_RUN -eq 0 ] && [ $SMOKE_RUN -eq 0 ] && [ $COMPAT_RUN -eq 0 ]; then
    E2E_RUN=1
    CHAOS_RUN=1
    SMOKE_RUN=1
    COMPAT_RUN=1
fi

run_cleanup() {
    if [ -f "$CLEANUP_SCRIPT" ]; then
        log_info "Running cleanup script..."
        sudo bash "$CLEANUP_SCRIPT" 2>/dev/null || true
        log_info "Cleanup completed"
    else
        log_warn "Cleanup script not found: $CLEANUP_SCRIPT"
    fi
}

run_e2e_tests() {
    echo ""
    echo "=========================================="
    echo "Running E2E Test Suite"
    echo "=========================================="

    local passed=0
    local failed=0

    cd "$PROJECT_ROOT"

    for test in tests/e2e/*.sh; do
        if [ -x "$test" ]; then
            test_name=$(basename "$test")
            log_info "Running $test_name..."

            if sudo -E "$test" > /tmp/e2e-$test_name.log 2>&1; then
                log_success "$test_name passed"
                ((passed++))
            else
                log_error "$test_name failed"
                ((failed++))
                echo "  See logs: /tmp/e2e-$test_name.log"
            fi
        fi
    done

    echo ""
    echo "E2E Results: $passed passed, $failed failed"
    TOTAL_PASSED=$((TOTAL_PASSED + passed))
    TOTAL_FAILED=$((TOTAL_FAILED + failed))
}

run_chaos_tests() {
    echo ""
    echo "=========================================="
    echo "Running Chaos Tests"
    echo "=========================================="

    local passed=0
    local failed=0

    cd "$PROJECT_ROOT"

    log_info "Building with chaos feature..."
    cargo build --features chaos --package vyomad --lib 2>/dev/null || true

    log_info "Running chaos test suite..."
    if sudo VYOMAD_PATH=./target/debug/vyomad cargo test --features chaos --package vyomad --lib chaos_tests::tests > /tmp/chaos.log 2>&1; then
        log_success "Chaos tests passed"
        ((passed++))
    else
        log_error "Chaos tests failed"
        ((failed++))
        echo "  See logs: /tmp/chaos.log"
    fi

    echo ""
    echo "Chaos Results: $passed passed, $failed failed"
    TOTAL_PASSED=$((TOTAL_PASSED + passed))
    TOTAL_FAILED=$((TOTAL_FAILED + failed))
}

run_smoke_test() {
    echo ""
    echo "=========================================="
    echo "Running Smoke Test"
    echo "=========================================="

    local passed=0
    local failed=0

    if [ -x "$PROJECT_ROOT/tests/smoke/install-and-run.sh" ]; then
        log_info "Running smoke test..."
        if sudo -E "$PROJECT_ROOT/tests/smoke/install-and-run.sh" > /tmp/smoke.log 2>&1; then
            log_success "Smoke test passed"
            ((passed++))
        else
            log_error "Smoke test failed"
            ((failed++))
            echo "  See logs: /tmp/smoke.log"
        fi
    else
        log_warn "Smoke test not found or not executable"
    fi

    echo ""
    echo "Smoke Results: $passed passed, $failed failed"
    TOTAL_PASSED=$((TOTAL_PASSED + passed))
    TOTAL_FAILED=$((TOTAL_FAILED + failed))
}

run_compat_matrix() {
    echo ""
    echo "=========================================="
    echo "Running Compatibility Matrix"
    echo "=========================================="

    local passed=0
    local failed=0

    cd "$PROJECT_ROOT/tests/compat"

    log_info "Building compatibility test..."
    cargo build --package compat-matrix --release 2>/dev/null || true

    if [ -x "../../target/release/compat-matrix" ]; then
        log_info "Running compatibility matrix (20 images)..."
        if ./target/release/compat-matrix --images-file tests/compat/images.json --parallel 4 > /tmp/compat.log 2>&1; then
            log_success "Compatibility matrix passed"
            ((passed++))
        else
            log_error "Compatibility matrix failed"
            ((failed++))
            echo "  See logs: /tmp/compat.log"
        fi
    else
        log_warn "Compat matrix binary not found"
    fi

    echo ""
    echo "Compatibility Results: $passed passed, $failed failed"
    TOTAL_PASSED=$((TOTAL_PASSED + passed))
    TOTAL_FAILED=$((TOTAL_FAILED + failed))
}

echo "=========================================="
echo "Vyoma Test Suite Runner"
echo "=========================================="

if [ $CLEAN_BEFORE -eq 1 ]; then
    run_cleanup
fi

if [ $E2E_RUN -eq 1 ]; then
    run_e2e_tests
fi

if [ $CHAOS_RUN -eq 1 ]; then
    run_chaos_tests
fi

if [ $SMOKE_RUN -eq 1 ]; then
    run_smoke_test
fi

if [ $COMPAT_RUN -eq 1 ]; then
    run_compat_matrix
fi

echo ""
echo "=========================================="
echo "Test Suite Summary"
echo "=========================================="
echo -e "Total Passed: ${GREEN}$TOTAL_PASSED${NC}"
echo -e "Total Failed: ${RED}$TOTAL_FAILED${NC}"

if [ $TOTAL_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi