#!/bin/bash
# 7-Agent Assistant - Status Check Script for ROG Ally
# Quick health check and status overview

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${BLUE}╔═══════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  7-Agent Assistant - ROG Ally Status Check              ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check Ollama
echo -e "${YELLOW}[1] Ollama Service${NC}"
if pgrep -f "ollama serve" > /dev/null; then
    PID=$(pgrep -f "ollama serve")
    echo -e "  ${GREEN}✓ Running${NC} (PID: $PID)"

    # Check API
    if curl -sf http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo -e "  ${GREEN}✓ API Responsive${NC}"

        # List models
        echo -e "  ${BLUE}Models installed:${NC}"
        ollama list | tail -n +2 | while read -r line; do
            echo -e "    - ${line}"
        done
    else
        echo -e "  ${RED}✗ API Not Responsive${NC}"
    fi
else
    echo -e "  ${RED}✗ Not Running${NC}"
    echo -e "  ${YELLOW}Start with: ollama serve &${NC}"
fi

echo ""

# Check 7-Agent Server
echo -e "${YELLOW}[2] 7-Agent Server${NC}"
if pgrep -f "start_server.py" > /dev/null; then
    PID=$(pgrep -f "start_server.py")
    echo -e "  ${GREEN}✓ Running${NC} (PID: $PID)"

    # Check health endpoint
    if curl -sf http://localhost:8000/health > /dev/null 2>&1; then
        HEALTH=$(curl -s http://localhost:8000/health | jq -r '.status // "unknown"')
        echo -e "  ${GREEN}✓ Health: $HEALTH${NC}"

        # Check ready endpoint
        if curl -sf http://localhost:8000/ready > /dev/null 2>&1; then
            echo -e "  ${GREEN}✓ API Endpoints: Responsive${NC}"
        fi
    else
        echo -e "  ${RED}✗ API Not Responsive${NC}"
    fi
else
    echo -e "  ${RED}✗ Not Running${NC}"
    echo -e "  ${YELLOW}Start with: ~/start_7agent.sh${NC}"
fi

echo ""

# Check Virtual Environment
echo -e "${YELLOW}[3] Python Environment${NC}"
if [ -d "${PROJECT_DIR}/venv" ]; then
    echo -e "  ${GREEN}✓ Virtual environment exists${NC}"

    if [ -f "${PROJECT_DIR}/venv/bin/python" ]; then
        PY_VERSION=$(${PROJECT_DIR}/venv/bin/python --version)
        echo -e "  ${GREEN}✓ Python: $PY_VERSION${NC}"
    fi
else
    echo -e "  ${RED}✗ Virtual environment not found${NC}"
    echo -e "  ${YELLOW}Run setup: ./setup_rog_ally.sh${NC}"
fi

echo ""

# Check Database
echo -e "${YELLOW}[4] Database${NC}"
# Project uses memory.db as primary database
if [ -f "${PROJECT_DIR}/data/memory.db" ]; then
    DB_SIZE=$(du -h "${PROJECT_DIR}/data/memory.db" | cut -f1)
    echo -e "  ${GREEN}✓ Database exists${NC} (memory.db, Size: $DB_SIZE)"

    # Count tables
    TABLE_COUNT=$(sqlite3 "${PROJECT_DIR}/data/memory.db" "SELECT count(*) FROM sqlite_master WHERE type='table';" 2>/dev/null)
    echo -e "  ${GREEN}✓ Tables: $TABLE_COUNT${NC}"

    # Count records
    TASK_COUNT=$(sqlite3 "${PROJECT_DIR}/data/memory.db" "SELECT count(*) FROM tasks;" 2>/dev/null || echo "0")
    echo -e "  ${BLUE}Tasks: $TASK_COUNT${NC}"
elif [ -f "${PROJECT_DIR}/data/assistant.db" ]; then
    # Legacy database location
    DB_SIZE=$(du -h "${PROJECT_DIR}/data/assistant.db" | cut -f1)
    echo -e "  ${YELLOW}⚠️  Using legacy database${NC} (assistant.db, Size: $DB_SIZE)"
    echo -e "  ${YELLOW}Consider running: ./reset_database.sh${NC}"
else
    echo -e "  ${RED}✗ Database not found${NC}"
    echo -e "  ${YELLOW}Initialize: python init_db.py${NC}"
fi

echo ""

# Check Configuration
echo -e "${YELLOW}[5] Configuration${NC}"
if [ -f "${PROJECT_DIR}/.env" ]; then
    echo -e "  ${GREEN}✓ Configuration file exists${NC}"

    # Check key settings
    if grep -q "llama3.2:3b" "${PROJECT_DIR}/.env" 2>/dev/null; then
        echo -e "  ${GREEN}✓ Model: llama3.2:3b (ROG Ally optimized)${NC}"
    fi

    if grep -q "LLM_PROVIDER=ollama" "${PROJECT_DIR}/.env" 2>/dev/null; then
        echo -e "  ${GREEN}✓ Provider: Ollama (local)${NC}"
    fi
else
    echo -e "  ${RED}✗ Configuration not found${NC}"
    echo -e "  ${YELLOW}Run setup: ./setup_rog_ally.sh${NC}"
fi

echo ""

# Resource Usage
echo -e "${YELLOW}[6] Resource Usage${NC}"
TOTAL_MEM=$(free -h | grep Mem: | awk '{print $2}')
USED_MEM=$(free -h | grep Mem: | awk '{print $3}')
echo -e "  ${BLUE}Memory: ${USED_MEM} / ${TOTAL_MEM}${NC}"

if pgrep -f "ollama serve" > /dev/null; then
    OLLAMA_MEM=$(ps aux | grep "ollama serve" | grep -v grep | awk '{print $6/1024}' | head -1)
    echo -e "  ${BLUE}Ollama: ${OLLAMA_MEM%.*}MB${NC}"
fi

if pgrep -f "start_server.py" > /dev/null; then
    SERVER_MEM=$(ps aux | grep "start_server.py" | grep -v grep | awk '{print $6/1024}' | head -1)
    echo -e "  ${BLUE}Server: ${SERVER_MEM%.*}MB${NC}"
fi

echo ""

# Quick Actions
echo -e "${YELLOW}[7] Quick Actions${NC}"
echo -e "  ${BLUE}Start services:${NC}      ~/start_7agent.sh"
echo -e "  ${BLUE}Stop services:${NC}       pkill -f 'start_server.py|ollama serve'"
echo -e "  ${BLUE}Interactive chat:${NC}    cd ${PROJECT_DIR} && source venv/bin/activate && python interactive_chat.py --local"
echo -e "  ${BLUE}View server logs:${NC}    tail -f /tmp/server.log"
echo -e "  ${BLUE}View ollama logs:${NC}    tail -f /tmp/ollama.log"
echo -e "  ${BLUE}API test:${NC}            curl http://localhost:8000/health"

echo ""

# Overall Status
echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
OLLAMA_OK=$(pgrep -f "ollama serve" > /dev/null && echo "1" || echo "0")
SERVER_OK=$(pgrep -f "start_server.py" > /dev/null && echo "1" || echo "0")
VENV_OK=$([ -d "${PROJECT_DIR}/venv" ] && echo "1" || echo "0")
DB_OK=$([ -f "${PROJECT_DIR}/data/memory.db" ] && echo "1" || echo "0")

TOTAL_OK=$((OLLAMA_OK + SERVER_OK + VENV_OK + DB_OK))

if [ $TOTAL_OK -eq 4 ]; then
    echo -e "${GREEN}✅ System Status: FULLY OPERATIONAL${NC}"
elif [ $TOTAL_OK -ge 2 ]; then
    echo -e "${YELLOW}⚠️  System Status: PARTIALLY OPERATIONAL${NC}"
else
    echo -e "${RED}❌ System Status: NOT OPERATIONAL${NC}"
    echo -e "${YELLOW}Run: ./setup_rog_ally.sh${NC}"
fi
echo -e "${BLUE}════════════════════════════════════════════════════════════${NC}"
echo ""
