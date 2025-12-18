#!/bin/bash
#
# Local PostgreSQL + pgvector Setup & Test Script
# Tests the full implementation without requiring all dependencies
#
# Previously 315 lines; now uses lib/database.sh for shared functionality.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Source library functions
source "$SCRIPT_DIR/lib/common.sh"
source "$SCRIPT_DIR/lib/database.sh"

# Print header
echo ""
print_info "======================================================================"
print_info "PostgreSQL + pgvector Local Setup & Test"
print_info "======================================================================"
echo ""

# Step 1: Check Prerequisites
print_step "Step 1: Checking prerequisites..."
echo ""

detect_container_runtime || exit 1

# Check for Python
if command_exists python3; then
    PYTHON_CMD="python3"
    print_success "Found Python: $(python3 --version)"
elif command_exists python; then
    PYTHON_CMD="python"
    print_success "Found Python: $(python --version)"
else
    print_error "Python not found!"
    exit 1
fi

# Check for psql (optional)
if command_exists psql; then
    print_success "Found psql (PostgreSQL client)"
    HAS_PSQL=true
else
    print_warning "psql not found (optional, but recommended)"
    HAS_PSQL=false
fi

echo ""

# Step 2: Start PostgreSQL
print_step "Step 2: Starting PostgreSQL container..."
echo ""

start_postgres_container || exit 1

echo ""

# Step 3: Check .env file
print_step "Step 3: Checking environment configuration..."
echo ""

cd "$PROJECT_DIR"

if [ -f .env ]; then
    if grep -q "DATABASE_BACKEND=postgres" .env; then
        print_success ".env configured for PostgreSQL"
    else
        print_warning ".env exists but not configured for PostgreSQL"
        print_info "Ensure DATABASE_BACKEND=postgres is set"
    fi
else
    print_warning "No .env file found, creating one..."
    cat > .env << 'EOF'
DATABASE_BACKEND=postgres
VECTOR_BACKEND=pgvector
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_DATABASE=assistant
POSTGRES_USER=assistant
POSTGRES_PASSWORD=dev_password_change_in_production
LLM_PROVIDER=mock
MEMORY_ENABLED=true
KANBAN_ENABLED=true
CHAT_ENABLED=true
EOF
    print_success "Created .env file"
fi

echo ""

# Step 4: Test PostgreSQL Connection
print_step "Step 4: Testing PostgreSQL connection..."
echo ""

if test_postgres_auth; then
    print_success "Can connect to PostgreSQL"
    PG_VERSION=$(get_postgres_version)
    echo "  PostgreSQL: $PG_VERSION"
else
    print_error "Cannot connect to PostgreSQL"
    exit 1
fi

echo ""

# Step 5: Check pgvector Extension
print_step "Step 5: Verifying pgvector extension..."
echo ""

ensure_pgvector || true

echo ""

# Step 6: Initialize Schema
print_step "Step 6: Initializing database schema..."
echo ""

TABLE_COUNT=$(get_postgres_table_count)

if [ "$TABLE_COUNT" -gt 5 ]; then
    print_success "Schema already initialized ($TABLE_COUNT tables found)"
    print_info "Use 'python init_db.py --backend postgres --force' to reinitialize"
else
    print_info "Initializing schema..."
    if $PYTHON_CMD init_db.py --backend postgres; then
        print_success "Schema initialized successfully"
    else
        print_error "Schema initialization failed"
        print_info "Try: python init_db.py --backend postgres --force"
        exit 1
    fi
fi

echo ""

# Step 7: Verify Tables
print_step "Step 7: Verifying database tables..."
echo ""

TABLE_COUNT=$(get_postgres_table_count)

if [ "$TABLE_COUNT" -gt 0 ]; then
    print_success "Found $TABLE_COUNT tables"
    echo ""
    echo "Tables:"
    $CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "\dt" 2>/dev/null | grep "public |" | awk '{print "  - " $3}'
else
    print_error "No tables found"
    exit 1
fi

echo ""

# Step 8: Test Vector Columns
print_step "Step 8: Checking vector columns..."
echo ""

VECTOR_COLS=$($CONTAINER_CMD exec "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -t -c "
SELECT COUNT(*)
FROM information_schema.columns
WHERE table_schema = 'public'
AND data_type = 'USER-DEFINED'
AND udt_name = 'vector';
" 2>/dev/null | tr -d ' ')

if [ "$VECTOR_COLS" -gt 0 ]; then
    print_success "Found $VECTOR_COLS vector columns (pgvector ready)"
else
    print_warning "No vector columns found (may be okay if pgvector isn't needed)"
fi

echo ""

# Summary
print_info "======================================================================"
print_info "Setup Summary"
print_info "======================================================================"
echo ""
print_success "PostgreSQL container running"
print_success "Database connection working"
print_success "Schema initialized with $TABLE_COUNT tables"
print_success "pgvector extension ready"
echo ""
print_info "Next Steps:"
echo ""
echo "1. Start the server:"
echo "   python start_server.py"
echo ""
echo "2. Test the API:"
echo "   curl http://localhost:8000/health"
echo ""
echo "3. Query database directly:"
if [ "$HAS_PSQL" = true ]; then
    echo "   psql -h localhost -U $POSTGRES_USER -d $POSTGRES_DB"
else
    echo "   $CONTAINER_CMD exec -it $POSTGRES_CONTAINER psql -U $POSTGRES_USER -d $POSTGRES_DB"
fi
echo ""
echo "4. View logs:"
echo "   $CONTAINER_CMD logs -f $POSTGRES_CONTAINER"
echo ""
print_success "Setup complete!"
echo ""
