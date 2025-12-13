#!/bin/bash
# =============================================================================
# Deployment Validation Script
# =============================================================================
# Purpose: Pre-flight checks before starting the 7-agent assistant
# Usage: ./scripts/validate_deployment.sh [OPTIONS]
#
# Options:
#   --skip-ollama     Skip Ollama connectivity checks
#   --verbose         Show detailed output
#   --help            Show this help message
#
# Exit Codes:
#   0  - All checks passed
#   1  - One or more checks failed (blockers)
#   2  - Warnings present but not blocking
# =============================================================================

set -u  # Exit on undefined variable

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
SKIP_OLLAMA=false
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-ollama) SKIP_OLLAMA=true; shift ;;
    --verbose) VERBOSE=true; shift ;;
    --help)
      grep "^#" "$0" | grep -v "^#!/" | sed 's/^# //' | sed 's/^#//'
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}"
      exit 1
      ;;
  esac
done

# Counters
PASSED=0
FAILED=0
WARNINGS=0

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
ASSISTANT_DIR="$REPO_ROOT/assistant_7agent"

# Helper functions
check_start() {
  echo -ne "${BLUE}[CHECK]${NC} $1... "
}

check_pass() {
  echo -e "${GREEN}✓${NC}"
  ((PASSED++))
}

check_fail() {
  echo -e "${RED}✗${NC}"
  if [ -n "${1:-}" ]; then
    echo -e "        ${RED}└─ $1${NC}"
  fi
  ((FAILED++))
}

check_warn() {
  echo -e "${YELLOW}⚠${NC}"
  if [ -n "${1:-}" ]; then
    echo -e "        ${YELLOW}└─ $1${NC}"
  fi
  ((WARNINGS++))
}

verbose_log() {
  if [ "$VERBOSE" = true ]; then
    echo "        └─ $1"
  fi
}

# =============================================================================
# Validation Checks
# =============================================================================

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Deployment Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# Check 1: Repository structure
check_start "Repository structure"
if [ -d "$ASSISTANT_DIR" ] && [ -f "$ASSISTANT_DIR/requirements.txt" ]; then
  check_pass
  verbose_log "assistant_7agent/ directory found"
else
  check_fail "assistant_7agent/ directory not found"
fi

# Check 2: Python version
check_start "Python 3.11+ available"
if command -v python3.11 &> /dev/null; then
  PYTHON_VERSION=$(python3.11 --version | awk '{print $2}')
  check_pass
  verbose_log "Python $PYTHON_VERSION"
elif command -v python3 &> /dev/null; then
  PY_VER=$(python3 -c 'import sys; print(".".join(map(str, sys.version_info[:2])))')
  if [ "$(echo "$PY_VER >= 3.11" | bc -l)" -eq 1 ]; then
    check_pass
    verbose_log "Python $PY_VER (via python3)"
  else
    check_fail "Python $PY_VER found, need 3.11+"
  fi
else
  check_fail "Python 3.11+ not found"
fi

# Check 3: Virtual environment
check_start "Virtual environment"
if [ -d "$ASSISTANT_DIR/venv" ]; then
  check_pass
  verbose_log "$ASSISTANT_DIR/venv exists"
else
  check_warn "Virtual environment not found (run bootstrap.sh)"
fi

# Check 4: Database
check_start "Database file"
if [ -f "$ASSISTANT_DIR/data/memory.db" ]; then
  check_pass
  DB_SIZE=$(du -h "$ASSISTANT_DIR/data/memory.db" | cut -f1)
  verbose_log "Database: $DB_SIZE"
else
  check_fail "Database not initialized (run init_database.sh)"
fi

# Check 5: Database schema
if [ -f "$ASSISTANT_DIR/data/memory.db" ]; then
  check_start "Database schema"
  if command -v sqlite3 &> /dev/null; then
    TABLE_COUNT=$(sqlite3 "$ASSISTANT_DIR/data/memory.db" "SELECT COUNT(*) FROM sqlite_master WHERE type='table';")
    if [ "$TABLE_COUNT" -ge 14 ]; then
      check_pass
      verbose_log "$TABLE_COUNT tables found"
    else
      check_fail "Only $TABLE_COUNT tables (expected 14+)"
    fi
  else
    check_warn "sqlite3 not available to verify"
  fi
fi

# Check 6: Configuration file
check_start "Configuration file (.env)"
if [ -f "$REPO_ROOT/.env" ]; then
  check_pass
  verbose_log ".env file exists"
elif [ -f "$ASSISTANT_DIR/.env" ]; then
  check_pass
  verbose_log "assistant_7agent/.env exists"
else
  check_warn ".env file not found (will use defaults)"
fi

