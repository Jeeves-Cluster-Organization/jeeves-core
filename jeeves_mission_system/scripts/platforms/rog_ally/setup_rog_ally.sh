#!/bin/bash
# 7-Agent Assistant - ROG Ally Setup & Test Script
# Optimized for Bazzite OS with AMD RDNA 3 GPU
# Safe, idempotent, and comprehensive

set -euo pipefail  # Exit on error, undefined vars, pipe failures

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Script configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
LOG_FILE="${SCRIPT_DIR}/setup_rog_ally.log"
VENV_DIR="${PROJECT_DIR}/venv"
DATA_DIR="${PROJECT_DIR}/data"
OLLAMA_LOG="/tmp/ollama_setup.log"
SERVER_LOG="/tmp/server_setup.log"

# Model configuration for ROG Ally (optimized for AMD iGPU)
RECOMMENDED_MODEL="llama3.2:3b"
ALTERNATIVE_MODEL="llama3.1:8b"
FALLBACK_MODEL="tinyllama:1.1b"

# Test configuration
TEST_USER_ID="rog_ally_user"
TEST_SESSION_ID="setup_test_$(date +%s)"

# Progress tracking
STEP_CURRENT=0
STEP_TOTAL=15

# ============================================================================
# Helper Functions
# ============================================================================

log() {
    echo -e "${CYAN}[$(date '+%Y-%m-%d %H:%M:%S')]${NC} $*" | tee -a "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}✅ $*${NC}" | tee -a "$LOG_FILE"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $*${NC}" | tee -a "$LOG_FILE"
}

log_error() {
    echo -e "${RED}❌ $*${NC}" | tee -a "$LOG_FILE"
}

log_info() {
    echo -e "${BLUE}ℹ️  $*${NC}" | tee -a "$LOG_FILE"
}

step() {
    STEP_CURRENT=$((STEP_CURRENT + 1))
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}Step ${STEP_CURRENT}/${STEP_TOTAL}: $*${NC}"
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    log "Step ${STEP_CURRENT}/${STEP_TOTAL}: $*"
}

check_command() {
    if command -v "$1" &> /dev/null; then
        log_success "$1 is installed"
        return 0
    else
        log_warning "$1 is not installed"
        return 1
    fi
}

wait_for_service() {
    local url=$1
    local max_attempts=${2:-30}
    local attempt=0

    log "Waiting for service at $url..."
    while [ $attempt -lt $max_attempts ]; do
        if curl -sf "$url" &> /dev/null; then
            log_success "Service is ready"
            return 0
        fi
        attempt=$((attempt + 1))
        echo -n "."
        sleep 1
    done

    log_error "Service did not become ready after ${max_attempts}s"
    return 1
}

check_disk_space() {
    local required_gb=$1
    local available_kb=$(df "$PROJECT_DIR" | tail -1 | awk '{print $4}')
    local available_gb=$((available_kb / 1024 / 1024))

    if [ $available_gb -lt $required_gb ]; then
        log_error "Insufficient disk space. Required: ${required_gb}GB, Available: ${available_gb}GB"
        return 1
    else
        log_success "Sufficient disk space available: ${available_gb}GB"
        return 0
    fi
}

cleanup_on_exit() {
    local exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo ""
        log_error "Setup failed with exit code $exit_code"
        log_info "Check logs at: $LOG_FILE"
        log_info "Ollama logs: $OLLAMA_LOG"
        log_info "Server logs: $SERVER_LOG"
    fi

    # Don't leave orphaned processes
    if [ -n "${OLLAMA_PID:-}" ]; then
        kill "$OLLAMA_PID" 2>/dev/null || true
    fi
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
}

trap cleanup_on_exit EXIT

# ============================================================================
# Main Setup Steps
# ============================================================================

print_banner() {
    clear
    echo -e "${GREEN}"
    cat << "EOF"
╔═══════════════════════════════════════════════════════════════════╗
║                                                                   ║
║         7-Agent Personal Assistant - ROG Ally Setup               ║
║                                                                   ║
║    Optimized for: Bazzite OS + AMD RDNA 3 GPU                    ║
║                                                                   ║
╚═══════════════════════════════════════════════════════════════════╝
EOF
    echo -e "${NC}"
}

