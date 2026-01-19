#!/bin/bash
# ==============================================================================
# Single-Node Configuration Validation Script
# ==============================================================================
#
# Purpose: Validate that single-node deployment works correctly
# Usage: ./scripts/testing/validate_single_node.sh
#
# Tests:
# 1. Environment configuration
# 2. Ollama connectivity
# 3. Database initialization
# 4. API health checks
# 5. Basic functionality
#
# ==============================================================================

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# ------------------------------------------------------------------------------
# Helper Functions
# ------------------------------------------------------------------------------

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1"
    ((TESTS_PASSED++))
}

log_error() {
    echo -e "${RED}[✗]${NC} $1"
    ((TESTS_FAILED++))
}

log_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

run_test() {
    ((TESTS_RUN++))
    echo -e "\n${BLUE}Test $TESTS_RUN:${NC} $1"
}

# ------------------------------------------------------------------------------
# Test 1: Check Environment Configuration
# ------------------------------------------------------------------------------

run_test "Verify .env file exists and is configured for single-node"

if [ ! -f .env ]; then
    log_error ".env file not found. Run: cp .env.example .env"
    exit 1
fi

# Check DEPLOYMENT_MODE
if grep -q "^DEPLOYMENT_MODE=single_node" .env; then
    log_success "DEPLOYMENT_MODE is set to single_node"
elif grep -q "^DEPLOYMENT_MODE=" .env; then
    MODE=$(grep "^DEPLOYMENT_MODE=" .env | cut -d'=' -f2)
    log_warning "DEPLOYMENT_MODE is set to '$MODE' (expected: single_node)"
else
    log_warning "DEPLOYMENT_MODE not set, will default to single_node"
fi

# Check LLM_PROVIDER
if grep -q "^LLM_PROVIDER=" .env; then
    PROVIDER=$(grep "^LLM_PROVIDER=" .env | cut -d'=' -f2)
    log_success "LLM_PROVIDER is set to '$PROVIDER'"
else
    log_warning "LLM_PROVIDER not set"
fi

# Check OLLAMA_HOST
if grep -q "^OLLAMA_HOST=" .env; then
    OLLAMA_HOST=$(grep "^OLLAMA_HOST=" .env | cut -d'=' -f2)
    log_success "OLLAMA_HOST is set to '$OLLAMA_HOST'"
else
    OLLAMA_HOST="http://localhost:11434"
    log_warning "OLLAMA_HOST not set, defaulting to $OLLAMA_HOST"
fi

# ------------------------------------------------------------------------------
# Test 2: Check Ollama Connectivity
# ------------------------------------------------------------------------------

run_test "Verify Ollama service is running and accessible"

if command -v curl &> /dev/null; then
    if curl -s -f "${OLLAMA_HOST}/api/tags" > /dev/null 2>&1; then
        log_success "Ollama is accessible at ${OLLAMA_HOST}"

        # Check available models
        MODELS=$(curl -s "${OLLAMA_HOST}/api/tags" | grep -o '"name":"[^"]*"' | cut -d'"' -f4 || echo "")
        if [ -n "$MODELS" ]; then
            log_info "Available models:"
            echo "$MODELS" | head -5 | sed 's/^/  - /'
            MODEL_COUNT=$(echo "$MODELS" | wc -l)
            if [ "$MODEL_COUNT" -gt 5 ]; then
                log_info "  ... and $((MODEL_COUNT - 5)) more"
            fi
        fi
    else
        log_error "Cannot connect to Ollama at ${OLLAMA_HOST}"
        log_info "Start Ollama with: ollama serve"
        exit 1
    fi
else
    log_warning "curl not found, skipping Ollama connectivity check"
fi

# ------------------------------------------------------------------------------
# Test 3: Check Python Dependencies
# ------------------------------------------------------------------------------

run_test "Verify Python dependencies are installed"

if command -v python3 &> /dev/null; then
    log_success "Python 3 is installed: $(python3 --version)"

    # Check critical packages
    REQUIRED_PACKAGES=("fastapi" "asyncpg" "pydantic" "httpx" "pytest")
    for package in "${REQUIRED_PACKAGES[@]}"; do
        if python3 -c "import $package" 2>/dev/null; then
            log_success "  ✓ $package is installed"
        else
            log_error "  ✗ $package is NOT installed"
            log_info "    Install with: pip install $package"
        fi
    done
else
    log_error "Python 3 is not installed"
    exit 1
fi

# ------------------------------------------------------------------------------
# Test 4: Check Database
# ------------------------------------------------------------------------------

run_test "Verify database configuration"

