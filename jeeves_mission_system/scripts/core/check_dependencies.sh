#!/bin/bash
# =============================================================================
# Dependency Verification Script
# =============================================================================
# Purpose: Check all prerequisites before deployment
# Usage: ./scripts/check_dependencies.sh [OPTIONS]
#
# Options:
#   --gpu             Include GPU-specific checks
#   --fix             Show commands to fix missing dependencies
#   --verbose         Show detailed version information
#   --help            Show this help message
#
# Exit Codes:
#   0 - All dependencies met
#   1 - One or more dependencies missing
# =============================================================================

set -u

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Defaults
CHECK_GPU=false
SHOW_FIX=false
VERBOSE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --gpu) CHECK_GPU=true; shift ;;
    --fix) SHOW_FIX=true; shift ;;
    --verbose) VERBOSE=true; shift ;;
    --help)
      grep "^#" "$0" | grep -v "^#!/" | sed 's/^# //' | sed 's/^#//'
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Counters
MET=0
MISSING=0
WARNINGS=0

# Fix commands
declare -a FIX_COMMANDS

# Helper functions
check_met() {
  echo -e "${GREEN}✅ $1${NC}"
  if [ "$VERBOSE" = true ] && [ -n "${2:-}" ]; then
    echo "   └─ $2"
  fi
  ((MET++))
}

check_missing() {
  echo -e "${RED}❌ $1${NC}"
  if [ -n "${2:-}" ]; then
    echo "   └─ $2"
  fi
  if [ -n "${3:-}" ]; then
    FIX_COMMANDS+=("$3")
  fi
  ((MISSING++))
}

check_warning() {
  echo -e "${YELLOW}⚠️  $1${NC}"
  if [ -n "${2:-}" ]; then
    echo "   └─ $2"
  fi
  ((WARNINGS++))
}

# =============================================================================
# System Information
# =============================================================================

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Dependency Verification"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

echo "System Information:"
echo "  OS: $(cat /etc/os-release | grep PRETTY_NAME | cut -d'"' -f2)"
echo "  Kernel: $(uname -r)"
echo "  Arch: $(uname -m)"
echo ""

# =============================================================================
# Check Fedora Version
# =============================================================================

echo "Fedora Version:"
if [ -f /etc/fedora-release ]; then
  FEDORA_VERSION=$(rpm -E %fedora)
  if [ "$FEDORA_VERSION" -ge 39 ]; then
    check_met "Fedora $FEDORA_VERSION" "Supported version"
  elif [ "$FEDORA_VERSION" -ge 38 ]; then
    check_warning "Fedora $FEDORA_VERSION" "Fedora 39+ recommended"
  else
    check_missing "Fedora $FEDORA_VERSION" "Unsupported version (need 39+)"
  fi
else
  check_missing "Not Fedora Linux" "This project is designed for Fedora"
fi
echo ""

# =============================================================================
# Build Tools
# =============================================================================

echo "Build Tools:"

# GCC
if command -v gcc &> /dev/null; then
  GCC_VER=$(gcc --version | head -1 | awk '{print $3}')
  check_met "gcc $GCC_VER"
else
  check_missing "gcc not found" "C compiler required for Python extensions" \
    "sudo dnf install -y gcc"
fi

# G++
if command -v g++ &> /dev/null; then
  GPP_VER=$(g++ --version | head -1 | awk '{print $3}')
  check_met "g++ $GPP_VER"
else
  check_missing "g++ not found" "C++ compiler required" \
    "sudo dnf install -y gcc-c++"
fi

# Git
if command -v git &> /dev/null; then
  GIT_VER=$(git --version | awk '{print $3}')
  check_met "git $GIT_VER"
else
  check_missing "git not found" "Required for repository management" \
    "sudo dnf install -y git"
fi

# Make
if command -v make &> /dev/null; then
  MAKE_VER=$(make --version | head -1 | awk '{print $3}')
  check_met "make $MAKE_VER"
else
  check_missing "make not found" "Build tool required" \
    "sudo dnf install -y make"
fi

echo ""

# =============================================================================
# Python
# =============================================================================

echo "Python Environment:"

