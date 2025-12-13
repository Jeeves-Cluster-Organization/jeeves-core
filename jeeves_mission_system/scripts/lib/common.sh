#!/bin/bash
# Common shared functions for all scripts
# Source this file at the beginning of any script

# Color codes for output
readonly COLOR_RESET='\033[0m'
readonly COLOR_RED='\033[0;31m'
readonly COLOR_GREEN='\033[0;32m'
readonly COLOR_YELLOW='\033[0;33m'
readonly COLOR_BLUE='\033[0;34m'
readonly COLOR_CYAN='\033[0;36m'

# Print functions with colors
print_success() {
    echo -e "${COLOR_GREEN}✓ $1${COLOR_RESET}"
}

print_error() {
    echo -e "${COLOR_RED}✗ $1${COLOR_RESET}"
}

print_warning() {
    echo -e "${COLOR_YELLOW}⚠ $1${COLOR_RESET}"
}

print_info() {
    echo -e "${COLOR_BLUE}ℹ $1${COLOR_RESET}"
}

print_step() {
    echo -e "${COLOR_CYAN}▶ $1${COLOR_RESET}"
}

# Get project root directory
get_project_root() {
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    echo "$(cd "$script_dir/../.." && pwd)"
}

# Change to project root
cd_project_root() {
    local root="$(get_project_root)"
    cd "$root" || {
        print_error "Failed to change to project root: $root"
        return 1
    }
    print_info "Working directory: $root"
}

# Check if command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Check Python 3 installation
check_python() {
    if ! command_exists python3; then
        print_error "Python 3 not found. Please install Python 3.8 or higher."
        return 1
    fi
    local python_version=$(python3 --version 2>&1 | awk '{print $2}')
    print_success "Python 3 found: $python_version"
    return 0
}

# Check if virtual environment exists
check_venv() {
    if [ ! -d "venv" ]; then
        print_error "Virtual environment not found. Run bootstrap.sh first."
        return 1
    fi
    return 0
}

# Activate virtual environment
activate_venv() {
    if [ ! -f "venv/bin/activate" ]; then
        print_error "Virtual environment not found at venv/bin/activate"
        return 1
    fi

    # Source the virtual environment
    source venv/bin/activate
    print_success "Virtual environment activated"
    return 0
}

# Load environment variables from .env file
load_env() {
    if [ -f .env ]; then
        set -a
        source .env
        set +a
        print_success "Environment variables loaded from .env"
    else
        print_warning ".env file not found, using defaults"
    fi
}

# Check if .env.example exists and .env doesn't
check_env_file() {
    if [ ! -f .env ] && [ -f .env.example ]; then
        print_warning ".env file not found"
        print_info "Copying .env.example to .env"
        cp .env.example .env
        print_success "Created .env from .env.example"
        print_warning "Please edit .env and configure your settings"
        return 1
    fi
    return 0
}

# Clean Python cache files
clean_python_cache() {
    print_step "Cleaning Python cache files..."
    find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null
    find . -type f -name "*.pyc" -delete 2>/dev/null
    find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null
    print_success "Python cache cleaned"
}

# Check if a port is in use
check_port() {
    local port=$1
    if command_exists lsof; then
        if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
            return 0  # Port is in use
        fi
    elif command_exists netstat; then
        if netstat -an | grep -q ":$port.*LISTEN"; then
            return 0  # Port is in use
        fi
    fi
    return 1  # Port is free
}

# Wait for service to be ready
wait_for_service() {
    local host="${1:-localhost}"
    local port="${2:-8000}"
    local max_attempts="${3:-30}"
    local attempt=0

    print_step "Waiting for service at $host:$port..."

    while [ $attempt -lt $max_attempts ]; do
        if check_port $port; then
            print_success "Service is ready at $host:$port"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 1
    done

    print_error "Service did not start within $max_attempts seconds"
    return 1
}

# Export functions for use in other scripts
export -f print_success
export -f print_error
export -f print_warning
export -f print_info
export -f print_step
export -f get_project_root
export -f cd_project_root
export -f command_exists
export -f check_python
export -f check_venv
export -f activate_venv
export -f load_env
export -f check_env_file
export -f clean_python_cache
export -f check_port
export -f wait_for_service