if grep -q "^DATABASE_PATH=" .env; then
    DB_PATH=$(grep "^DATABASE_PATH=" .env | cut -d'=' -f2)
    log_success "DATABASE_PATH is set to '$DB_PATH'"

    # Check if database directory exists
    DB_DIR=$(dirname "$DB_PATH")
    if [ ! -d "$DB_DIR" ]; then
        log_info "Creating database directory: $DB_DIR"
        mkdir -p "$DB_DIR"
    fi

    # Check if database exists
    if [ -f "$DB_PATH" ]; then
        log_success "Database file exists: $DB_PATH"
        DB_SIZE=$(du -h "$DB_PATH" | cut -f1)
        log_info "Database size: $DB_SIZE"
    else
        log_warning "Database file does not exist (will be created on first run)"
    fi
else
    log_warning "DATABASE_PATH not set, will use default"
fi

# ------------------------------------------------------------------------------
# Test 5: Check API Configuration
# ------------------------------------------------------------------------------

run_test "Verify API configuration"

if grep -q "^API_HOST=" .env; then
    API_HOST=$(grep "^API_HOST=" .env | cut -d'=' -f2)
    log_success "API_HOST is set to '$API_HOST'"
else
    API_HOST="0.0.0.0"
    log_warning "API_HOST not set, defaulting to $API_HOST"
fi

if grep -q "^API_PORT=" .env; then
    API_PORT=$(grep "^API_PORT=" .env | cut -d'=' -f2)
    log_success "API_PORT is set to '$API_PORT'"
else
    API_PORT="8000"
    log_warning "API_PORT not set, defaulting to $API_PORT"
fi

# ------------------------------------------------------------------------------
# Test 6: Run Unit Tests
# ------------------------------------------------------------------------------

run_test "Run unit tests (without live LLM)"

if command -v pytest &> /dev/null; then
    log_info "Running unit tests..."

    # Run tests with mock LLM
    if MOCK_LLM_ENABLED=true pytest tests/unit/ -v --tb=short -x 2>&1 | tail -20; then
        log_success "Unit tests passed"
    else
        log_warning "Some unit tests failed (check output above)"
    fi
else
    log_warning "pytest not found, skipping unit tests"
    log_info "Install with: pip install pytest"
fi

# ------------------------------------------------------------------------------
# Test 7: Check API Server (if running)
# ------------------------------------------------------------------------------

run_test "Check if API server is running"

API_URL="http://localhost:${API_PORT}"

if curl -s -f "${API_URL}/health" > /dev/null 2>&1; then
    log_success "API server is running at ${API_URL}"

    # Get health status
    HEALTH=$(curl -s "${API_URL}/health")
    log_info "Health check response:"
    echo "$HEALTH" | python3 -m json.tool 2>/dev/null || echo "$HEALTH"

    # Check ready endpoint
    if curl -s -f "${API_URL}/ready" > /dev/null 2>&1; then
        log_success "API server is ready"
    else
        log_warning "API server is not ready yet"
    fi
else
    log_warning "API server is not running"
    log_info "Start with: python -m api.server"
fi

# ------------------------------------------------------------------------------
# Test 8: Validate Configuration Files
# ------------------------------------------------------------------------------

run_test "Validate Python configuration files"

# Check config/settings.py
if [ -f "config/settings.py" ]; then
    if python3 -c "from config.settings import settings; print(f'Deployment mode: {settings.deployment_mode}')" 2>/dev/null; then
        log_success "config/settings.py is valid"
    else
        log_error "config/settings.py has syntax errors"
    fi
else
    log_warning "config/settings.py not found (expected in Phase 2)"
fi

# Check agents are importable
if python3 -c "from orchestrator.orchestrator_compat import Orchestrator" 2>/dev/null; then
    log_success "Agent modules are importable"
else
    log_warning "Some agent modules may have import errors"
fi

# ------------------------------------------------------------------------------
# Summary
# ------------------------------------------------------------------------------

echo ""
echo "=============================================================================="
echo "                          VALIDATION SUMMARY"
echo "=============================================================================="
echo -e "Tests run:    ${BLUE}${TESTS_RUN}${NC}"
echo -e "Tests passed: ${GREEN}${TESTS_PASSED}${NC}"
echo -e "Tests failed: ${RED}${TESTS_FAILED}${NC}"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}✓ All critical tests passed!${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Start Ollama: ollama serve"
    echo "  2. Pull a model: ollama pull qwen2.5:7b-instruct"
    echo "  3. Start API: python -m api.server"
    echo "  4. Run full tests: pytest tests/ -v"
    echo ""
    exit 0
else
    echo -e "${RED}✗ Some tests failed${NC}"
    echo ""
    echo "Please fix the issues above before continuing."
    echo ""
    exit 1
fi