# Python 3.11+
if command -v python3.11 &> /dev/null; then
  PY_VER=$(python3.11 --version | awk '{print $2}')
  check_met "python3.11 $PY_VER"

  # Check python3.11-devel
  if rpm -q python3.11-devel &> /dev/null; then
    check_met "python3.11-devel"
  else
    check_missing "python3.11-devel not found" "Required for compiling extensions" \
      "sudo dnf install -y python3.11-devel"
  fi

  # Check pip
  if python3.11 -m pip --version &> /dev/null; then
    PIP_VER=$(python3.11 -m pip --version | awk '{print $2}')
    check_met "pip $PIP_VER"
  else
    check_missing "pip not available" "Package manager required" \
      "sudo dnf install -y python3.11-pip"
  fi
else
  check_missing "python3.11 not found" "Python 3.11+ required" \
    "sudo dnf install -y python3.11 python3.11-devel python3.11-pip"
fi

echo ""

# =============================================================================
# Container Runtime
# =============================================================================

echo "Container Runtime:"

CONTAINER_FOUND=false

# Podman
if command -v podman &> /dev/null; then
  PODMAN_VER=$(podman --version | awk '{print $3}')
  check_met "podman $PODMAN_VER"
  CONTAINER_FOUND=true

  # Check podman-compose
  if command -v podman-compose &> /dev/null; then
    PCOMP_VER=$(podman-compose --version | awk '{print $3}' || echo "unknown")
    check_met "podman-compose $PCOMP_VER"
  else
    check_warning "podman-compose not found" "Useful for multi-container setups" \
      "sudo dnf install -y podman-compose"
  fi
else
  check_missing "podman not found" "Container runtime required" \
    "sudo dnf install -y podman podman-compose"
fi

# Docker (alternative)
if command -v docker &> /dev/null; then
  DOCKER_VER=$(docker --version | awk '{print $3}' | tr -d ',')
  if [ "$CONTAINER_FOUND" = true ]; then
    check_met "docker $DOCKER_VER (alternative to podman)"
  else
    check_met "docker $DOCKER_VER"
    CONTAINER_FOUND=true
  fi
fi

echo ""

# =============================================================================
# Database
# =============================================================================

echo "Database:"

# SQLite
if command -v sqlite3 &> /dev/null; then
  SQLITE_VER=$(sqlite3 --version | awk '{print $1}')
  check_met "sqlite3 $SQLITE_VER"
else
  check_missing "sqlite3 not found" "Database required" \
    "sudo dnf install -y sqlite"
fi

# SQLite devel
if rpm -q sqlite-devel &> /dev/null; then
  check_met "sqlite-devel"
else
  check_missing "sqlite-devel not found" "Development headers required" \
    "sudo dnf install -y sqlite-devel"
fi

echo ""

# =============================================================================
# Utilities
# =============================================================================

echo "Utilities:"

# curl
if command -v curl &> /dev/null; then
  CURL_VER=$(curl --version | head -1 | awk '{print $2}')
  check_met "curl $CURL_VER"
else
  check_missing "curl not found" "HTTP client required" \
    "sudo dnf install -y curl"
fi

# wget
if command -v wget &> /dev/null; then
  WGET_VER=$(wget --version | head -1 | awk '{print $3}')
  check_met "wget $WGET_VER"
else
  check_warning "wget not found" "Alternative HTTP client (optional)"
fi

# jq
if command -v jq &> /dev/null; then
  JQ_VER=$(jq --version | sed 's/jq-//')
  check_met "jq $JQ_VER"
else
  check_missing "jq not found" "JSON parser required for scripts" \
    "sudo dnf install -y jq"
fi

# bc
if command -v bc &> /dev/null; then
  check_met "bc (calculator)"
else
  check_missing "bc not found" "Calculator required for scripts" \
    "sudo dnf install -y bc"
fi

echo ""

# =============================================================================
# System Resources
# =============================================================================

echo "System Resources:"

# Disk space
AVAILABLE_GB=$(df -BG / | tail -1 | awk '{print $4}' | sed 's/G//')
if [ "$AVAILABLE_GB" -ge 10 ]; then
  check_met "Disk space: ${AVAILABLE_GB}GB available"
elif [ "$AVAILABLE_GB" -ge 5 ]; then
  check_warning "Disk space: ${AVAILABLE_GB}GB available" "10GB+ recommended"
else
  check_missing "Disk space: ${AVAILABLE_GB}GB available" "Need at least 10GB free"
fi

# Memory
TOTAL_MEM_GB=$(free -g | awk '/^Mem:/{print $2}')
if [ "$TOTAL_MEM_GB" -ge 8 ]; then
  check_met "Memory: ${TOTAL_MEM_GB}GB total"
