#!/bin/bash
# 7-Agent Assistant - ROG Ally Cleanup Script
# Safely removes setup components and resets to clean state

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${YELLOW}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${YELLOW}║  7-Agent Assistant - ROG Ally Cleanup                    ║${NC}"
echo -e "${YELLOW}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Warning
echo -e "${RED}⚠️  WARNING: This will:${NC}"
echo "  - Stop all running services"
echo "  - Remove virtual environment"
echo "  - Backup and remove database"
echo "  - Remove logs"
echo "  - Keep: source code, models, configuration backups"
echo ""

read -p "Continue? (yes/no): " confirm
if [ "$confirm" != "yes" ]; then
    echo "Cleanup cancelled."
    exit 0
fi

echo ""
echo "Starting cleanup..."

# Stop services
echo -e "\n${GREEN}[1/6]${NC} Stopping services..."
pkill -f "start_server.py" 2>/dev/null && echo "  ✓ Server stopped" || echo "  - Server not running"
pkill -f "ollama serve" 2>/dev/null && echo "  ✓ Ollama stopped" || echo "  - Ollama not running"
sleep 2

# Backup database
echo -e "\n${GREEN}[2/6]${NC} Backing up database..."
if [ -f "${PROJECT_DIR}/data/assistant.db" ]; then
    mkdir -p "${PROJECT_DIR}/data/backups"
    cp "${PROJECT_DIR}/data/assistant.db" "${PROJECT_DIR}/data/backups/assistant.db.cleanup.$(date +%s)"
    echo "  ✓ Database backed up"
fi

# Remove virtual environment
echo -e "\n${GREEN}[3/6]${NC} Removing virtual environment..."
if [ -d "${PROJECT_DIR}/venv" ]; then
    rm -rf "${PROJECT_DIR}/venv"
    echo "  ✓ Virtual environment removed"
else
    echo "  - No virtual environment found"
fi

# Remove logs
echo -e "\n${GREEN}[4/6]${NC} Removing logs..."
rm -f /tmp/ollama*.log /tmp/server*.log "${PROJECT_DIR}"/*.log
echo "  ✓ Logs removed"

# Remove test artifacts
echo -e "\n${GREEN}[5/6]${NC} Removing test artifacts..."
rm -f "${PROJECT_DIR}/test_results.txt"
rm -f "${PROJECT_DIR}/baseline_metrics.json"
rm -f "${PROJECT_DIR}"/setup_report_*.txt
echo "  ✓ Test artifacts removed"

# Backup configuration
echo -e "\n${GREEN}[6/6]${NC} Backing up configuration..."
if [ -f "${PROJECT_DIR}/.env" ]; then
    cp "${PROJECT_DIR}/.env" "${PROJECT_DIR}/.env.backup.$(date +%s)"
    echo "  ✓ Configuration backed up"
fi

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✅ Cleanup completed!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
echo ""
echo "What's left:"
echo "  ✓ Source code (untouched)"
echo "  ✓ Ollama models (saved at ~/.ollama)"
echo "  ✓ Database backups (data/backups/)"
echo "  ✓ Configuration backups (.env.backup.*)"
echo ""
echo "To setup again:"
echo "  ./setup_rog_ally.sh"
echo ""
