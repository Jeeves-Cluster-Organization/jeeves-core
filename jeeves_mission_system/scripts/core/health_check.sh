#!/bin/bash
# =============================================================================
# Health Check Script
# =============================================================================
# Purpose: Quick health check for running 7-agent assistant system
# Usage: ./scripts/health_check.sh [OPTIONS]
#
# Options:
#   --api-url URL     API URL to check (default: http://localhost:8000)
#   --timeout SEC     Timeout in seconds (default: 5)
#   --json            Output in JSON format
#   --help            Show this help message
#
# Exit Codes:
#   0 - System healthy
#   1 - System unhealthy
#   2 - Degraded (some components failing)
# =============================================================================

set -u

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Defaults
API_URL="http://localhost:8000"
TIMEOUT=5
JSON_OUTPUT=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --api-url) API_URL="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    --json) JSON_OUTPUT=true; shift ;;
    --help)
      grep "^#" "$0" | grep -v "^#!/" | sed 's/^# //' | sed 's/^#//'
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
ASSISTANT_DIR="$REPO_ROOT/assistant_7agent"

# Health check results
declare -A CHECKS
OVERALL_STATUS="healthy"

# Helper functions
check_endpoint() {
  local url="$1"
  local name="$2"

  if curl -sf --max-time "$TIMEOUT" "$url" > /dev/null 2>&1; then
    CHECKS["$name"]="pass"
    return 0
  else
    CHECKS["$name"]="fail"
    return 1
  fi
}

check_json_endpoint() {
  local url="$1"
  local name="$2"

  RESPONSE=$(curl -sf --max-time "$TIMEOUT" "$url" 2>/dev/null)
  if [ $? -eq 0 ] && echo "$RESPONSE" | jq . > /dev/null 2>&1; then
    CHECKS["$name"]="pass"
    echo "$RESPONSE"
    return 0
  else
    CHECKS["$name"]="fail"
    return 1
  fi
}

# =============================================================================
# Run Health Checks
# =============================================================================

# 1. API Health Endpoint
API_HEALTH=$(check_json_endpoint "$API_URL/health" "api_health")
API_HEALTH_STATUS=$?

# 2. API Metrics (Prometheus)
check_endpoint "$API_URL/metrics" "api_metrics"
METRICS_STATUS=$?

# 3. Database connectivity (via API)
DB_CHECK=$(curl -sf --max-time "$TIMEOUT" "$API_URL/health" 2>/dev/null | jq -r '.database // "unknown"' 2>/dev/null)
if [ "$DB_CHECK" = "healthy" ] || [ "$DB_CHECK" = "ok" ]; then
  CHECKS["database"]="pass"
  DB_STATUS=0
else
  CHECKS["database"]="fail"
  DB_STATUS=1
fi

# 4. Ollama connectivity
if curl -sf --max-time "$TIMEOUT" "http://localhost:11434/api/tags" > /dev/null 2>&1; then
  CHECKS["ollama"]="pass"
  OLLAMA_STATUS=0
else
  CHECKS["ollama"]="warn"
  OLLAMA_STATUS=1
fi

# 5. Database file lock check
if [ -f "$ASSISTANT_DIR/data/memory.db" ]; then
  if fuser "$ASSISTANT_DIR/data/memory.db" > /dev/null 2>&1; then
    CHECKS["db_lock"]="pass"
  else
    CHECKS["db_lock"]="warn"
  fi
else
  CHECKS["db_lock"]="fail"
fi

# 6. Process check
if pgrep -f "uvicorn.*api.server:app" > /dev/null 2>&1; then
  CHECKS["api_process"]="pass"
  API_PROC_STATUS=0
else
  CHECKS["api_process"]="fail"
  API_PROC_STATUS=1
fi

# 7. Port check
if lsof -Pi :8000 -sTCP:LISTEN -t >/dev/null 2>&1; then
  CHECKS["port_8000"]="pass"
else
  CHECKS["port_8000"]="fail"
fi

# Determine overall status
if [ $API_HEALTH_STATUS -ne 0 ] || [ $DB_STATUS -ne 0 ] || [ $API_PROC_STATUS -ne 0 ]; then
  OVERALL_STATUS="unhealthy"
  EXIT_CODE=1
elif [ $OLLAMA_STATUS -ne 0 ] || [ $METRICS_STATUS -ne 0 ]; then
  OVERALL_STATUS="degraded"
  EXIT_CODE=2
else
  OVERALL_STATUS="healthy"
  EXIT_CODE=0
fi

# =============================================================================
# Output Results
# =============================================================================

if [ "$JSON_OUTPUT" = true ]; then
  # JSON output
  cat <<EOF
{
  "status": "$OVERALL_STATUS",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "checks": {
    "api_health": "${CHECKS[api_health]:-unknown}",
    "api_metrics": "${CHECKS[api_metrics]:-unknown}",
    "database": "${CHECKS[database]:-unknown}",
    "ollama": "${CHECKS[ollama]:-unknown}",
    "db_lock": "${CHECKS[db_lock]:-unknown}",
    "api_process": "${CHECKS[api_process]:-unknown}",
    "port_8000": "${CHECKS[port_8000]:-unknown}"
  },
  "api_url": "$API_URL"
}
EOF
else
  # Human-readable output
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "System Health Check"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # Print each check
  for check in api_health api_metrics database ollama db_lock api_process port_8000; do
    status="${CHECKS[$check]:-unknown}"
    label=$(echo "$check" | tr '_' ' ' | sed 's/.*/\u&/')

    case $status in
      pass)
        echo -e "${GREEN}✓${NC} $label"
        ;;
      fail)
        echo -e "${RED}✗${NC} $label"
        ;;
      warn)
        echo -e "${YELLOW}⚠${NC} $label"
        ;;
      *)
        echo -e "${BLUE}?${NC} $label"
        ;;
    esac
  done

  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  case $OVERALL_STATUS in
    healthy)
      echo -e "${GREEN}✅ System is healthy${NC}"
      ;;
    degraded)
      echo -e "${YELLOW}⚠️  System is degraded (some components unavailable)${NC}"
      ;;
    unhealthy)
      echo -e "${RED}❌ System is unhealthy${NC}"
      ;;
  esac

  echo ""
  echo "API URL: $API_URL"
  echo "Timestamp: $(date)"
  echo ""
fi

exit $EXIT_CODE
