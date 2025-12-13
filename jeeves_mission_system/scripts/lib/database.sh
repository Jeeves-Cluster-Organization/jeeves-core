#!/bin/bash
# Database operations library
# Source common.sh before using this library

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source common functions if not already loaded
if [ -z "$(type -t print_info)" ]; then
    source "$SCRIPT_DIR/common.sh"
fi

# Database configuration
DB_PATH="${DB_PATH:-data/memory.db}"
DB_BACKUP_DIR="${DB_BACKUP_DIR:-data/backups}"

# Initialize database
init_database() {
    print_step "Initializing database at $DB_PATH..."

    # Ensure data directory exists
    mkdir -p "$(dirname "$DB_PATH")"

    # Run Python initialization script
    if [ -f "init_db.py" ]; then
        python3 init_db.py
        if [ $? -eq 0 ]; then
            print_success "Database initialized successfully"
            return 0
        else
            print_error "Database initialization failed"
            return 1
        fi
    else
        print_error "init_db.py not found"
        return 1
    fi
}

# Backup database
backup_database() {
    local backup_name="${1:-memory.db.backup.$(date +%Y%m%d_%H%M%S)}"

    if [ ! -f "$DB_PATH" ]; then
        print_warning "Database file $DB_PATH does not exist, skipping backup"
        return 1
    fi

    # Create backup directory
    mkdir -p "$DB_BACKUP_DIR"

    local backup_path="$DB_BACKUP_DIR/$backup_name"

    print_step "Backing up database to $backup_path..."
    cp "$DB_PATH" "$backup_path"

    if [ $? -eq 0 ]; then
        print_success "Database backed up to $backup_path"
        echo "$backup_path"
        return 0
    else
        print_error "Database backup failed"
        return 1
    fi
}

# Reset database (backup, delete, reinitialize)
reset_database() {
    print_step "Resetting database..."

    # Backup existing database if it exists
    if [ -f "$DB_PATH" ]; then
        backup_database "memory.db.before_reset.$(date +%Y%m%d_%H%M%S)"
    fi

    # Remove database files
    print_info "Removing database files..."
    rm -f "$DB_PATH" "${DB_PATH}-shm" "${DB_PATH}-wal"

    # Reinitialize
    init_database
    return $?
}

# Validate database
validate_database() {
    print_step "Validating database..."

    if [ ! -f "$DB_PATH" ]; then
        print_error "Database file $DB_PATH does not exist"
        return 1
    fi

    # Use Python to validate SQLite database
    python3 << EOF
import sqlite3
import sys

try:
    conn = sqlite3.connect('$DB_PATH')
    cursor = conn.cursor()

    # Check if database is valid by querying schema
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table' LIMIT 1")
    result = cursor.fetchone()

    if result:
        print("✓ Database is valid and contains tables")
        sys.exit(0)
    else:
        print("✗ Database is empty (no tables found)")
        sys.exit(1)

except sqlite3.DatabaseError as e:
    print(f"✗ Database validation failed: {e}")
    sys.exit(1)
except Exception as e:
    print(f"✗ Unexpected error: {e}")
    sys.exit(1)
finally:
    if 'conn' in locals():
        conn.close()
EOF

    if [ $? -eq 0 ]; then
        print_success "Database validation passed"
        return 0
    else
        print_error "Database validation failed"
        return 1
    fi
}

# Check database size
check_database_size() {
    if [ ! -f "$DB_PATH" ]; then
        print_warning "Database file does not exist"
        return 1
    fi

    local size=$(du -h "$DB_PATH" | cut -f1)
    print_info "Database size: $size"
    echo "$size"
}

# List recent backups
list_backups() {
    local limit="${1:-5}"

    if [ ! -d "$DB_BACKUP_DIR" ]; then
        print_info "No backup directory found"
        return 0
    fi

    print_info "Recent database backups (latest $limit):"
    ls -lt "$DB_BACKUP_DIR" | head -n $((limit + 1)) | tail -n $limit | awk '{print "  " $9 " (" $5 " bytes, " $6 " " $7 " " $8 ")"}'
}

# Restore database from backup
restore_database() {
    local backup_file="$1"

    if [ -z "$backup_file" ]; then
        print_error "Usage: restore_database <backup_file>"
        list_backups
        return 1
    fi

    if [ ! -f "$backup_file" ]; then
        print_error "Backup file not found: $backup_file"
        return 1
    fi

    print_step "Restoring database from $backup_file..."

    # Backup current database before restoring
    if [ -f "$DB_PATH" ]; then
        backup_database "memory.db.before_restore.$(date +%Y%m%d_%H%M%S)"
    fi

    # Restore from backup
    cp "$backup_file" "$DB_PATH"

    if [ $? -eq 0 ]; then
        print_success "Database restored from $backup_file"
        validate_database
        return $?
    else
        print_error "Database restore failed"
        return 1
    fi
}

# Export functions
export -f init_database
export -f backup_database
export -f reset_database
export -f validate_database
export -f check_database_size
export -f list_backups
export -f restore_database
