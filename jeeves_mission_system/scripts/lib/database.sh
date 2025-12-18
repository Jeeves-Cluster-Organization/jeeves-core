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

# =============================================================================
# Docker Detection
# =============================================================================

# Detect Docker and docker compose
detect_container_runtime() {
    if ! command_exists docker; then
        print_error "Docker not found. Please install Docker."
        return 1
    fi

    CONTAINER_CMD="docker"

    if command_exists docker-compose; then
        COMPOSE_CMD="docker-compose"
    elif docker compose version &> /dev/null 2>&1; then
        COMPOSE_CMD="docker compose"
    else
        COMPOSE_CMD=""
    fi

    COMPOSE_FILE="docker-compose.yml"

    print_success "Docker found"
    if [ -n "$COMPOSE_CMD" ]; then
        print_success "Compose: $COMPOSE_CMD"
    else
        print_warning "docker compose not found, will use docker directly"
    fi
    return 0
}

# =============================================================================
# PostgreSQL Container Functions
# =============================================================================

# PostgreSQL defaults
POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-assistant-postgres}"
POSTGRES_IMAGE="${POSTGRES_IMAGE:-pgvector/pgvector:pg16}"
POSTGRES_DB="${POSTGRES_DB:-assistant}"
POSTGRES_USER="${POSTGRES_USER:-assistant}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-dev_password_change_in_production}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"

# Check if PostgreSQL container is running
pg_container_running() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime
    $CONTAINER_CMD ps --format '{{.Names}}' 2>/dev/null | grep -q "^${POSTGRES_CONTAINER}$"
}

# Check if PostgreSQL container exists (running or stopped)
pg_container_exists() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime
    $CONTAINER_CMD ps -a --format '{{.Names}}' 2>/dev/null | grep -q "^${POSTGRES_CONTAINER}$"
}

# Wait for PostgreSQL to be ready
wait_for_postgres() {
    local max_attempts="${1:-30}"
    local attempt=0

    print_step "Waiting for PostgreSQL to be ready..."

    while [ $attempt -lt $max_attempts ]; do
        if $CONTAINER_CMD exec "$POSTGRES_CONTAINER" pg_isready -U "$POSTGRES_USER" &> /dev/null; then
            print_success "PostgreSQL is ready"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 1
    done

    print_error "PostgreSQL failed to become ready after ${max_attempts}s"
    return 1
}

# Test PostgreSQL authentication
test_postgres_auth() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime
    PGPASSWORD="$POSTGRES_PASSWORD" $CONTAINER_CMD exec "$POSTGRES_CONTAINER" \
        psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "SELECT 1" &> /dev/null
}

# Start PostgreSQL container
start_postgres_container() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime

    if pg_container_running; then
        print_success "PostgreSQL container already running"
        return 0
    fi

    if pg_container_exists; then
        print_info "Starting existing PostgreSQL container..."
        $CONTAINER_CMD start "$POSTGRES_CONTAINER"
    elif [ -n "$COMPOSE_CMD" ] && [ -f "$COMPOSE_FILE" ]; then
        print_info "Starting PostgreSQL via docker compose..."
        $COMPOSE_CMD -f "$COMPOSE_FILE" up -d postgres
    else
        print_info "Creating PostgreSQL container..."
        $CONTAINER_CMD run -d \
            --name "$POSTGRES_CONTAINER" \
            -e POSTGRES_DB="$POSTGRES_DB" \
            -e POSTGRES_USER="$POSTGRES_USER" \
            -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
            -p "$POSTGRES_PORT":5432 \
            "$POSTGRES_IMAGE"
    fi

    wait_for_postgres
}

# Stop PostgreSQL container
stop_postgres_container() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime

    if pg_container_running; then
        print_step "Stopping PostgreSQL container..."
        $CONTAINER_CMD stop "$POSTGRES_CONTAINER"
        print_success "PostgreSQL container stopped"
    else
        print_info "PostgreSQL container not running"
    fi
}

# Get PostgreSQL version
get_postgres_version() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime
    $CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
        -t -c "SELECT version();" 2>/dev/null | head -1 | xargs
}

# Check/install pgvector extension
ensure_pgvector() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime

    # Check if available
    local available=$($CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
        -t -c "SELECT COUNT(*) FROM pg_available_extensions WHERE name='vector';" 2>/dev/null | xargs)

    if [ "$available" -gt 0 ]; then
        # Install if not already installed
        $CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
            -c "CREATE EXTENSION IF NOT EXISTS vector;" &> /dev/null

        local version=$($CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
            -t -c "SELECT extversion FROM pg_extension WHERE extname='vector';" 2>/dev/null | xargs)

        if [ -n "$version" ]; then
            print_success "pgvector extension installed: v$version"
            return 0
        fi
    fi

    print_warning "pgvector extension not available"
    return 1
}

# Get PostgreSQL table count
get_postgres_table_count() {
    [ -z "$CONTAINER_CMD" ] && detect_container_runtime
    $CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
        -t -c "SELECT COUNT(*) FROM pg_tables WHERE schemaname='public';" 2>/dev/null | xargs
}

# Print PostgreSQL connection info
print_postgres_connection_info() {
    echo ""
    print_info "PostgreSQL Connection Details:"
    echo "  Host:     localhost"
    echo "  Port:     $POSTGRES_PORT"
    echo "  Database: $POSTGRES_DB"
    echo "  User:     $POSTGRES_USER"
    echo ""
    echo "  Connection string:"
    echo "    postgresql://$POSTGRES_USER:$POSTGRES_PASSWORD@localhost:$POSTGRES_PORT/$POSTGRES_DB"
    echo ""
    echo "  Connect via container:"
    echo "    $CONTAINER_CMD exec -it $POSTGRES_CONTAINER psql -U $POSTGRES_USER -d $POSTGRES_DB"
    echo ""
}

# Export functions
export -f init_database
export -f backup_database
export -f reset_database
export -f validate_database
export -f check_database_size
export -f list_backups
export -f restore_database
export -f detect_container_runtime
export -f pg_container_running
export -f pg_container_exists
export -f wait_for_postgres
export -f test_postgres_auth
export -f start_postgres_container
export -f stop_postgres_container
export -f get_postgres_version
export -f ensure_pgvector
export -f get_postgres_table_count
export -f print_postgres_connection_info
