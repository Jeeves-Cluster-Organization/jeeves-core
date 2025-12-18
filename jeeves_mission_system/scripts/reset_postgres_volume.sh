#!/bin/bash
#
# PostgreSQL Volume Reset Script
# Handles credential mismatch issues when the PostgreSQL volume was created with different credentials
#
# Usage:
#   ./scripts/reset_postgres_volume.sh          # Interactive mode
#   ./scripts/reset_postgres_volume.sh --force  # Force reset without prompting

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source library functions
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/database.sh"

print_info "======================================================================"
print_info "PostgreSQL Volume Reset Tool"
print_info "======================================================================"
echo ""

# Parse arguments
FORCE=false
if [ "$1" == "--force" ] || [ "$1" == "-f" ]; then
    FORCE=true
fi

# Detect Docker
detect_container_runtime || exit 1

VOLUME_NAME="jeeves_postgres-data"

echo ""
print_warning "This script will:"
echo "  1. Stop the PostgreSQL container"
echo "  2. Remove the PostgreSQL container"
echo "  3. Delete the PostgreSQL data volume (ALL DATA WILL BE LOST)"
echo "  4. Recreate the container with current credentials from .env"
echo ""
print_error "WARNING: This will DELETE ALL DATABASE DATA!"
echo ""

# Check if volume exists
if ! docker volume inspect "$VOLUME_NAME" &> /dev/null 2>&1; then
    print_info "Volume $VOLUME_NAME does not exist."
    echo "No reset needed. Run the setup script to create a fresh database."
    exit 0
fi

# Confirmation prompt (unless --force)
if [ "$FORCE" != true ]; then
    read -p "Are you sure you want to continue? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Aborted."
        exit 0
    fi
fi

echo ""
print_step "Step 1: Stopping PostgreSQL container..."
stop_postgres_container

echo ""
print_step "Step 2: Removing PostgreSQL container..."
if pg_container_exists; then
    docker rm -f "$POSTGRES_CONTAINER"
    print_success "Container removed"
else
    print_info "Container does not exist"
fi

echo ""
print_step "Step 3: Deleting PostgreSQL data volume..."
if docker volume inspect "$VOLUME_NAME" &> /dev/null 2>&1; then
    docker volume rm "$VOLUME_NAME"
    print_success "Volume deleted"
else
    print_info "Volume does not exist"
fi

echo ""
print_step "Step 4: Recreating PostgreSQL container..."

# Load environment variables from .env if it exists
load_env

# Start container
start_postgres_container || {
    print_error "PostgreSQL failed to start"
    echo "Check logs with: docker logs $POSTGRES_CONTAINER"
    exit 1
}

echo ""
print_info "======================================================================"
print_success "PostgreSQL volume reset complete!"
print_info "======================================================================"
echo ""
echo "The PostgreSQL container has been recreated with fresh credentials."
echo "You can now initialize the database schema by running:"
echo ""
echo "  python init_db.py --backend postgres"
echo ""
