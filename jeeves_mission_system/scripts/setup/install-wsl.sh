#!/bin/bash
# =============================================================================
# Jeeves FF - WSL Setup Script
# =============================================================================
# Wrapper for WSL-specific setup
# =============================================================================

set -e

# Colors
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Jeeves FF - WSL Setup${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Check if running on WSL
if ! grep -qi microsoft /proc/version; then
  echo "This script is for WSL (Windows Subsystem for Linux) only."
  echo "For native Linux, use: ./setup.sh"
  exit 1
fi

# Run main setup script
echo "Running main setup script with WSL optimizations..."
echo ""

# WSL-specific environment variables
export WSL_DISTRO_NAME=${WSL_DISTRO_NAME:-Ubuntu}

# Call main setup
./scripts/setup/install.sh "$@"

# WSL-specific post-install tips
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "WSL-Specific Notes:"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "1. PostgreSQL container is accessible from Windows at:"
echo "   localhost:5432"
echo ""
echo "2. API server will be accessible from Windows at:"
echo "   http://localhost:8000"
echo ""
echo "3. To access from Windows browser:"
echo "   - Navigate to http://localhost:8000/health"
echo "   - Chat UI: http://localhost:8000/chat"
echo ""
echo "4. If Ollama service fails to start:"
echo "   Run manually: ollama serve &"
echo ""