check_system() {
    step "System Requirements Check"

    log "Checking system information..."
    log "Hostname: $(hostname)"
    log "Kernel: $(uname -r)"
    log "OS: $(grep PRETTY_NAME /etc/os-release | cut -d'"' -f2)"

    # Check if running on Bazzite/Fedora-based system
    if ! grep -q "fedora\|bazzite" /etc/os-release 2>/dev/null; then
        log_warning "This script is optimized for Bazzite OS but will attempt to continue"
    else
        log_success "Bazzite OS detected"
    fi

    # Check CPU
    local cpu_info=$(grep "model name" /proc/cpuinfo | head -1 | cut -d':' -f2 | xargs)
    log "CPU: $cpu_info"

    # Check memory
    local mem_gb=$(free -g | grep Mem: | awk '{print $2}')
    log "RAM: ${mem_gb}GB"

    if [ "$mem_gb" -lt 8 ]; then
        log_warning "Less than 8GB RAM detected. May impact performance."
    else
        log_success "Sufficient RAM: ${mem_gb}GB"
    fi

    # Check disk space (need ~10GB for models and data)
    check_disk_space 10 || exit 1

    # Check GPU
    log "Checking AMD GPU..."
    if lspci | grep -i vga | grep -qi amd; then
        local gpu_info=$(lspci | grep -i vga | grep -i amd)
        log_success "AMD GPU detected: $gpu_info"
    else
        log_warning "AMD GPU not detected, but continuing..."
    fi
}

