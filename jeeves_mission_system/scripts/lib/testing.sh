#!/bin/bash
# Testing utilities library
# Source common.sh before using this library

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common functions if not already loaded
if [ -z "$(type -t print_info)" ]; then
    source "$SCRIPT_DIR/common.sh"
fi

# Run unit tests
run_unit_tests() {
    local verbose="${1:-0}"

    print_step "Running unit tests..."

    activate_venv || return 1

    if [ "$verbose" = "1" ]; then
        pytest tests/unit/ -v --tb=short
    else
        pytest tests/unit/ -q
    fi

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Unit tests passed"
    else
        print_error "Unit tests failed"
    fi
    return $result
}

# Run integration tests
run_integration_tests() {
    local verbose="${1:-0}"

    print_step "Running integration tests..."

    activate_venv || return 1

    if [ "$verbose" = "1" ]; then
        pytest tests/integration/ -v --tb=short
    else
        pytest tests/integration/ -q
    fi

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Integration tests passed"
    else
        print_error "Integration tests failed"
    fi
    return $result
}

# Run all tests
run_all_tests() {
    local verbose="${1:-0}"
    local coverage="${2:-0}"

    print_step "Running all tests..."

    activate_venv || return 1

    local pytest_args="tests/"

    if [ "$verbose" = "1" ]; then
        pytest_args="$pytest_args -v --tb=short"
    else
        pytest_args="$pytest_args -q"
    fi

    if [ "$coverage" = "1" ]; then
        pytest_args="$pytest_args --cov=. --cov-report=term-missing --cov-report=html"
        print_info "Coverage report will be generated"
    fi

    pytest $pytest_args

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "All tests passed"
        if [ "$coverage" = "1" ]; then
            print_info "Coverage report saved to htmlcov/index.html"
        fi
    else
        print_error "Some tests failed"
    fi
    return $result
}

# Run quick smoke test
run_smoke_test() {
    print_step "Running smoke test..."

    activate_venv || return 1

    # Run a minimal set of fast tests
    pytest tests/unit/ -k "not slow" -q --tb=line

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Smoke test passed"
    else
        print_error "Smoke test failed"
    fi
    return $result
}

# Run tests with specific marker
run_tests_with_marker() {
    local marker="$1"
    local verbose="${2:-0}"

    if [ -z "$marker" ]; then
        print_error "Usage: run_tests_with_marker <marker> [verbose]"
        return 1
    fi

    print_step "Running tests with marker: $marker..."

    activate_venv || return 1

    if [ "$verbose" = "1" ]; then
        pytest tests/ -m "$marker" -v --tb=short
    else
        pytest tests/ -m "$marker" -q
    fi

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Tests with marker '$marker' passed"
    else
        print_error "Tests with marker '$marker' failed"
    fi
    return $result
}

# Run tests for Ollama
run_ollama_tests() {
    local verbose="${1:-0}"

    print_step "Running Ollama tests..."

    # Set Ollama environment
    export LLM_PROVIDER=ollama
    export OLLAMA_HOST="${OLLAMA_HOST:-http://localhost:11434}"

    activate_venv || return 1

    if [ "$verbose" = "1" ]; then
        pytest tests/ -m "ollama" -v --tb=short
    else
        pytest tests/ -m "ollama" -q
    fi

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Ollama tests passed"
    else
        print_error "Ollama tests failed"
    fi
    return $result
}

# Check API health
check_api_health() {
    local host="${1:-localhost}"
    local port="${2:-8000}"
    local timeout="${3:-5}"

    print_step "Checking API health at http://$host:$port/health..."

    if ! command_exists curl; then
        print_warning "curl not found, skipping health check"
        return 1
    fi

    local response=$(curl -s -o /dev/null -w "%{http_code}" --max-time $timeout "http://$host:$port/health")

    if [ "$response" = "200" ]; then
        print_success "API is healthy (HTTP 200)"
        return 0
    else
        print_error "API is not healthy (HTTP $response)"
        return 1
    fi
}

# Run regression suite
run_regression_suite() {
    print_step "Running regression test suite..."

    activate_venv || return 1

    # Run all tests with coverage
    pytest tests/ -v --cov=. --cov-report=term-missing --cov-report=html --cov-report=json

    local result=$?
    if [ $result -eq 0 ]; then
        print_success "Regression suite passed"
        print_info "Coverage reports:"
        print_info "  - HTML: htmlcov/index.html"
        print_info "  - JSON: coverage.json"
    else
        print_error "Regression suite failed"
    fi
    return $result
}

# Clean test artifacts
clean_test_artifacts() {
    print_step "Cleaning test artifacts..."

    rm -rf .pytest_cache
    rm -rf htmlcov
    rm -f .coverage
    rm -f coverage.json
    rm -rf tests/__pycache__
    rm -rf tests/*/__pycache__

    print_success "Test artifacts cleaned"
}

# Run tests based on mode
run_tests() {
    local mode="${1:-quick}"
    local verbose="${2:-0}"

    case "$mode" in
        quick|smoke)
            run_smoke_test
            ;;
        unit)
            run_unit_tests "$verbose"
            ;;
        integration)
            run_integration_tests "$verbose"
            ;;
        full|all)
            run_all_tests "$verbose" 0
            ;;
        coverage)
            run_all_tests "$verbose" 1
            ;;
        ollama)
            run_ollama_tests "$verbose"
            ;;
        regression)
            run_regression_suite
            ;;
        *)
            print_error "Unknown test mode: $mode"
            print_info "Available modes: quick, unit, integration, full, coverage, ollama, regression"
            return 1
            ;;
    esac
}

# Export functions
export -f run_unit_tests
export -f run_integration_tests
export -f run_all_tests
export -f run_smoke_test
export -f run_tests_with_marker
export -f run_ollama_tests
export -f check_api_health
export -f run_regression_suite
export -f clean_test_artifacts
export -f run_tests
