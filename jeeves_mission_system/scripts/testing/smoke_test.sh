#!/bin/bash
# Quick smoke test using modular libraries
# Tests basic functionality without comprehensive testing

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source shared libraries
source "$SCRIPT_DIR/../lib/common.sh"
source "$SCRIPT_DIR/../lib/testing.sh"
source "$SCRIPT_DIR/../lib/database.sh"

# Change to project root
cd "$PROJECT_ROOT"

main() {
    print_step "7-Agent Assistant - Smoke Test"
    echo ""

    # Check requirements
    check_python || exit 1
    check_venv || exit 1

    # Validate database if it exists
    if [ -f "data/memory.db" ]; then
        validate_database || print_warning "Database validation failed (continuing)"
    fi

    # Run smoke tests
    run_smoke_test
    local test_result=$?

    echo ""
    if [ $test_result -eq 0 ]; then
        print_success "Smoke test completed successfully!"
        print_info "Run './scripts/testing/run_tests.sh full' for comprehensive testing"
    else
        print_error "Smoke test failed"
        print_info "Check logs and try './scripts/testing/run_tests.sh unit -v' for details"
        exit 1
    fi
}

main