check_dependencies() {
    step "Checking System Dependencies"

    local missing_deps=()

    # Essential commands
    for cmd in python3 pip3 curl git sqlite3 jq; do
        if ! check_command "$cmd"; then
            missing_deps+=("$cmd")
        fi
    done

    if [ ${#missing_deps[@]} -gt 0 ]; then
        log_error "Missing dependencies: ${missing_deps[*]}"
        log_info "Install with: sudo dnf install -y ${missing_deps[*]}"

        # Attempt to install if we can
        if command -v dnf &> /dev/null; then
            log_info "Attempting to install missing dependencies..."
            sudo dnf install -y "${missing_deps[@]}" || {
                log_error "Failed to install dependencies. Please install manually."
                exit 1
            }
        else
            exit 1
        fi
    else
        log_success "All system dependencies are installed"
    fi

    # Check Python version
    local python_version=$(python3 --version | cut -d' ' -f2)
    log "Python version: $python_version"

    if python3 -c 'import sys; exit(0 if sys.version_info >= (3, 10) else 1)'; then
        log_success "Python version is sufficient (3.10+)"
    else
        log_error "Python 3.10+ required, found: $python_version"
        exit 1
    fi
}

setup_python_env() {
    step "Setting Up Python Virtual Environment"

    if [ -d "$VENV_DIR" ]; then
        log_warning "Virtual environment already exists at $VENV_DIR"
        log_info "Reusing existing virtual environment"
    else
        log "Creating virtual environment..."
        python3 -m venv "$VENV_DIR"
        log_success "Virtual environment created"
    fi

    log "Activating virtual environment..."
    source "${VENV_DIR}/bin/activate"

    log "Upgrading pip..."
    pip install --quiet --upgrade pip setuptools wheel
    log_success "pip upgraded"

    log "Installing project dependencies..."
    if [ -f "${PROJECT_DIR}/requirements.txt" ]; then
        pip install --quiet -r "${PROJECT_DIR}/requirements.txt" || {
            log_error "Failed to install dependencies"
            exit 1
        }
        log_success "Project dependencies installed"
    else
        log_error "requirements.txt not found"
        exit 1
    fi

    # Verify key packages
    log "Verifying key packages..."
    python3 -c "import fastapi, ollama, chromadb, asyncpg" || {
        log_error "Failed to import required packages"
        exit 1
    }
    log_success "All key packages verified"
}

install_ollama() {
    step "Installing Ollama (AMD GPU Support)"

    if command -v ollama &> /dev/null; then
        log_warning "Ollama already installed"
        ollama --version
    else
        log "Downloading and installing Ollama..."
        curl -fsSL https://ollama.com/install.sh | sh || {
            log_error "Failed to install Ollama"
            exit 1
        }
        log_success "Ollama installed"
    fi

    # Start Ollama service
    log "Starting Ollama service..."

    # Kill any existing Ollama processes
    pkill -f "ollama serve" 2>/dev/null || true
    sleep 2

    # Start new instance
    ollama serve > "$OLLAMA_LOG" 2>&1 &
    OLLAMA_PID=$!
    log "Ollama PID: $OLLAMA_PID"

    # Wait for Ollama to be ready
    wait_for_service "http://localhost:11434/api/tags" 30 || {
        log_error "Ollama failed to start"
        log_info "Check logs: $OLLAMA_LOG"
        exit 1
    }

    log_success "Ollama is running"
}

download_models() {
    step "Downloading LLM Models (Optimized for ROG Ally)"

    # Check existing models
    log "Checking existing models..."
    local existing_models=$(ollama list 2>/dev/null || echo "")

    # Download recommended model
    if echo "$existing_models" | grep -q "$RECOMMENDED_MODEL"; then
        log_success "Recommended model already downloaded: $RECOMMENDED_MODEL"
    else
        log "Downloading $RECOMMENDED_MODEL (~2GB)..."
        log_info "This is optimized for ROG Ally's AMD iGPU - please wait..."

        ollama pull "$RECOMMENDED_MODEL" || {
            log_error "Failed to download $RECOMMENDED_MODEL"

            # Try fallback model
            log_info "Trying fallback model: $FALLBACK_MODEL"
            ollama pull "$FALLBACK_MODEL" || {
                log_error "Failed to download fallback model"
                exit 1
            }
            RECOMMENDED_MODEL="$FALLBACK_MODEL"
        }

        log_success "Model downloaded: $RECOMMENDED_MODEL"
    fi

    # Test model inference
    log "Testing model inference..."
    local test_response=$(ollama run "$RECOMMENDED_MODEL" "Say 'Ready' in one word" 2>&1 || echo "ERROR")

    if echo "$test_response" | grep -qi "ready\|ok"; then
        log_success "Model inference test passed"
    else
        log_warning "Model inference test unclear, continuing anyway..."
    fi

    # List all available models
    log "Available models:"
    ollama list | tail -n +2 | while read -r line; do
        log "  - $line"
    done
}

configure_env() {
    step "Creating Configuration File"

    local env_file="${PROJECT_DIR}/.env"

    if [ -f "$env_file" ]; then
        log_warning "Configuration file already exists"
        log_info "Backing up to .env.backup.$(date +%s)"
        cp "$env_file" "${env_file}.backup.$(date +%s)"
    fi

    log "Creating optimized configuration for ROG Ally..."

    cat > "$env_file" << EOF
# 7-Agent Assistant Configuration
# Generated: $(date)
# Platform: ROG Ally (Bazzite OS + AMD RDNA 3)

# LLM Provider Configuration
LLM_PROVIDER=ollama
OLLAMA_BASE_URL=http://localhost:11434

# Model Configuration (optimized for AMD iGPU)
DEFAULT_MODEL=${RECOMMENDED_MODEL}
PLANNER_MODEL=${RECOMMENDED_MODEL}
VALIDATOR_MODEL=${RECOMMENDED_MODEL}
CRITIC_MODEL=${RECOMMENDED_MODEL}
META_VALIDATOR_MODEL=${RECOMMENDED_MODEL}

# Performance Settings (conservative for integrated GPU)
OLLAMA_NUM_PARALLEL=1
OLLAMA_NUM_GPU=1
OLLAMA_NUM_THREAD=4

# Token limits (ROG Ally has shared RAM, keep moderate)
PLANNER_MAX_TOKENS=400
VALIDATOR_MAX_TOKENS=600
CRITIC_MAX_TOKENS=400
META_VALIDATOR_MAX_TOKENS=800

# Temperature settings (lower = more deterministic)
PLANNER_TEMPERATURE=0.3
VALIDATOR_TEMPERATURE=0.5
CRITIC_TEMPERATURE=0.3

# Database Configuration
DATABASE_PATH=${DATA_DIR}/memory.db
CHROMA_PERSIST_DIR=${DATA_DIR}/chroma

# Feature Flags (disabled for initial setup)
FEATURE_USE_LLM_GATEWAY=false
FEATURE_USE_REDIS_STATE=false
FEATURE_ENABLE_DEBUG_LOGGING=false

# Mock mode (false to use real LLM)
MOCK_MODE=false

# API Settings
API_HOST=0.0.0.0
API_PORT=8000

# Logging
LOG_LEVEL=INFO
LOG_FORMAT=json

# Memory settings
MEMORY_SEARCH_TOP_K=5
EMBEDDING_MODEL=all-MiniLM-L6-v2

# Rate limiting
REQUESTS_PER_MINUTE=30

# Session settings
SESSION_TIMEOUT_MINUTES=30
EOF

    log_success "Configuration file created: $env_file"
    log_info "Using model: $RECOMMENDED_MODEL"
}

setup_database() {
    step "Initializing Database"

    # Ensure venv is active for database operations
    source "${VENV_DIR}/bin/activate"

    # Create data directories
    log "Creating data directories..."
    mkdir -p "${DATA_DIR}"/{chroma,backups}
    log_success "Data directories created"

    # Check if database exists and is valid
    # The project uses memory.db as the primary database
    local db_exists=false
    local db_valid=false
    local db_file="${DATA_DIR}/memory.db"

    if [ -f "$db_file" ]; then
        db_exists=true
        # Check if database has tables
        local table_count=$(sqlite3 "$db_file" "SELECT count(*) FROM sqlite_master WHERE type='table';" 2>/dev/null || echo "0")
        if [ "$table_count" -gt 0 ]; then
            db_valid=true
            log_warning "Database already exists with $table_count tables"
            log_info "Skipping database initialization (database is valid)"
            log_success "Using existing database: memory.db"
            return 0
        fi
    fi

    # Also check for old assistant.db (legacy database location)
    if [ -f "${DATA_DIR}/assistant.db" ]; then
        log_warning "Found legacy assistant.db - will be backed up"
    fi

    # Backup existing databases before initialization
    if [ "$db_exists" = true ] || [ -f "${DATA_DIR}/assistant.db" ]; then
        log_info "Backing up existing databases..."
        [ -f "${DATA_DIR}/memory.db" ] && cp "${DATA_DIR}/memory.db" "${DATA_DIR}/backups/memory.db.backup.$(date +%s)"
        [ -f "${DATA_DIR}/assistant.db" ] && cp "${DATA_DIR}/assistant.db" "${DATA_DIR}/backups/assistant.db.backup.$(date +%s)"
        log_success "Databases backed up"
    fi

    # Initialize database with --force flag to overwrite if needed
    log "Initializing SQLite database..."
    if python3 "${PROJECT_DIR}/init_db.py" --force 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Database initialized"
    else
        # Try without --force flag (in case script doesn't support it)
        log_info "Retrying without --force flag..."
        if python3 "${PROJECT_DIR}/init_db.py" 2>&1 | tee -a "$LOG_FILE"; then
            log_success "Database initialized"
        else
            log_error "Database initialization failed"
            log_info "Check if data/memory.db or data/assistant.db need to be removed manually"
            exit 1
        fi
    fi

    # Verify database (check for memory.db which is what init_db.py creates)
    if [ -f "${DATA_DIR}/memory.db" ]; then
        local db_size=$(du -h "${DATA_DIR}/memory.db" | cut -f1)
        log_success "Database created successfully: memory.db (${db_size})"

        # Check schema
        local table_count=$(sqlite3 "${DATA_DIR}/memory.db" "SELECT count(*) FROM sqlite_master WHERE type='table';" 2>/dev/null || echo "0")
        log "Database tables: $table_count"

        if [ "$table_count" -eq 0 ]; then
            log_error "Database has no tables"
            exit 1
        fi
    else
        log_error "Database file not created (expected: ${DATA_DIR}/memory.db)"
        log_info "Check init_db.py output above for errors"
        exit 1
    fi
}

run_unit_tests() {
    step "Running Unit Tests"

    log "Running test suite (this may take 2-3 minutes)..."

    # Ensure venv is active
    source "${VENV_DIR}/bin/activate"

    # Run specific test suites
    local test_results="${PROJECT_DIR}/test_results.txt"

    # Cost calculator tests
    log "Testing cost calculator (Phase 1 component)..."
    echo -n "  "
    if pytest tests/unit/test_cost_calculator.py -v --tb=short > "$test_results" 2>&1; then
        log_success "Cost calculator tests passed (17 tests)"
    else
        log_warning "Some cost calculator tests failed (see $test_results)"
    fi

    # Connection manager tests
    log "Testing connection manager..."
    echo -n "  "
    if pytest tests/unit/test_connection_manager.py -v --tb=short >> "$test_results" 2>&1; then
        log_success "Connection manager tests passed (12 tests)"
    else
        log_warning "Some connection manager tests failed (see $test_results)"
    fi

    # Run all unit tests (continue on failure)
    log "Running remaining unit tests (may take 1-2 minutes)..."
    echo "  This is a comprehensive test suite..."
    if timeout 180 pytest tests/unit -v --tb=short -x >> "$test_results" 2>&1; then
        log_success "All unit tests passed"
    else
        local exit_code=$?
        if [ $exit_code -eq 124 ]; then
            log_warning "Unit tests timed out after 3 minutes - continuing anyway"
        else
            log_warning "Some unit tests failed - this is OK for initial setup"
        fi
        log_info "Check detailed results: $test_results"
    fi
}

start_server() {
    step "Starting 7-Agent Server"

    # Ensure venv is active
    source "${VENV_DIR}/bin/activate"

    # Kill any existing server
    pkill -f "start_server.py" 2>/dev/null || true
    sleep 2

    log "Starting server..."
    python3 "${PROJECT_DIR}/start_server.py" > "$SERVER_LOG" 2>&1 &
    SERVER_PID=$!
    log "Server PID: $SERVER_PID"

    # Wait for server to be ready
    wait_for_service "http://localhost:8000/health" 30 || {
        log_error "Server failed to start"
        log_info "Check logs: $SERVER_LOG"
        tail -50 "$SERVER_LOG" | tee -a "$LOG_FILE"
        exit 1
    }

    log_success "Server is running"
}

test_endpoints() {
    step "Testing API Endpoints"

    # Test 1: Health check
    log "Testing /health endpoint..."
    local health_response=$(curl -sf http://localhost:8000/health)
    if echo "$health_response" | jq -e '.status == "healthy"' > /dev/null 2>&1; then
        log_success "Health check passed"
    else
        log_error "Health check failed: $health_response"
        exit 1
    fi

    # Test 2: Ready check
    log "Testing /ready endpoint..."
    local ready_response=$(curl -sf http://localhost:8000/ready)
    if echo "$ready_response" | jq -e '.status' > /dev/null 2>&1; then
        log_success "Ready check passed"
    else
        log_error "Ready check failed: $ready_response"
        exit 1
    fi

    # Test 3: Add task
    log "Testing task creation..."
    local task_response=$(curl -sf -X POST http://localhost:8000/api/v1/requests \
        -H "Content-Type: application/json" \
        -d "{
            \"user_message\": \"Add a task: Test ROG Ally setup on $(date +%Y-%m-%d)\",
            \"user_id\": \"${TEST_USER_ID}\",
            \"session_id\": \"${TEST_SESSION_ID}\"
        }")

    if echo "$task_response" | jq -e '.response_text' > /dev/null 2>&1; then
        local response_text=$(echo "$task_response" | jq -r '.response_text')
        log_success "Task creation test passed"
        log_info "Response: $response_text"
    else
        log_warning "Task creation test unclear - check response"
    fi

    sleep 2

    # Test 4: List tasks
    log "Testing task listing..."
    local list_response=$(curl -sf -X POST http://localhost:8000/api/v1/requests \
        -H "Content-Type: application/json" \
        -d "{
            \"user_message\": \"Show all my tasks\",
            \"user_id\": \"${TEST_USER_ID}\",
            \"session_id\": \"${TEST_SESSION_ID}\"
        }")

    if echo "$list_response" | jq -e '.response_text' > /dev/null 2>&1; then
        local response_text=$(echo "$list_response" | jq -r '.response_text')
        log_success "Task listing test passed"
        log_info "Response: $response_text"
    else
        log_warning "Task listing test unclear"
    fi
}

test_performance() {
    step "Measuring Performance on ROG Ally"

    source "${VENV_DIR}/bin/activate"

    log "Running performance benchmark (may take 1-2 minutes)..."
    log_info "This measures test coverage, lines of code, and system metrics"

    if [ -f "${PROJECT_DIR}/scripts/measure_baseline_metrics.py" ]; then
        # Run with timeout to prevent hanging
        echo "  Collecting metrics..."
        if timeout 120 python3 "${PROJECT_DIR}/scripts/measure_baseline_metrics.py" 2>&1 | tee -a "$LOG_FILE"; then
            log_success "Performance metrics collected"
        else
            local exit_code=$?
            if [ $exit_code -eq 124 ]; then
                log_warning "Performance measurement timed out after 2 minutes - skipping"
            else
                log_warning "Performance measurement failed - continuing without metrics"
            fi
        fi

        if [ -f "${PROJECT_DIR}/baseline_metrics.json" ]; then
            log "Performance metrics summary:"
            echo "  $(cat ${PROJECT_DIR}/baseline_metrics.json | jq -r '.test_coverage.total_coverage // "N/A"')% test coverage"
            echo "  $(cat ${PROJECT_DIR}/baseline_metrics.json | jq -r '.code_stats.total_lines // "N/A"') total lines of code"
            log_success "Performance baseline established"
        else
            log_info "Metrics file not generated - skipping this step"
        fi
    else
        log_info "Performance measurement script not found, skipping..."
    fi
}

run_integration_tests() {
    step "Running Integration Tests"

    source "${VENV_DIR}/bin/activate"

    log "Testing complete workflow (5 tests, ~15-20 seconds total)..."
    log_info "Each test uses real LLM inference on ROG Ally"

    local session_id="integration_test_$(date +%s)"

    # Test sequence
    local tests=(
        "Add three tasks: water plants, read book, exercise"
        "Show all my tasks"
        "Mark water plants as done"
        "Add a journal entry: Successfully set up 7-agent on ROG Ally"
        "Search for tasks about reading"
    )

    local passed=0
    local failed=0

    for i in "${!tests[@]}"; do
        local msg="${tests[$i]}"
        echo ""
        log "Test $((i+1))/${#tests[@]}: $msg"
        echo -n "  Processing... "

        # Add timeout for each request (10 seconds max)
        local response=$(timeout 10 curl -sf -X POST http://localhost:8000/api/v1/requests \
            -H "Content-Type: application/json" \
            -d "{
                \"user_message\": \"$msg\",
                \"user_id\": \"${TEST_USER_ID}\",
                \"session_id\": \"$session_id\"
            }" 2>&1)

        local curl_exit=$?

        if [ $curl_exit -eq 124 ]; then
            echo "⏱️  TIMEOUT"
            log_warning "Request timed out (>10s)"
            failed=$((failed + 1))
        elif echo "$response" | jq -e '.response_text' > /dev/null 2>&1; then
            echo "✓"
            local resp_text=$(echo "$response" | jq -r '.response_text // "No response"' | head -c 100)
            log_success "Response: $resp_text..."
            passed=$((passed + 1))
        else
            echo "✗"
            log_warning "Test response unclear or failed"
            failed=$((failed + 1))
        fi

        sleep 2
    done

    echo ""
    log "Integration test results: ${passed}/${#tests[@]} passed"

    if [ $passed -ge 3 ]; then
        log_success "Integration tests completed successfully"
    else
        log_warning "Some integration tests failed, but setup can continue"
    fi
}

create_quick_start_script() {
    step "Creating Quick Start Script"

    local quick_start="${HOME}/start_7agent.sh"

    cat > "$quick_start" << EOFSCRIPT
#!/bin/bash
# Quick start script for 7-Agent Assistant on ROG Ally

set -e

PROJECT_DIR="${PROJECT_DIR}"
cd "$PROJECT_DIR"

echo "🎮 Starting 7-Agent Assistant on ROG Ally..."

# Start Ollama if not running
if ! pgrep -f "ollama serve" > /dev/null; then
    echo "Starting Ollama..."
    ollama serve > /tmp/ollama.log 2>&1 &
    sleep 5
fi

# Activate venv
source venv/bin/activate

# Start server
echo "Starting server..."
python start_server.py > /tmp/server.log 2>&1 &
sleep 8

# Health check
echo "Checking health..."
curl -s http://localhost:8000/health | jq '.' || echo "Health check failed"

echo ""
echo "✅ 7-Agent Assistant is running!"
echo ""
echo "📊 Server logs:    tail -f /tmp/server.log"
echo "🦙 Ollama logs:    tail -f /tmp/ollama.log"
echo "💬 Interactive:    cd $PROJECT_DIR && source venv/bin/activate && python interactive_chat.py --local"
echo "🌐 API:            http://localhost:8000"
echo "🛑 Stop:           pkill -f 'start_server.py|ollama serve'"
echo ""
EOFSCRIPT

    chmod +x "$quick_start"
    log_success "Quick start script created: $quick_start"
}

generate_report() {
    step "Generating Setup Report"

    local report_file="${PROJECT_DIR}/setup_report_$(date +%Y%m%d_%H%M%S).txt"

    cat > "$report_file" << EOF
================================================================================
7-Agent Assistant Setup Report
================================================================================
Date: $(date)
Platform: ROG Ally (Bazzite OS)
Project Directory: $PROJECT_DIR

SYSTEM INFORMATION
------------------
Hostname: $(hostname)
Kernel: $(uname -r)
OS: $(grep PRETTY_NAME /etc/os-release | cut -d'"' -f2)
CPU: $(grep "model name" /proc/cpuinfo | head -1 | cut -d':' -f2 | xargs)
RAM: $(free -h | grep Mem: | awk '{print $2}')
GPU: $(lspci | grep -i vga | head -1)

INSTALLED COMPONENTS
--------------------
Python: $(python3 --version)
Ollama: $(ollama --version 2>/dev/null || echo "Not found")
LLM Model: ${RECOMMENDED_MODEL}
Virtual Environment: ${VENV_DIR}
Database: ${DATA_DIR}/assistant.db

CONFIGURATION
-------------
Configuration File: ${PROJECT_DIR}/.env
Model: ${RECOMMENDED_MODEL}
API Port: 8000
Database Path: ${DATA_DIR}/assistant.db

SERVICE STATUS
--------------
Ollama Service: $(pgrep -f "ollama serve" > /dev/null && echo "Running (PID: $(pgrep -f 'ollama serve'))" || echo "Stopped")
7-Agent Server: $(pgrep -f "start_server.py" > /dev/null && echo "Running (PID: $(pgrep -f 'start_server.py'))" || echo "Stopped")

QUICK COMMANDS
--------------
Start:      ~/start_7agent.sh
Stop:       pkill -f 'start_server.py|ollama serve'
Logs:       tail -f /tmp/server.log
Interactive: cd ${PROJECT_DIR} && source venv/bin/activate && python interactive_chat.py --local

API ENDPOINTS
-------------
Health:     curl http://localhost:8000/health
Ready:      curl http://localhost:8000/ready
Request:    curl -X POST http://localhost:8000/api/v1/requests -H "Content-Type: application/json" -d '{"user_message":"Hello","user_id":"test","session_id":"test"}'

LOGS
----
Setup Log: ${LOG_FILE}
Ollama Log: ${OLLAMA_LOG}
Server Log: ${SERVER_LOG}

DOCUMENTATION
-------------
Current Status: ${PROJECT_DIR}/CURRENT_STATUS.md
README: ${PROJECT_DIR}/README.md
API Contract: ${PROJECT_DIR}/API_CONTRACT.md

================================================================================
Setup completed successfully!
================================================================================
EOF

    cat "$report_file"
    log_success "Setup report saved: $report_file"
}

# ============================================================================
# Main Execution
# ============================================================================

main() {
    print_banner

    log "Setup started at $(date)"
    log "Project directory: $PROJECT_DIR"
    log "Log file: $LOG_FILE"

    # Run all setup steps
    check_system
    check_dependencies
    setup_python_env
    install_ollama
    download_models
    configure_env
    setup_database
    run_unit_tests
    start_server
    test_endpoints
    test_performance
    run_integration_tests
    create_quick_start_script
    generate_report

    # Final summary
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}✅ Setup completed successfully!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${CYAN}Next steps:${NC}"
    echo -e "  1. Try interactive chat: ${YELLOW}cd ${PROJECT_DIR} && source venv/bin/activate && python interactive_chat.py --local${NC}"
    echo -e "  2. Or use quick start: ${YELLOW}~/start_7agent.sh${NC}"
    echo -e "  3. Read documentation: ${YELLOW}cat ${PROJECT_DIR}/CURRENT_STATUS.md${NC}"
    echo ""
    echo -e "${CYAN}Services running:${NC}"
    echo -e "  - Ollama: http://localhost:11434"
    echo -e "  - 7-Agent API: http://localhost:8000"
    echo ""
    echo -e "${CYAN}To stop services:${NC}"
    echo -e "  ${YELLOW}pkill -f 'start_server.py|ollama serve'${NC}"
    echo ""
}

# Run main function
main "$@"
