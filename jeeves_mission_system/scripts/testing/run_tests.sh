#!/bin/bash
# Unified test runner using modular libraries
# Usage: ./run_tests.sh [mode] [options]
# Modes: quick, unit, integration, full, coverage, ollama, regression

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source shared libraries
source "$SCRIPT_DIR/../lib/common.sh"
source "$SCRIPT_DIR/../lib/testing.sh"

# Change to project root
cd "$PROJECT_ROOT"

# Parse arguments
MODE="${1:-quick}"
VERBOSE=0

# Parse options
shift || true
while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_step "7-Agent Assistant - Test Runner"
    echo ""

    # Check Python
    check_python || exit 1

    # Check venv
    check_venv || exit 1

    # Run tests based on mode
    run_tests "$MODE" "$VERBOSE"
    exit $?
}

# Show usage if help requested
if [[ "$MODE" == "-h" || "$MODE" == "--help" ]]; then
    cat << EOF
Usage: $0 [mode] [options]

Modes:
  quick       Quick smoke test (default)
  unit        Run unit tests only
  integration Run integration tests only
  full        Run all tests
  coverage    Run all tests with coverage report
  ollama      Run Ollama-specific tests
  regression  Run full regression suite with coverage

Options:
  -v, --verbose   Verbose output
  -h, --help      Show this help message

Examples:
  $0                    # Quick smoke test
  $0 full               # Run all tests
  $0 coverage           # Run with coverage
  $0 unit -v            # Run unit tests verbosely

EOF
    exit 0
fi

# Run main
main
