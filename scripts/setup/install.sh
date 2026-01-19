#!/bin/bash
# =============================================================================
# Jeeves FF - Universal Setup Script
# =============================================================================
# Production-ready setup: PostgreSQL + pgvector + Ollama
# Works on: Ubuntu, Debian, Fedora, RHEL, WSL
#
# Usage:
#   ./setup.sh              # Full production setup
#   ./setup.sh --skip-ollama # Skip Ollama installation (use OpenAI/Anthropic)
# =============================================================================

set -e  # Exit on error
set -u  # Exit on undefined variable

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Options
SKIP_OLLAMA=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --skip-ollama)
      SKIP_OLLAMA=true
      shift
      ;;
    --help)
      echo "Jeeves FF Setup"
      echo ""
      echo "Usage: ./setup.sh [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --skip-ollama    Skip Ollama installation (use cloud LLM providers)"
      echo "  --help           Show this help"
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}"
      echo "Use --help for usage"
      exit 1
      ;;
  esac
done

# Helper functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[✓]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }

header() {
  echo ""
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BLUE}$1${NC}"
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

# =============================================================================
# Detect OS
# =============================================================================
header "Detecting Operating System"

if [ -f /etc/os-release ]; then
  . /etc/os-release
  OS_ID=$ID
  OS_VERSION=$VERSION_ID
else
  log_error "Cannot detect OS. /etc/os-release not found."
  exit 1
fi

# Check if WSL
if grep -qi microsoft /proc/version 2>/dev/null; then
  IS_WSL=true
  log_info "Running on WSL (Windows Subsystem for Linux)"
else
  IS_WSL=false
fi

log_success "Detected: $PRETTY_NAME"
if [ "$IS_WSL" = true ]; then
  log_info "WSL mode enabled"
fi

# =============================================================================
# Check prerequisites
# =============================================================================
header "Checking Prerequisites"

# Check if running as root
if [ "$EUID" -eq 0 ]; then
  log_error "Do not run as root! Run as regular user (script will use sudo when needed)"
  exit 1
fi
log_success "Running as regular user"

# Check disk space (need at least 10GB)
AVAILABLE_GB=$(df -BG . | tail -1 | awk '{print $4}' | sed 's/G//')
if [ "$AVAILABLE_GB" -lt 10 ]; then
  log_warning "Less than 10GB free disk space (${AVAILABLE_GB}GB available)"
  read -p "Continue anyway? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
  fi
else
  log_success "Disk space: ${AVAILABLE_GB}GB available"
fi

# =============================================================================
# Install system dependencies
# =============================================================================
header "Installing System Dependencies"

case "$OS_ID" in
  ubuntu|debian)
    log_info "Updating package list..."
    sudo apt-get update -qq

    log_info "Installing build tools and Python..."
    sudo apt-get install -y -qq \
      python3 \
      python3-pip \
      python3-venv \
      python3-dev \
      build-essential \
      curl \
      wget \
      git \
      postgresql-client \
      jq

    # Install Docker for containers
    if ! command -v docker &> /dev/null; then
      log_info "Installing Docker..."
      sudo apt-get install -y -qq docker.io docker-compose
      sudo usermod -aG docker "$USER"
      log_warning "You may need to log out and back in for Docker group permissions"
    fi
    ;;

  fedora|rhel|centos)
    log_info "Installing build tools and Python..."
    sudo dnf install -y \
      python3.11 \
      python3-devel \
      gcc \
      gcc-c++ \
      git \
      curl \
      wget \
      postgresql \
      jq \
      docker \
      docker-compose
    sudo systemctl enable --now docker
    sudo usermod -aG docker "$USER"
    log_warning "You may need to log out and back in for Docker group permissions"
    ;;

  *)
    log_error "Unsupported OS: $OS_ID"
    log_error "This script supports: Ubuntu, Debian, Fedora, RHEL"
    exit 1
    ;;
esac

log_success "System dependencies installed"

# =============================================================================
# Python setup
# =============================================================================
header "Setting Up Python Environment"

# Determine Python command
if command -v python3.11 &> /dev/null; then
  PYTHON_CMD=python3.11
elif command -v python3 &> /dev/null; then
  PYTHON_VERSION=$(python3 --version | awk '{print $2}')
  PYTHON_MAJOR=$(echo $PYTHON_VERSION | cut -d. -f1)
  PYTHON_MINOR=$(echo $PYTHON_VERSION | cut -d. -f2)

  if [ "$PYTHON_MAJOR" -eq 3 ] && [ "$PYTHON_MINOR" -ge 10 ]; then
    PYTHON_CMD=python3
  else
    log_error "Python 3.10+ required. Found: $PYTHON_VERSION"
    exit 1
  fi
else
  log_error "Python 3 not found"
  exit 1
fi

log_success "Using $(${PYTHON_CMD} --version)"

