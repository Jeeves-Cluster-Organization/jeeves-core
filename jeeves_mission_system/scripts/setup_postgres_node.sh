#!/bin/bash
#
# PostgreSQL + pgvector Container Setup Script
# Sets up only the PostgreSQL node/container
#
# Previously 317 lines; now uses lib/database.sh for shared functionality.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source library functions
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/database.sh"

# Print header
echo ""
print_info "======================================================================"
print_info "PostgreSQL + pgvector Node Setup"
print_info "======================================================================"
echo ""

# Detect container runtime
detect_container_runtime || exit 1

echo ""
print_info "Configuration:"
echo "  Database: $POSTGRES_DB"
echo "  User:     $POSTGRES_USER"
echo "  Port:     $POSTGRES_PORT"
echo ""

# Start PostgreSQL container
start_postgres_container || exit 1

# Test authentication
print_step "Testing authentication..."
if test_postgres_auth; then
    print_success "Authentication successful"
else
    print_error "Authentication failed with current credentials"
    print_warning "The volume may have different credentials from a previous setup."
    echo ""
    echo "Options:"
    echo "  1) Reset volume: ./scripts/reset_postgres_volume.sh --force"
    echo "  2) Manually update password in container"
    exit 1
fi

# Verify PostgreSQL version
print_step "Verifying PostgreSQL..."
PG_VERSION=$(get_postgres_version)
if [ -n "$PG_VERSION" ]; then
    print_success "PostgreSQL: $(echo "$PG_VERSION" | cut -d',' -f1)"
else
    print_error "Could not get PostgreSQL version"
fi

# Check pgvector extension
ensure_pgvector || true

# Print connection info
echo ""
print_info "======================================================================"
print_success "PostgreSQL node setup complete!"
print_info "======================================================================"

print_postgres_connection_info

echo "View logs:"
echo "  $CONTAINER_CMD logs -f $POSTGRES_CONTAINER"
echo ""
echo "Stop container:"
echo "  $CONTAINER_CMD stop $POSTGRES_CONTAINER"
echo ""