elif [ "$TOTAL_MEM_GB" -ge 4 ]; then
  check_warning "Memory: ${TOTAL_MEM_GB}GB total" "8GB+ recommended"
else
  check_warning "Memory: ${TOTAL_MEM_GB}GB total" "May not be sufficient"
fi

echo ""

# =============================================================================
# Network Ports
# =============================================================================

echo "Network Ports:"

# Check if ports are available
for port in 8000 9090 11434; do
  if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
    check_warning "Port $port in use" "May conflict with deployment"
  else
    check_met "Port $port available"
  fi
done

echo ""

# =============================================================================
# GPU Checks (if --gpu specified)
# =============================================================================

if [ "$CHECK_GPU" = true ]; then
  echo "GPU Configuration:"

  # NVIDIA drivers
  if command -v nvidia-smi &> /dev/null; then
    DRIVER_VER=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader | head -1)
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader | head -1)
    check_met "NVIDIA driver $DRIVER_VER" "$GPU_NAME"

    # CUDA version
    CUDA_VER=$(nvidia-smi --query-gpu=cuda_version --format=csv,noheader | head -1)
    check_met "CUDA $CUDA_VER"
  else
    check_missing "nvidia-smi not found" "NVIDIA drivers not installed" \
      "sudo dnf install -y akmod-nvidia xorg-x11-drv-nvidia-cuda"
  fi

  # Ollama
  if command -v ollama &> /dev/null; then
    OLLAMA_VER=$(ollama --version | head -1 || echo "unknown")
    check_met "Ollama installed" "$OLLAMA_VER"

    # Check if Ollama is running
    if curl -sf http://localhost:11434/api/tags > /dev/null 2>&1; then
      MODELS=$(curl -sf http://localhost:11434/api/tags | jq -r '.models | length' 2>/dev/null || echo "0")
      check_met "Ollama running" "$MODELS models available"
    else
      check_warning "Ollama installed but not running" "Start with: ollama serve"
    fi
  else
    check_missing "Ollama not found" "LLM runtime required for GPU features" \
      "curl -fsSL https://ollama.ai/install.sh | sh"
  fi

  echo ""
fi

# =============================================================================
# Optional Monitoring Tools
# =============================================================================

echo "Optional Monitoring Tools:"

if command -v htop &> /dev/null; then
  check_met "htop (process monitor)"
else
  check_warning "htop not found" "Useful for monitoring (optional)"
fi

if command -v iotop &> /dev/null; then
  check_met "iotop (disk I/O monitor)"
else
  check_warning "iotop not found" "Useful for monitoring (optional)"
fi

echo ""

# =============================================================================
# Summary
# =============================================================================

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

TOTAL=$((MET + MISSING + WARNINGS))
echo "Total checks: $TOTAL"
echo -e "${GREEN}Met:${NC}      $MET"
echo -e "${YELLOW}Warnings:${NC} $WARNINGS"
echo -e "${RED}Missing:${NC}  $MISSING"
echo ""

# Show fix commands if requested
if [ "$SHOW_FIX" = true ] && [ ${#FIX_COMMANDS[@]} -gt 0 ]; then
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "Fix Missing Dependencies"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""
  echo "Run these commands to install missing dependencies:"
  echo ""
  for cmd in "${FIX_COMMANDS[@]}"; do
    echo "  $cmd"
  done
  echo ""
  echo "Or install all at once:"
  echo ""
  echo "  sudo dnf install -y \\"
  echo "    gcc gcc-c++ git make automake \\"
  echo "    python3.11 python3.11-devel python3.11-pip \\"
  echo "    podman podman-compose \\"
  echo "    sqlite sqlite-devel \\"
  echo "    curl wget jq bc"
  echo ""
fi

# Exit code
if [ $MISSING -gt 0 ]; then
  echo -e "${RED}❌ Some dependencies are missing${NC}"
  echo ""
  echo "Run with --fix to see commands to install missing dependencies:"
  echo "  $0 --fix"
  echo ""
  exit 1
elif [ $WARNINGS -gt 0 ]; then
  echo -e "${YELLOW}⚠️  All required dependencies met (warnings present)${NC}"
  echo ""
  echo "System is ready but some optional components are missing."
  echo ""
  exit 0
else
  echo -e "${GREEN}✅ All dependencies met!${NC}"
  echo ""
  echo "System is ready for deployment."
  echo "Next step: Run ./scripts/bootstrap.sh"
  echo ""
  exit 0
fi
