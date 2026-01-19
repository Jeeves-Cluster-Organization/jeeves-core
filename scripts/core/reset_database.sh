#!/bin/bash
# Reset database using modular library
# Backs up existing database and reinitializes

set -e

# Get script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source shared libraries
source "$SCRIPT_DIR/../lib/common.sh"
source "$SCRIPT_DIR/../lib/database.sh"

# Change to project root
cd "$PROJECT_ROOT"

main() {
    print_step "7-Agent Assistant - Database Reset"
    echo ""

    # Confirm action
    if [ "$1" != "--yes" ] && [ "$1" != "-y" ]; then
        print_warning "This will backup and reset the database"
        read -p "Continue? (y/N) " -n 1 -r
        echo ""
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            print_info "Aborted"
            exit 0
        fi
    fi

    # Reset database
    reset_database
    local result=$?

    echo ""
    if [ $result -eq 0 ]; then
        print_success "Database reset complete!"
        check_database_size
    else
        print_error "Database reset failed"
        exit 1
    fi
}

main "$@"
