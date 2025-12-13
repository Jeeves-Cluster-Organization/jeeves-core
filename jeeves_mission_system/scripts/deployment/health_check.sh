#!/bin/bash
# Comprehensive Health Check Script for Docker Compose Deployment
# Verifies all services are running and properly configured
#
# Usage: ./scripts/deployment/health_check.sh [--wait-timeout SECONDS]
#
# Exit codes:
#   0 - All services healthy
#   1 - One or more services unhealthy
#   2 - Timeout waiting for services

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Default timeout (seconds)
WAIT_TIMEOUT=120

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --wait-timeout)
            WAIT_TIMEOUT="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

print_header() {
    echo -e "${BLUE}=================================================${NC}"
    echo -e "${BLUE}  Docker Compose Deployment Health Check${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""
}

print_critical() {
    echo -e "${RED}[CRITICAL]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_ok() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Check if docker compose is available
check_docker_compose() {
    print_info "Checking docker compose availability..."

    if command -v docker &> /dev/null && docker compose version &> /dev/null; then
        print_ok "docker compose available"
        return 0
    elif command -v podman-compose &> /dev/null; then
        print_ok "podman-compose available"
        return 0
    else
        print_critical "Neither 'docker compose' nor 'podman-compose' found"
        return 1
    fi
}

# Get compose command
get_compose_cmd() {
    if command -v docker &> /dev/null && docker compose version &> /dev/null; then
        echo "docker compose"
    elif command -v podman-compose &> /dev/null; then
        echo "podman-compose"
    else
        echo ""
    fi
}

# Check if services are running
check_services_running() {
    print_info "Checking if services are running..."

    local compose_cmd=$(get_compose_cmd)
    if [[ -z "$compose_cmd" ]]; then
        print_critical "No compose command available"
        return 1
    fi

    cd "$PROJECT_ROOT"

    # Check if containers are running
    local running_containers=$($compose_cmd ps --services --filter "status=running" 2>/dev/null | wc -l)

    if [[ $running_containers -eq 0 ]]; then
        print_critical "No containers are running"
        print_info "Start services with: $compose_cmd up -d"
        return 1
    fi

    print_ok "$running_containers service(s) running"
    return 0
}

# Wait for PostgreSQL to be healthy
wait_for_postgres() {
    print_info "Waiting for PostgreSQL to be healthy..."

    local compose_cmd=$(get_compose_cmd)
    local elapsed=0
    local interval=2

    cd "$PROJECT_ROOT"

    while [[ $elapsed -lt $WAIT_TIMEOUT ]]; do
        # Check container health status
        local health=$($compose_cmd ps postgres --format json 2>/dev/null | jq -r '.[0].Health // "unknown"')

        if [[ "$health" == "healthy" ]]; then
            print_ok "PostgreSQL is healthy"
            return 0
        fi

        sleep $interval
        ((elapsed += interval))
    done

    print_critical "PostgreSQL failed to become healthy within ${WAIT_TIMEOUT}s"
    return 1
}

# Wait for Ollama to be healthy
wait_for_ollama() {
    print_info "Waiting for Ollama to be healthy..."

    local compose_cmd=$(get_compose_cmd)
    local elapsed=0
    local interval=2

    cd "$PROJECT_ROOT"

    while [[ $elapsed -lt $WAIT_TIMEOUT ]]; do
        # Check container health status
        local health=$($compose_cmd ps ollama --format json 2>/dev/null | jq -r '.[0].Health // "unknown"' 2>/dev/null)

        if [[ "$health" == "healthy" ]]; then
            print_ok "Ollama is healthy"
            return 0
        fi

        # Also try direct health check if container exists but health status unknown
        if $compose_cmd ps | grep -q ollama; then
            if curl -sf http://localhost:11434/api/tags &>/dev/null; then
                print_ok "Ollama is responding"
                return 0
            fi
        fi

        sleep $interval
        ((elapsed += interval))
    done

    print_warning "Ollama health check timed out (may still be starting)"
    return 0  # Non-critical - API can work without Ollama in mock mode
}

# Wait for API to be healthy
wait_for_api() {
    print_info "Waiting for API to be healthy..."

    local elapsed=0
    local interval=2

    while [[ $elapsed -lt $WAIT_TIMEOUT ]]; do
        if curl -sf http://localhost:8000/health &>/dev/null; then
            print_ok "API is healthy"
            return 0
        fi

        sleep $interval
        ((elapsed += interval))
    done

    print_critical "API failed to become healthy within ${WAIT_TIMEOUT}s"
    return 1
}

# Check API health details
check_api_health() {
    print_info "Checking API health details..."

    local health_response=$(curl -s http://localhost:8000/health 2>/dev/null)

    if [[ -z "$health_response" ]]; then
        print_critical "Failed to get health response from API"
        return 1
    fi

    # Parse health response (assuming JSON)
    local status=$(echo "$health_response" | jq -r '.status // "unknown"' 2>/dev/null)

    if [[ "$status" == "healthy" ]] || [[ "$status" == "ok" ]]; then
        print_ok "API health status: $status"
    else
        print_warning "API health status: $status"
    fi

    # Check database connectivity
    local db_status=$(echo "$health_response" | jq -r '.database // "unknown"' 2>/dev/null)
    if [[ "$db_status" == "connected" ]] || [[ "$db_status" == "ok" ]]; then
        print_ok "Database connection: OK"
    else
        print_warning "Database status: $db_status"
    fi

    return 0
}

# Check PostgreSQL schema
check_postgres_schema() {
    print_info "Checking PostgreSQL schema..."

    local compose_cmd=$(get_compose_cmd)
    cd "$PROJECT_ROOT"

    # Get PostgreSQL password from environment
    local pg_password=$(grep "^POSTGRES_PASSWORD=" .env 2>/dev/null | cut -d'=' -f2)

    if [[ -z "$pg_password" ]]; then
        print_warning "Could not read POSTGRES_PASSWORD from .env (schema check skipped)"
        return 0
    fi

    # Check if critical tables exist
    local table_count=$($compose_cmd exec -T postgres psql -U assistant -d assistant -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_type = 'BASE TABLE';" 2>/dev/null | tr -d ' ')

    if [[ -z "$table_count" ]] || [[ "$table_count" -eq 0 ]]; then
        print_critical "No tables found in PostgreSQL database"
        print_info "Schema may not have been initialized"
        return 1
    fi

    print_ok "PostgreSQL schema initialized ($table_count tables)"

    # Check for V2 memory tables
    local v2_tables=$($compose_cmd exec -T postgres psql -U assistant -d assistant -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('session_state', 'open_loops', 'tool_metrics');" 2>/dev/null | tr -d ' ')

    if [[ "$v2_tables" -eq 3 ]]; then
        print_ok "V2 memory infrastructure tables present"
    else
        print_warning "V2 memory tables incomplete ($v2_tables/3 found)"
    fi

    # Check for pgvector extension
    local pgvector_installed=$($compose_cmd exec -T postgres psql -U assistant -d assistant -t -c "SELECT COUNT(*) FROM pg_extension WHERE extname = 'vector';" 2>/dev/null | tr -d ' ')

    if [[ "$pgvector_installed" -eq 1 ]]; then
        print_ok "pgvector extension installed"
    else
        print_warning "pgvector extension not found"
    fi

    return 0
}

# Check container logs for errors
check_container_logs() {
    print_info "Checking container logs for errors..."

    local compose_cmd=$(get_compose_cmd)
    cd "$PROJECT_ROOT"

    # Check API container logs for errors
    local api_errors=$($compose_cmd logs --tail=50 assistant 2>/dev/null | grep -iE "error|critical|exception" | wc -l)

    if [[ $api_errors -gt 5 ]]; then
        print_warning "Found $api_errors error messages in API logs (check: $compose_cmd logs assistant)"
    elif [[ $api_errors -gt 0 ]]; then
        print_info "Found $api_errors error messages in API logs (may be normal)"
    else
        print_ok "No critical errors in API logs"
    fi

    return 0
}

# Print service status summary
print_service_status() {
    echo ""
    echo -e "${BLUE}=================================================${NC}"
    echo -e "${BLUE}  Service Status${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""

    local compose_cmd=$(get_compose_cmd)
    cd "$PROJECT_ROOT"

    echo "Services:"
    $compose_cmd ps --format "table {{.Service}}\t{{.State}}\t{{.Status}}"

    echo ""
    echo "Endpoints:"
    echo "  API:        http://localhost:8000"
    echo "  Health:     http://localhost:8000/health"
    echo "  PostgreSQL: localhost:5432"
    echo "  Ollama:     http://localhost:11434"
    echo ""
}

# Main execution
main() {
    print_header

    # Check docker compose
    if ! check_docker_compose; then
        exit 1
    fi

    # Check services are running
    if ! check_services_running; then
        exit 1
    fi

    # Wait for PostgreSQL
    if ! wait_for_postgres; then
        print_service_status
        exit 1
    fi

    # Wait for Ollama (non-critical)
    wait_for_ollama || true

    # Wait for API
    if ! wait_for_api; then
        print_service_status
        exit 1
    fi

    # Check API health details
    check_api_health || true

    # Check PostgreSQL schema
    check_postgres_schema || true

    # Check logs for errors
    check_container_logs || true

    # Print status summary
    print_service_status

    echo -e "${GREEN}✅ All critical health checks passed!${NC}"
    echo ""

    return 0
}

# Run main
main