# Create virtual environment
if [ -d "venv" ]; then
  log_warning "Virtual environment already exists"
  read -p "Recreate it? (y/N) " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf venv
    ${PYTHON_CMD} -m venv venv
    log_success "Virtual environment recreated"
  else
    log_info "Using existing virtual environment"
  fi
else
  log_info "Creating virtual environment..."
  ${PYTHON_CMD} -m venv venv
  log_success "Virtual environment created"
fi

# Activate venv
source venv/bin/activate

# Upgrade pip
log_info "Upgrading pip..."
pip install --upgrade pip --quiet

# Install dependencies
log_info "Installing Python dependencies (this may take a few minutes)..."
pip install -r requirements.txt --quiet
log_success "Python dependencies installed"

# Verify core imports
log_info "Verifying core dependencies..."
python -c "import fastapi, uvicorn, asyncpg, pgvector; print('✅ Core dependencies OK')"

# =============================================================================
# PostgreSQL setup
# =============================================================================
header "Setting Up PostgreSQL + pgvector"

# Load existing .env file to get password (if exists)
if [ -f ".env" ]; then
  # Export variables from .env (only POSTGRES_* variables for safety)
  set -a
  source <(grep ^POSTGRES_ .env 2>/dev/null || true)
  set +a
fi

# Default password if not set
POSTGRES_PASSWORD=${POSTGRES_PASSWORD:-dev_password_change_in_production}
POSTGRES_USER=${POSTGRES_USER:-assistant}
POSTGRES_DATABASE=${POSTGRES_DATABASE:-assistant}

# Start PostgreSQL container
CONTAINER_ALREADY_RUNNING=false
if docker ps --format "{{.Names}}" | grep -q "assistant-postgres"; then
  log_info "PostgreSQL container already running"
  CONTAINER_ALREADY_RUNNING=true
else
  log_info "Starting PostgreSQL container with pgvector..."
  docker compose -f docker-compose.yml up -d postgres

  # Wait for PostgreSQL to be ready
  log_info "Waiting for PostgreSQL to be ready..."
  sleep 5

  for i in {1..30}; do
    if docker exec assistant-postgres pg_isready -U assistant &> /dev/null; then
      log_success "PostgreSQL is ready"
      break
    fi
    if [ $i -eq 30 ]; then
      log_error "PostgreSQL failed to start"
      exit 1
    fi
    sleep 1
  done
fi

# Verify PostgreSQL password matches .env - fix if needed
if [ "$CONTAINER_ALREADY_RUNNING" = true ]; then
  log_info "Verifying PostgreSQL credentials..."

  # Test if current .env password works
  if PGPASSWORD="${POSTGRES_PASSWORD}" psql -h localhost -p 5432 -U "${POSTGRES_USER}" -d "${POSTGRES_DATABASE}" -c "SELECT 1" &> /dev/null; then
    log_success "PostgreSQL credentials verified"
  else
    log_warning "PostgreSQL password mismatch detected"
    log_info "Updating PostgreSQL password to match .env file..."

    # Reset password using the container's postgres superuser
    # First, get current password from container environment
    CONTAINER_PASSWORD=$(docker exec assistant-postgres printenv POSTGRES_PASSWORD 2>/dev/null || echo "")

    if [ -n "$CONTAINER_PASSWORD" ]; then
      # Use container's password to connect and update
      if PGPASSWORD="${CONTAINER_PASSWORD}" psql -h localhost -p 5432 -U "${POSTGRES_USER}" -d "${POSTGRES_DATABASE}" -c "ALTER USER ${POSTGRES_USER} WITH PASSWORD '${POSTGRES_PASSWORD}';" &> /dev/null; then
        log_success "PostgreSQL password updated to match .env"
      else
        # Try using postgres superuser inside container
        log_info "Attempting password reset via container..."
        if docker exec assistant-postgres psql -U "${POSTGRES_USER}" -d "${POSTGRES_DATABASE}" -c "ALTER USER ${POSTGRES_USER} WITH PASSWORD '${POSTGRES_PASSWORD}';" &> /dev/null; then
          log_success "PostgreSQL password updated to match .env"
        else
          log_error "Failed to update PostgreSQL password"
          log_info "Try recreating the container: docker compose down -v && docker compose up -d postgres"
          exit 1
        fi
      fi
    else
      # No container password found, try direct update via container
      log_info "Attempting password reset via container..."
      if docker exec assistant-postgres psql -U "${POSTGRES_USER}" -d "${POSTGRES_DATABASE}" -c "ALTER USER ${POSTGRES_USER} WITH PASSWORD '${POSTGRES_PASSWORD}';" &> /dev/null; then
        log_success "PostgreSQL password updated to match .env"
      else
        log_error "Failed to update PostgreSQL password"
        log_info "Try recreating the container: docker compose down -v && docker compose up -d postgres"
        exit 1
      fi
    fi
  fi
fi

# Create .env file
if [ -f ".env" ]; then
  log_warning ".env file already exists"
  read -p "Overwrite with production defaults? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    log_info "Keeping existing .env file"
  else
    CREATE_ENV=true
  fi
