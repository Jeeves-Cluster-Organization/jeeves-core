#!/bin/bash
# Environment Variable Validation Script for Production Deployment
# Ensures all critical variables are set and secure before deployment
#
# Usage: ./scripts/deployment/verify_env.sh [--fix-passwords]
#
# Exit codes:
#   0 - All checks passed
#   1 - Critical errors found
#   2 - Warnings only (deployment allowed but not recommended)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ENV_FILE="$PROJECT_ROOT/.env"

# Colors for output
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Counters
CRITICAL_ERRORS=0
WARNINGS=0
CHECKS_PASSED=0

# Parse arguments
FIX_PASSWORDS=false
if [[ "$1" == "--fix-passwords" ]]; then
    FIX_PASSWORDS=true
fi

print_header() {
    echo -e "${BLUE}=================================================${NC}"
    echo -e "${BLUE}  Production Deployment Environment Validation${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""
}

print_critical() {
    echo -e "${RED}[CRITICAL]${NC} $1"
    ((CRITICAL_ERRORS++))
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
    ((WARNINGS++))
}

print_ok() {
    echo -e "${GREEN}[OK]${NC} $1"
    ((CHECKS_PASSED++))
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Check if .env file exists
check_env_file() {
    print_info "Checking .env file..."

    if [[ ! -f "$ENV_FILE" ]]; then
        print_critical ".env file not found at $ENV_FILE"
        print_info "Run: cp .env.production.example .env"
        return 1
    fi

    print_ok ".env file exists"
    return 0
}

# Source .env file
source_env() {
    if [[ -f "$ENV_FILE" ]]; then
        set -a
        source "$ENV_FILE"
        set +a
    fi
}

# Check PostgreSQL password
check_postgres_password() {
    print_info "Checking PostgreSQL password..."

    if [[ -z "$POSTGRES_PASSWORD" ]]; then
        print_critical "POSTGRES_PASSWORD not set"
        return 1
    fi

    # Check for default/example passwords
    if [[ "$POSTGRES_PASSWORD" == *"CHANGE_ME"* ]] || \
       [[ "$POSTGRES_PASSWORD" == "dev_password_change_in_production" ]] || \
       [[ "$POSTGRES_PASSWORD" == "postgres" ]] || \
       [[ "$POSTGRES_PASSWORD" == "password" ]]; then
        print_critical "POSTGRES_PASSWORD is using default/insecure value"
        if $FIX_PASSWORDS; then
            NEW_PASSWORD=$(openssl rand -base64 48 | tr -d '=' | tr '+/' '-_')
            sed -i.bak "s|POSTGRES_PASSWORD=.*|POSTGRES_PASSWORD=$NEW_PASSWORD|" "$ENV_FILE"
            print_ok "Generated new PostgreSQL password"
        fi
        return 1
    fi

    # Check password strength
    if [[ ${#POSTGRES_PASSWORD} -lt 32 ]]; then
        print_warning "POSTGRES_PASSWORD is shorter than 32 characters (current: ${#POSTGRES_PASSWORD})"
        if $FIX_PASSWORDS; then
            NEW_PASSWORD=$(openssl rand -base64 48 | tr -d '=' | tr '+/' '-_')
            sed -i.bak "s|POSTGRES_PASSWORD=.*|POSTGRES_PASSWORD=$NEW_PASSWORD|" "$ENV_FILE"
            print_ok "Generated stronger PostgreSQL password"
        fi
    else
        print_ok "PostgreSQL password is strong (${#POSTGRES_PASSWORD} characters)"
    fi

    return 0
}

# Check WebSocket auth token
check_websocket_token() {
    print_info "Checking WebSocket authentication token..."

    if [[ -z "$WEBSOCKET_AUTH_TOKEN" ]]; then
        print_warning "WEBSOCKET_AUTH_TOKEN not set (WebSocket auth disabled)"
        return 0
    fi

    # Check for default token
    if [[ "$WEBSOCKET_AUTH_TOKEN" == *"CHANGE_ME"* ]]; then
        print_critical "WEBSOCKET_AUTH_TOKEN is using default value"
        if $FIX_PASSWORDS; then
            NEW_TOKEN=$(openssl rand -hex 32)
            sed -i.bak "s|WEBSOCKET_AUTH_TOKEN=.*|WEBSOCKET_AUTH_TOKEN=$NEW_TOKEN|" "$ENV_FILE"
            print_ok "Generated new WebSocket auth token"
        fi
        return 1
    fi

    # Check token length
    if [[ ${#WEBSOCKET_AUTH_TOKEN} -lt 32 ]]; then
        print_warning "WEBSOCKET_AUTH_TOKEN is shorter than 32 characters (current: ${#WEBSOCKET_AUTH_TOKEN})"
    else
        print_ok "WebSocket auth token is strong (${#WEBSOCKET_AUTH_TOKEN} characters)"
    fi

    return 0
}

# Check CORS origins
check_cors_origins() {
    print_info "Checking CORS origins..."

    if [[ -z "$CORS_ORIGINS" ]]; then
        print_warning "CORS_ORIGINS not set (will use defaults)"
        return 0
    fi

    if [[ "$CORS_ORIGINS" == "*" ]]; then
        print_critical "CORS_ORIGINS is set to '*' (allows all origins - SECURITY RISK)"
        print_info "Set CORS_ORIGINS to your actual domain(s)"
        return 1
    fi

    print_ok "CORS origins configured: $CORS_ORIGINS"
    return 0
}

# Check database backend
check_database_backend() {
    print_info "Checking database backend..."

    if [[ -z "$DATABASE_BACKEND" ]]; then
        print_critical "DATABASE_BACKEND not set"
        return 1
    fi

    if [[ "$DATABASE_BACKEND" != "postgres" ]]; then
        print_warning "DATABASE_BACKEND is '$DATABASE_BACKEND' (expected 'postgres' for production)"
    else
        print_ok "Database backend: PostgreSQL"
    fi

    return 0
}

# Check LLM provider configuration
check_llm_provider() {
    print_info "Checking LLM provider..."

    if [[ -z "$LLM_PROVIDER" ]]; then
        print_critical "LLM_PROVIDER not set"
        return 1
    fi

    if [[ "$LLM_PROVIDER" == "mock" ]]; then
        print_warning "LLM_PROVIDER is 'mock' (testing only, not for production)"
    else
        print_ok "LLM provider: $LLM_PROVIDER"
    fi

    # Check Ollama host if using Ollama
    if [[ "$LLM_PROVIDER" == "ollama" ]]; then
        if [[ -z "$OLLAMA_HOST" ]]; then
            print_critical "OLLAMA_HOST not set (required for ollama provider)"
            return 1
        fi
        print_ok "Ollama host: $OLLAMA_HOST"
    fi

    return 0
}

# Check log level
check_log_level() {
    print_info "Checking log level..."

    if [[ "$LOG_LEVEL" == "DEBUG" ]]; then
        print_warning "LOG_LEVEL is DEBUG (may leak sensitive data in production)"
        print_info "Recommended: LOG_LEVEL=INFO for production"
    elif [[ "$LOG_LEVEL" == "INFO" ]] || [[ "$LOG_LEVEL" == "WARNING" ]]; then
        print_ok "Log level: $LOG_LEVEL (production-safe)"
    else
        print_ok "Log level: ${LOG_LEVEL:-INFO}"
    fi

    return 0
}

# Check API reload setting
check_api_reload() {
    print_info "Checking API reload setting..."

    if [[ "$API_RELOAD" == "true" ]]; then
        print_warning "API_RELOAD is true (development mode, not recommended for production)"
        print_info "Set API_RELOAD=false for production"
    else
        print_ok "API auto-reload disabled (production mode)"
    fi

    return 0
}

# Check confirmations enabled
check_confirmations() {
    print_info "Checking user confirmations..."

    if [[ "$ENABLE_CONFIRMATIONS" == "false" ]]; then
        print_warning "ENABLE_CONFIRMATIONS is false (risky operations won't ask for confirmation)"
        print_info "Recommended: ENABLE_CONFIRMATIONS=true for production"
    else
        print_ok "User confirmations enabled"
    fi

    return 0
}

# Print summary
print_summary() {
    echo ""
    echo -e "${BLUE}=================================================${NC}"
    echo -e "${BLUE}  Validation Summary${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""
    echo -e "  ${GREEN}Passed:${NC}   $CHECKS_PASSED"
    echo -e "  ${YELLOW}Warnings:${NC} $WARNINGS"
    echo -e "  ${RED}Critical:${NC} $CRITICAL_ERRORS"
    echo ""

    if [[ $CRITICAL_ERRORS -gt 0 ]]; then
        echo -e "${RED}❌ DEPLOYMENT BLOCKED${NC}"
        echo ""
        echo "Critical errors must be fixed before deployment."
        if ! $FIX_PASSWORDS; then
            echo ""
            echo "Quick fix: Run with --fix-passwords to auto-generate secure passwords:"
            echo "  ./scripts/deployment/verify_env.sh --fix-passwords"
        fi
        echo ""
        return 1
    elif [[ $WARNINGS -gt 0 ]]; then
        echo -e "${YELLOW}⚠️  WARNINGS DETECTED${NC}"
        echo ""
        echo "Deployment is allowed but not recommended."
        echo "Review warnings above and fix if possible."
        echo ""
        return 2
    else
        echo -e "${GREEN}✅ ALL CHECKS PASSED${NC}"
        echo ""
        echo "Environment is ready for production deployment."
        echo ""
        return 0
    fi
}

# Main execution
main() {
    print_header

    # Check .env file exists
    if ! check_env_file; then
        exit 1
    fi

    # Source environment variables
    source_env

    # Run all checks
    check_postgres_password || true
    check_websocket_token || true
    check_cors_origins || true
    check_database_backend || true
    check_llm_provider || true
    check_log_level || true
    check_api_reload || true
    check_confirmations || true

    # Print summary and exit with appropriate code
    print_summary
    exit $?
}

# Run main
main
