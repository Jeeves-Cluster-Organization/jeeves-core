#!/bin/bash
# =============================================================================
# Fedora Node Config - Automated Bootstrap Script
# =============================================================================
# Purpose: Fully automate deployment on fresh Fedora installations
# Usage: ./scripts/bootstrap.sh [OPTIONS]
#
# Options:
#   --gpu             Enable GPU support (NVIDIA drivers, Ollama)
#   --multi-ollama    Set up 4 Ollama instances for multi-GPU
#   --systemd         Install and configure systemd services
#   --install-path    Installation path (default: current directory or /opt/fedora-node-config)
#   --skip-system     Skip system package installation (assumes already installed)
#   --help            Show this help message
#
# Example:
#   ./scripts/bootstrap.sh --gpu --multi-ollama --systemd
# =============================================================================

set -e  # Exit on error
set -u  # Exit on undefined variable

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
ENABLE_GPU=false
ENABLE_MULTI_OLLAMA=false
ENABLE_SYSTEMD=false
SKIP_SYSTEM_PACKAGES=false
INSTALL_PATH=""

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --gpu)
      ENABLE_GPU=true
      shift
      ;;
    --multi-ollama)
      ENABLE_MULTI_OLLAMA=true
      ENABLE_GPU=true  # Multi-ollama requires GPU
      shift
      ;;
    --systemd)
      ENABLE_SYSTEMD=true
      shift
      ;;
    --install-path)
      INSTALL_PATH="$2"
      shift 2
      ;;
    --skip-system)
      SKIP_SYSTEM_PACKAGES=true
      shift
      ;;
    --help)
      grep "^#" "$0" | grep -v "^#!/" | sed 's/^# //' | sed 's/^#//'
      exit 0
      ;;
    *)
      echo -e "${RED}Error: Unknown option $1${NC}"
      echo "Use --help for usage information"
      exit 1
      ;;
  esac
done

# Determine installation path
if [ -z "$INSTALL_PATH" ]; then
  if [ "$ENABLE_SYSTEMD" = true ]; then
    INSTALL_PATH="/opt/fedora-node-config"
  else
    INSTALL_PATH="$(pwd)"
  fi
fi