else
  CREATE_ENV=true
fi

if [ "${CREATE_ENV:-false}" = true ]; then
  log_info "Creating .env configuration..."

  # Generate secure password
  POSTGRES_PASSWORD=$(openssl rand -base64 24 | tr -d "=+/" | cut -c1-32)

  cat > .env << EOF
# 7-Agent Assistant - Production Configuration
# Auto-generated by setup.sh on $(date)

# Database: PostgreSQL + pgvector (Production)
DATABASE_BACKEND=postgres
VECTOR_BACKEND=pgvector

# PostgreSQL Configuration
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_DATABASE=assistant
POSTGRES_USER=assistant
POSTGRES_PASSWORD=${POSTGRES_PASSWORD}

# LLM Provider (change if using cloud providers)
LLM_PROVIDER=ollama
DEFAULT_MODEL=qwen2.5:3b-instruct
OLLAMA_HOST=http://localhost:11434

# Optional: Cloud LLM Providers (uncomment to use)
# LLM_PROVIDER=openai
# OPENAI_API_KEY=your-key-here
# LLM_PROVIDER=anthropic
# ANTHROPIC_API_KEY=your-key-here

# Application Settings
LOG_LEVEL=INFO
DEPLOYMENT_MODE=single_node

# Feature Flags
KANBAN_ENABLED=false
CHAT_ENABLED=true
EOF

  log_success ".env file created"
fi

# Initialize database with V2 migrations
log_info "Initializing PostgreSQL schema..."
python scripts/database/init.py --backend postgres --apply-v2
log_success "Database initialized with V2 memory infrastructure"

# =============================================================================
# Ollama setup (optional)
# =============================================================================
if [ "$SKIP_OLLAMA" = false ]; then
  header "Setting Up Ollama"

  if command -v ollama &> /dev/null; then
    log_info "Ollama already installed: $(ollama --version | head -1)"
  else
    log_info "Installing Ollama..."
    curl -fsSL https://ollama.com/install.sh | sh
    log_success "Ollama installed"
  fi

  # Start Ollama service
  if systemctl is-active --quiet ollama 2>/dev/null; then
    log_info "Ollama service already running"
  else
    log_info "Starting Ollama service..."
    if [ "$IS_WSL" = true ]; then
      # WSL doesn't have systemd by default
      nohup ollama serve > /dev/null 2>&1 &
      sleep 2
      log_success "Ollama started in background"
    else
      sudo systemctl start ollama || ollama serve &
      sleep 2
      log_success "Ollama service started"
    fi
  fi

  # Pull model
  log_info "Pulling qwen2.5:3b-instruct model (this may take a few minutes)..."
  ollama pull qwen2.5:3b-instruct
  log_success "Model downloaded"

  log_info "Available models:"
  ollama list
else
  log_warning "Skipping Ollama installation (--skip-ollama flag)"
  log_info "Configure cloud LLM provider in .env file"
fi

# =============================================================================
# Health check
# =============================================================================
header "Running Health Checks"

log_info "Testing database connection..."
python -c "
import asyncio
from database.postgres_client import PostgreSQLClient
from config.settings import settings

async def test():
    client = PostgreSQLClient(
        database_url=settings.postgres_url,
        pool_size=5
    )
    await client.connect()
    result = await client.fetch_one('SELECT 1 as num')
    assert result['num'] == 1
    await client.disconnect()
    print('✅ Database connection OK')

asyncio.run(test())
"

log_info "Testing core imports..."
python -c "
from orchestrator.orchestrator_compat import Orchestrator
from api.server import app
from tools.task_tools import TaskTools
print('✅ Core imports OK')
"

log_success "All health checks passed"

# =============================================================================
# Completion
# =============================================================================
header "🎉 Setup Complete!"

echo ""
log_success "Installation completed successfully!"
echo ""
echo "Configuration:"
echo "  - Database: PostgreSQL + pgvector (localhost:5432)"
if [ "$SKIP_OLLAMA" = false ]; then
  echo "  - LLM: Ollama (qwen2.5:3b-instruct)"
else
  echo "  - LLM: Configure cloud provider in .env"
fi
echo "  - Config: .env"
echo ""
echo "Next steps:"
echo ""
echo "1. Start the server:"
echo "   source venv/bin/activate"
echo "   python scripts/run/server.py"
echo ""
echo "2. Test the API:"
echo "   curl http://localhost:8000/health"
echo ""
echo "3. Try the interactive chat:"
echo "   source venv/bin/activate"
echo "   python scripts/run/chat.py"
echo ""
echo "4. Run tests:"
echo "   source venv/bin/activate"
echo "   pytest tests/unit/ -v"
echo ""
echo "Documentation: docs/setup/SETUP_GUIDE.md"
echo "Status: CURRENT_STATUS.md"
echo ""
log_success "Happy coding! 🚀"
echo ""
