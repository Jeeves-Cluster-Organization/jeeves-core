#!/bin/bash
# 7-Agent Assistant - ROG Ally Setup (Thin Wrapper)
# Optimized for Bazzite OS with AMD RDNA 3 GPU
#
# This script delegates to core/bootstrap.sh with ROG Ally-specific configuration.
# Previously this was 932 lines of duplicated code; now it's a 50-line wrapper.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PROJECT_DIR="$(cd "${SCRIPTS_DIR}/.." && pwd)"

# ROG Ally specific defaults
export DEFAULT_MODEL="${DEFAULT_MODEL:-llama3.2:3b}"
export OLLAMA_NUM_PARALLEL="${OLLAMA_NUM_PARALLEL:-1}"
export OLLAMA_NUM_GPU="${OLLAMA_NUM_GPU:-1}"
export OLLAMA_NUM_THREAD="${OLLAMA_NUM_THREAD:-4}"

# Conservative token limits for shared RAM on integrated GPU
export PLANNER_MAX_TOKENS="${PLANNER_MAX_TOKENS:-400}"
export VALIDATOR_MAX_TOKENS="${VALIDATOR_MAX_TOKENS:-600}"
export CRITIC_MAX_TOKENS="${CRITIC_MAX_TOKENS:-400}"
export META_VALIDATOR_MAX_TOKENS="${META_VALIDATOR_MAX_TOKENS:-800}"

# Source common library
source "${SCRIPTS_DIR}/lib/common.sh"

print_header "ROG Ally Setup"
echo ""
echo "Platform: Bazzite OS + AMD RDNA 3 GPU"
echo "Model: ${DEFAULT_MODEL}"
echo ""

# Check for AMD GPU
if lspci | grep -i vga | grep -qi amd; then
    print_success "AMD GPU detected"
else
    print_warning "AMD GPU not detected - continuing anyway"
fi

# Check for Bazzite OS
if grep -q "bazzite\|fedora" /etc/os-release 2>/dev/null; then
    print_success "Bazzite/Fedora OS detected"
else
    print_warning "Non-Bazzite OS - script may still work"
fi

echo ""
print_info "Delegating to core/bootstrap.sh..."
echo ""

# Run the main bootstrap script
exec "${SCRIPTS_DIR}/core/bootstrap.sh" "$@"