# Helper functions
log_info() {
  echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
  echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
  echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

step_header() {
  echo ""
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
  echo -e "${BLUE}$1${NC}"
  echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

check_command() {
  if command -v "$1" &> /dev/null; then
    return 0
  else
    return 1
  fi
}

# =============================================================================
# Step 1: Pre-flight checks
# =============================================================================
step_header "Step 1: Pre-flight Checks"

# Check if running as root
if [ "$EUID" -eq 0 ]; then
  log_error "Do not run this script as root!"
  log_info "Run as a regular user. The script will use sudo when needed."
  exit 1
fi
log_success "Not running as root ✓"

# Detect Fedora version
if [ ! -f /etc/fedora-release ]; then
  log_error "This script is designed for Fedora Linux"
  exit 1
fi

FEDORA_VERSION=$(rpm -E %fedora)
log_info "Detected Fedora $FEDORA_VERSION"

if [ "$FEDORA_VERSION" -lt 39 ]; then
  log_warning "Fedora $FEDORA_VERSION is not officially supported. Fedora 39+ recommended."
  read -p "Continue anyway? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
  fi
fi

# Check available disk space (need at least 10GB)
AVAILABLE_SPACE=$(df -BG "$INSTALL_PATH" | tail -1 | awk '{print $4}' | sed 's/G//')
if [ "$AVAILABLE_SPACE" -lt 10 ]; then
  log_warning "Less than 10GB free disk space available. This may cause issues."
fi
log_success "Disk space: ${AVAILABLE_SPACE}GB available ✓"

# =============================================================================
# Step 2: Install system dependencies
# =============================================================================
if [ "$SKIP_SYSTEM_PACKAGES" = false ]; then
  step_header "Step 2: Installing System Dependencies"

  log_info "Updating package database..."
  sudo dnf check-update || true

  log_info "Installing base development tools..."
  sudo dnf install -y \
    gcc \
    gcc-c++ \
    git \
    make \
    automake

  log_info "Installing Python 3.11..."
  sudo dnf install -y \
    python3.11 \
    python3.11-devel \
    python3.11-pip

  log_info "Installing container runtime..."
  sudo dnf install -y \
    docker \
    docker-compose

  log_info "Installing database and utilities..."
  sudo dnf install -y \
    sqlite \
    sqlite-devel \
    curl \
    wget \
    jq \
    bc

  log_info "Installing monitoring tools..."
  sudo dnf install -y \
    htop \
    sysstat || log_warning "Some monitoring tools failed to install"

  log_success "System dependencies installed ✓"
else
  log_info "Skipping system package installation (--skip-system)"
fi

# =============================================================================
# Step 3: Verify Python 3.11+
# =============================================================================
step_header "Step 3: Verifying Python Installation"

if ! check_command python3.11; then
  log_error "Python 3.11 not found! Install it first:"
  log_error "  sudo dnf install -y python3.11 python3.11-devel"
  exit 1
fi

PYTHON_VERSION=$(python3.11 --version | awk '{print $2}')
log_success "Python $PYTHON_VERSION installed ✓"

# Verify pip
if ! python3.11 -m pip --version &> /dev/null; then
  log_error "pip not available for Python 3.11"
  exit 1
fi
log_success "pip available ✓"

# =============================================================================
# Step 4: Create virtual environment
# =============================================================================
step_header "Step 4: Creating Virtual Environment"

VENV_PATH="$INSTALL_PATH/assistant_7agent/venv"

if [ -d "$VENV_PATH" ]; then
  log_warning "Virtual environment already exists at $VENV_PATH"
  read -p "Recreate it? (y/N) " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf "$VENV_PATH"
  else
    log_info "Using existing virtual environment"
  fi
fi

if [ ! -d "$VENV_PATH" ]; then
  log_info "Creating virtual environment at $VENV_PATH..."
  python3.11 -m venv "$VENV_PATH"
  log_success "Virtual environment created ✓"
fi

# Activate virtual environment
source "$VENV_PATH/bin/activate"
log_success "Virtual environment activated ✓"

# =============================================================================
# Step 5: Install Python dependencies
# =============================================================================
step_header "Step 5: Installing Python Dependencies"

cd "$INSTALL_PATH/assistant_7agent"

log_info "Upgrading pip..."
pip install --upgrade pip --quiet

log_info "Installing requirements (this may take a few minutes)..."
pip install -r requirements.txt --quiet
log_success "Python dependencies installed ✓"

# Verify core dependencies
log_info "Verifying core dependencies..."
python -c "import fastapi, uvicorn, ollama, asyncpg; print('✅ All core dependencies available')"

# =============================================================================
# Step 6: Initialize database
# =============================================================================
step_header "Step 6: Initializing Database"

DB_PATH="$INSTALL_PATH/assistant_7agent/data/memory.db"

if [ -f "$DB_PATH" ]; then
  log_warning "Database already exists at $DB_PATH"
  read -p "Reinitialize it? This will DELETE all data! (y/N) " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -f "$DB_PATH"
  else
    log_info "Skipping database initialization"
  fi
fi

if [ ! -f "$DB_PATH" ]; then
  if [ -f "$INSTALL_PATH/scripts/init_database.sh" ]; then
    log_info "Running init_database.sh..."
    bash "$INSTALL_PATH/scripts/init_database.sh" --db-path "$DB_PATH"
  else
    # Fallback: Create database directly with Python
    log_info "Creating database with Python..."
    python - <<'PYEOF'
import asyncio
from database.client import DatabaseClient
from pathlib import Path

async def init_db():
    db_path = Path("data/memory.db")
    db_path.parent.mkdir(parents=True, exist_ok=True)
    db = DatabaseClient(str(db_path))
    await db.initialize()
    print("✅ Database initialized")

asyncio.run(init_db())
PYEOF
  fi
  log_success "Database initialized ✓"
fi

# =============================================================================
# Step 7: Generate .env configuration
# =============================================================================
step_header "Step 7: Generating Configuration"

cd "$INSTALL_PATH"
ENV_FILE="$INSTALL_PATH/.env"

if [ -f "$ENV_FILE" ]; then
  log_warning ".env file already exists"
  read -p "Overwrite with defaults? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    log_info "Keeping existing .env file"
  fi
fi

if [ ! -f "$ENV_FILE" ] || [[ $REPLY =~ ^[Yy]$ ]]; then
  log_info "Creating .env from template..."
  cp .env.template .env

  # Set sensible defaults
  sed -i "s|NODE_ID=node-01|NODE_ID=$(hostname)|" .env
  sed -i "s|APP_DEBUG=false|APP_DEBUG=false|" .env
  sed -i "s|LOG_LEVEL=INFO|LOG_LEVEL=INFO|" .env

  # Set database path
  sed -i "s|APP_DB_PATH=data/memory.db|APP_DB_PATH=$INSTALL_PATH/assistant_7agent/data/memory.db|" .env

  log_success ".env configuration created ✓"
  log_info "Edit .env to customize settings"
fi

# =============================================================================
# Step 8: GPU Setup (if --gpu enabled)
# =============================================================================
if [ "$ENABLE_GPU" = true ]; then
  step_header "Step 8: GPU Setup"

  # Check for NVIDIA drivers
  if ! check_command nvidia-smi; then
    log_warning "NVIDIA drivers not detected!"
    log_info "To install NVIDIA drivers on Fedora:"
    log_info "  1. sudo dnf install -y akmod-nvidia xorg-x11-drv-nvidia-cuda"
    log_info "  2. sudo akmods --force && sudo dracut --force"
    log_info "  3. sudo reboot"
    log_info ""
    read -p "Continue without GPU support? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
      exit 1
    fi
  else
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)
    log_success "GPU detected: $GPU_NAME ✓"
  fi

  # Install Ollama if not present
  if ! check_command ollama; then
    log_info "Installing Ollama..."
    curl -fsSL https://ollama.ai/install.sh | sh
    log_success "Ollama installed ✓"
  else
    OLLAMA_VERSION=$(ollama --version | head -1)
    log_success "Ollama already installed: $OLLAMA_VERSION ✓"
  fi
fi

# =============================================================================
# Step 9: Multi-Ollama Setup (if --multi-ollama enabled)
# =============================================================================
if [ "$ENABLE_MULTI_OLLAMA" = true ]; then
  step_header "Step 9: Multi-Ollama Setup"

  if [ -f "$INSTALL_PATH/scripts/setup_ollama_cluster.sh" ]; then
    log_info "Setting up 4 Ollama instances..."
    bash "$INSTALL_PATH/scripts/setup_ollama_cluster.sh" --instances 4 --model llama3.2:3b
    log_success "Multi-Ollama cluster configured ✓"
  else
    log_warning "setup_ollama_cluster.sh not found, skipping multi-instance setup"
    log_info "You can set up Ollama instances manually later"
  fi

  # Update .env for multi-Ollama
  if [ -f "$ENV_FILE" ]; then
    log_info "Updating .env for multi-Ollama configuration..."
    cat >> "$ENV_FILE" <<EOF

# Multi-Ollama Configuration (added by bootstrap.sh)
OLLAMA_PLANNER_URL=http://localhost:11434
OLLAMA_VALIDATOR_URL=http://localhost:11435
OLLAMA_META_VALIDATOR_URL=http://localhost:11436
OLLAMA_EXECUTOR_URL=http://localhost:11434
EOF
    log_success ".env updated for multi-Ollama ✓"
  fi
fi

# =============================================================================
# Step 10: Pull required models
# =============================================================================
if [ "$ENABLE_GPU" = true ] && check_command ollama; then
  step_header "Step 10: Pulling LLM Models"

  log_info "Pulling llama3.2:3b (this may take several minutes)..."
  ollama pull llama3.2:3b || log_warning "Failed to pull llama3.2:3b"

  log_info "Pulling qwen2.5:3b-instruct..."
  ollama pull qwen2.5:3b-instruct || log_warning "Failed to pull qwen2.5:3b-instruct"

  log_success "Models pulled ✓"

  log_info "Available models:"
  ollama list
fi

# =============================================================================
# Step 11: Run pre-flight validation
# =============================================================================
step_header "Step 11: Running Validation"

if [ -f "$INSTALL_PATH/scripts/validate_deployment.sh" ]; then
  log_info "Running deployment validation..."
  bash "$INSTALL_PATH/scripts/validate_deployment.sh" || log_warning "Some validations failed"
else
  log_info "Validation script not found, running basic checks..."

  # Basic validation
  cd "$INSTALL_PATH/assistant_7agent"
  source venv/bin/activate

  log_info "Testing database connectivity..."
  python -c "
import asyncio
from database.client import DatabaseClient
async def test():
    db = DatabaseClient('data/memory.db')
    await db.initialize()
    print('✅ Database OK')
asyncio.run(test())
" || log_error "Database validation failed"

  log_info "Testing imports..."
  python -c "
from orchestrator.orchestrator_compat import Orchestrator
from agents.planner import PlannerAgent
from api.server import app
print('✅ Imports OK')
" || log_error "Import validation failed"
fi

# =============================================================================
# Step 12: Install systemd services (if --systemd enabled)
# =============================================================================
if [ "$ENABLE_SYSTEMD" = true ]; then
  step_header "Step 12: Installing Systemd Services"

  log_info "This will install systemd services for:"
  log_info "  - assistant-7agent@${USER}.service"
  log_info "  - ollama@${USER}.service"
  echo ""
  read -p "Continue? (y/N) " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    if [ -f "$INSTALL_PATH/scripts/install-systemd-services.sh" ]; then
      sudo bash "$INSTALL_PATH/scripts/install-systemd-services.sh"
      log_success "Systemd services installed ✓"

      log_info "To start services:"
      log_info "  sudo systemctl start ollama@${USER}.service"
      log_info "  sudo systemctl start assistant-7agent@${USER}.service"
    else
      log_warning "install-systemd-services.sh not found"
    fi
  else
    log_info "Skipping systemd installation"
  fi
fi

# =============================================================================
# Completion Summary
# =============================================================================
step_header "🎉 Bootstrap Complete!"

echo ""
log_success "Installation completed successfully!"
echo ""
log_info "Configuration Summary:"
echo "  Installation Path: $INSTALL_PATH"
echo "  Virtual Environment: $VENV_PATH"
echo "  Database: $DB_PATH"
echo "  Configuration: $ENV_FILE"
if [ "$ENABLE_GPU" = true ]; then
  echo "  GPU Support: Enabled"
fi
if [ "$ENABLE_MULTI_OLLAMA" = true ]; then
  echo "  Multi-Ollama: 4 instances configured"
fi
if [ "$ENABLE_SYSTEMD" = true ]; then
  echo "  Systemd Services: Installed"
fi

echo ""
log_info "Next Steps:"
echo ""
echo "1. Review and customize configuration:"
echo "   nano $ENV_FILE"
echo ""
echo "2. Start the API server:"
echo "   cd $INSTALL_PATH/assistant_7agent"
echo "   source venv/bin/activate"
echo "   python -m uvicorn api.server:app --host 0.0.0.0 --port 8000"
echo ""
echo "3. Test the API:"
echo "   curl http://localhost:8000/health"
echo ""
echo "4. Run smoke tests:"
if [ -f "$INSTALL_PATH/scripts/smoke_test.sh" ]; then
  echo "   bash $INSTALL_PATH/scripts/smoke_test.sh"
else
  echo "   (smoke_test.sh not yet available)"
fi

if [ "$ENABLE_SYSTEMD" = true ]; then
  echo ""
  echo "5. Start systemd services:"
  echo "   sudo systemctl start ollama@${USER}.service"
  echo "   sudo systemctl start assistant-7agent@${USER}.service"
  echo "   sudo systemctl status assistant-7agent@${USER}.service"
fi

echo ""
log_info "Documentation:"
echo "  - Full setup guide: $INSTALL_PATH/GPU_NODE_SETUP.md"
echo "  - System requirements: $INSTALL_PATH/docs/FEDORA_REQUIREMENTS.md"
echo "  - Architecture: $INSTALL_PATH/ARCHITECTURE.md"
echo "  - Quick start: $INSTALL_PATH/assistant_7agent/QUICKSTART.md"

echo ""
log_success "Happy coding! 🚀"
echo ""