# Check 7: Python dependencies (if venv exists)
if [ -d "$ASSISTANT_DIR/venv" ]; then
  check_start "Python dependencies"

  # Activate venv and check
  cd "$ASSISTANT_DIR"
  source venv/bin/activate 2>/dev/null

  if python -c "import fastapi, uvicorn, ollama, asyncpg" 2>/dev/null; then
    check_pass
    verbose_log "Core dependencies installed"
  else
    check_fail "Missing dependencies (run: pip install -r requirements.txt)"
  fi

  deactivate 2>/dev/null || true
fi

# Check 8: Import validation
if [ -d "$ASSISTANT_DIR/venv" ]; then
  check_start "Module imports"

  cd "$ASSISTANT_DIR"
  source venv/bin/activate 2>/dev/null

  IMPORT_CHECK=$(python -c "
try:
    from orchestrator.orchestrator_compat import Orchestrator
    from agents.planner import PlannerAgent
    from api.server import app
    from database.client import DatabaseClient
    print('OK')
except Exception as e:
    print(f'ERROR: {e}')
" 2>&1)

  if echo "$IMPORT_CHECK" | grep -q "OK"; then
    check_pass
    verbose_log "All modules importable"
  else
    check_fail "Import errors detected"
    if [ "$VERBOSE" = true ]; then
      echo "$IMPORT_CHECK" | sed 's/^/        /'
    fi
  fi

  deactivate 2>/dev/null || true
fi

# Check 9: Ollama connectivity (if not skipped)
if [ "$SKIP_OLLAMA" = false ]; then
  check_start "Ollama service"

  if command -v ollama &> /dev/null; then
    # Check if Ollama is running
    if curl -s http://localhost:11434/api/tags &> /dev/null; then
      check_pass
      verbose_log "Ollama running on port 11434"
    else
      check_warn "Ollama installed but not running"
    fi
  else
    check_warn "Ollama not installed (GPU features disabled)"
  fi

  # Check for models
  if command -v ollama &> /dev/null && curl -s http://localhost:11434/api/tags &> /dev/null; then
    check_start "Ollama models"

    MODELS=$(curl -s http://localhost:11434/api/tags | grep -o '"name":"[^"]*"' | wc -l)
    if [ "$MODELS" -gt 0 ]; then
      check_pass
      verbose_log "$MODELS models available"
    else
      check_warn "No models installed (run: ollama pull llama3.2:3b)"
    fi
  fi
fi

# Check 10: Port availability
check_start "Port 8000 (API) available"
if lsof -Pi :8000 -sTCP:LISTEN -t >/dev/null 2>&1; then
  check_warn "Port 8000 already in use"
else
  check_pass
  verbose_log "Port 8000 is free"
fi

# Check 11: Container runtime (optional)
check_start "Container runtime"
if command -v podman &> /dev/null; then
  PODMAN_VER=$(podman --version | awk '{print $3}')
  check_pass
  verbose_log "Podman $PODMAN_VER"
elif command -v docker &> /dev/null; then
  DOCKER_VER=$(docker --version | awk '{print $3}' | tr -d ',')
  check_pass
  verbose_log "Docker $DOCKER_VER"
else
  check_warn "No container runtime found (optional)"
fi

# Check 12: Disk space
check_start "Disk space"
AVAILABLE_GB=$(df -BG "$REPO_ROOT" | tail -1 | awk '{print $4}' | sed 's/G//')
if [ "$AVAILABLE_GB" -ge 5 ]; then
  check_pass
  verbose_log "${AVAILABLE_GB}GB available"
elif [ "$AVAILABLE_GB" -ge 2 ]; then
  check_warn "Low disk space: ${AVAILABLE_GB}GB"
else
  check_fail "Insufficient disk space: ${AVAILABLE_GB}GB (need 5GB+)"
fi

# =============================================================================
# Summary
# =============================================================================

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Validation Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo -e "${GREEN}Passed:${NC}   $PASSED"
echo -e "${YELLOW}Warnings:${NC} $WARNINGS"
echo -e "${RED}Failed:${NC}   $FAILED"
echo ""

if [ $FAILED -gt 0 ]; then
  echo -e "${RED}❌ Validation failed!${NC}"
  echo "Fix the failed checks before starting the system."
  echo ""
  exit 1
elif [ $WARNINGS -gt 0 ]; then
  echo -e "${YELLOW}⚠️  Validation passed with warnings${NC}"
  echo "Review the warnings above. System may have limited functionality."
  echo ""
  exit 2
else
  echo -e "${GREEN}✅ All checks passed!${NC}"
  echo "System is ready to start."
  echo ""
  echo "To start the API server:"
  echo "  cd $ASSISTANT_DIR"
  echo "  source venv/bin/activate"
  echo "  python -m uvicorn api.server:app --host 0.0.0.0 --port 8000"
  echo ""
  exit 0
fi
