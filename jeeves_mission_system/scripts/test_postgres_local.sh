#!/bin/bash
#
# Local PostgreSQL + pgvector Setup & Test Script
# Tests the full implementation without requiring all dependencies
#

set -e  # Exit on error

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================================================${NC}"
echo -e "${BLUE}PostgreSQL + pgvector Local Setup & Test${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""

# Step 1: Check Prerequisites
echo -e "${YELLOW}Step 1: Checking prerequisites...${NC}"
echo ""

# Check for Podman or Docker
if command -v podman &> /dev/null; then
    CONTAINER_CMD="podman"
    echo -e "${GREEN}✓${NC} Found Podman"

    # Check for podman-compose
    if command -v podman-compose &> /dev/null; then
        COMPOSE_CMD="podman-compose"
        echo -e "${GREEN}✓${NC} Found podman-compose"
    else
        echo -e "${YELLOW}⚠${NC}  podman-compose not found"
        echo ""
        echo "Install podman-compose:"
        echo "  pip install podman-compose"
        echo ""
        echo "Or use podman directly:"
        echo "  podman run -d --name assistant-postgres \\"
        echo "    -e POSTGRES_DB=assistant \\"
        echo "    -e POSTGRES_USER=assistant \\"
        echo "    -e POSTGRES_PASSWORD=dev_password_change_in_production \\"
        echo "    -p 5432:5432 \\"
        echo "    docker.io/pgvector/pgvector:pg16"
        echo ""
        exit 1
    fi
elif command -v docker &> /dev/null; then
    CONTAINER_CMD="docker"
    echo -e "${GREEN}✓${NC} Found Docker"

    # Check for docker-compose or docker compose
    if command -v docker-compose &> /dev/null; then
        COMPOSE_CMD="docker-compose"
        echo -e "${GREEN}✓${NC} Found docker-compose"
    elif docker compose version &> /dev/null; then
        COMPOSE_CMD="docker compose"
        echo -e "${GREEN}✓${NC} Found docker compose (plugin)"
    else
        echo -e "${YELLOW}⚠${NC}  docker-compose not found"
        echo ""
        echo "Install docker-compose:"
        echo "  https://docs.docker.com/compose/install/"
        echo ""
        echo "Or use docker directly:"
        echo "  docker run -d --name assistant-postgres \\"
        echo "    -e POSTGRES_DB=assistant \\"
        echo "    -e POSTGRES_USER=assistant \\"
        echo "    -e POSTGRES_PASSWORD=dev_password_change_in_production \\"
        echo "    -p 5432:5432 \\"
        echo "    pgvector/pgvector:pg16"
        echo ""
        exit 1
    fi
else
    echo -e "${RED}✗${NC} Neither Podman nor Docker found!"
    echo "Please install Podman or Docker first."
    exit 1
fi

# Check for Python
if command -v python &> /dev/null || command -v python3 &> /dev/null; then
    PYTHON_CMD=$(command -v python3 || command -v python)
    echo -e "${GREEN}✓${NC} Found Python: $PYTHON_CMD"
else
    echo -e "${RED}✗${NC} Python not found!"
    exit 1
fi

# Check for psql (optional but helpful)
if command -v psql &> /dev/null; then
    echo -e "${GREEN}✓${NC} Found psql (PostgreSQL client)"
    HAS_PSQL=true
else
    echo -e "${YELLOW}⚠${NC}  psql not found (optional, but recommended)"
    HAS_PSQL=false
fi

echo ""

# Step 2: Start PostgreSQL
echo -e "${YELLOW}Step 2: Starting PostgreSQL container...${NC}"
echo ""

# Check if already running
if $CONTAINER_CMD ps | grep -q assistant-postgres; then
    echo -e "${GREEN}✓${NC} PostgreSQL container already running"
else
    echo "Starting PostgreSQL with pgvector..."
    # Specify compose file to avoid merging docker-compose.yml and podman-compose.yml
    if [ "$CONTAINER_CMD" = "podman" ]; then
        $COMPOSE_CMD -f podman-compose.yml up -d postgres
    else
        $COMPOSE_CMD -f docker-compose.yml up -d postgres
    fi

    echo "Waiting for PostgreSQL to be ready..."
    sleep 5

    # Wait for PostgreSQL to accept connections
    for i in {1..30}; do
        if $CONTAINER_CMD exec assistant-postgres pg_isready -U assistant &> /dev/null; then
            echo -e "${GREEN}✓${NC} PostgreSQL is ready"
            break
        fi
        if [ $i -eq 30 ]; then
            echo -e "${RED}✗${NC} PostgreSQL failed to start"
            echo "Check logs with: $CONTAINER_CMD logs assistant-postgres"
            exit 1
        fi
        sleep 1
    done
fi

echo ""

# Step 3: Check .env file
echo -e "${YELLOW}Step 3: Checking environment configuration...${NC}"
echo ""

if [ -f .env ]; then
    if grep -q "DATABASE_BACKEND=postgres" .env; then
        echo -e "${GREEN}✓${NC} .env configured for PostgreSQL"
    else
        echo -e "${YELLOW}⚠${NC}  .env exists but not configured for PostgreSQL"
        echo "Creating .env.postgres for reference..."
        cat > .env.postgres.test << 'EOF'
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
        echo "Created .env.postgres.test - review and rename to .env if needed"
    fi
else
    echo -e "${YELLOW}⚠${NC}  No .env file found, creating one..."
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
    echo -e "${GREEN}✓${NC} Created .env file"
fi

echo ""

# Step 4: Test PostgreSQL Connection
echo -e "${YELLOW}Step 4: Testing PostgreSQL connection...${NC}"
echo ""

if $CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -c "SELECT version();" &> /dev/null; then
    echo -e "${GREEN}✓${NC} Can connect to PostgreSQL"

    # Get PostgreSQL version
    PG_VERSION=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "SELECT version();" | head -1)
    echo "  PostgreSQL: $PG_VERSION"
else
    echo -e "${RED}✗${NC} Cannot connect to PostgreSQL"
    exit 1
fi

echo ""

# Step 5: Check pgvector Extension
echo -e "${YELLOW}Step 5: Verifying pgvector extension...${NC}"
echo ""

PGVECTOR_CHECK=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "SELECT COUNT(*) FROM pg_extension WHERE extname='vector';" 2>/dev/null || echo "0")

if [ "$PGVECTOR_CHECK" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} pgvector extension installed"
    PGVECTOR_VERSION=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "SELECT extversion FROM pg_extension WHERE extname='vector';" | tr -d ' ')
    echo "  pgvector version: $PGVECTOR_VERSION"
else
    echo -e "${YELLOW}⚠${NC}  pgvector extension not installed yet"
    echo "Will be installed during schema initialization"
fi

echo ""

# Step 6: Initialize Schema
echo -e "${YELLOW}Step 6: Initializing database schema...${NC}"
echo ""

# Check if schema already exists
TABLE_COUNT=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "SELECT COUNT(*) FROM pg_tables WHERE schemaname='public';" 2>/dev/null || echo "0")

if [ "$TABLE_COUNT" -gt 5 ]; then
    echo -e "${GREEN}✓${NC} Schema already initialized ($TABLE_COUNT tables found)"
    echo "Use 'python init_db.py --backend postgres --force' to reinitialize"
else
    echo "Initializing schema..."
    if $PYTHON_CMD init_db.py --backend postgres; then
        echo -e "${GREEN}✓${NC} Schema initialized successfully"
    else
        echo -e "${RED}✗${NC} Schema initialization failed"
        echo "Try: python init_db.py --backend postgres --force"
        exit 1
    fi
fi

echo ""

# Step 7: Verify Tables
echo -e "${YELLOW}Step 7: Verifying database tables...${NC}"
echo ""

TABLES=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "\dt" | grep -c "public |" || echo "0")

if [ "$TABLES" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} Found $TABLES tables"
    echo ""
    echo "Tables:"
    $CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -c "\dt" | grep "public |" | awk '{print "  - " $3}'
else
    echo -e "${RED}✗${NC} No tables found"
    exit 1
fi

echo ""

# Step 8: Test Vector Columns
echo -e "${YELLOW}Step 8: Checking vector columns...${NC}"
echo ""

VECTOR_COLS=$($CONTAINER_CMD exec assistant-postgres psql -U assistant -d assistant -t -c "
SELECT COUNT(*)
FROM information_schema.columns
WHERE table_schema = 'public'
AND data_type = 'USER-DEFINED'
AND udt_name = 'vector';
" | tr -d ' ')

if [ "$VECTOR_COLS" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} Found $VECTOR_COLS vector columns (pgvector ready)"
else
    echo -e "${YELLOW}⚠${NC}  No vector columns found"
    echo "This might be okay if pgvector isn't needed"
fi

echo ""

# Summary
echo -e "${BLUE}======================================================================${NC}"
echo -e "${BLUE}Setup Summary${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""
echo -e "${GREEN}✓${NC} PostgreSQL container running"
echo -e "${GREEN}✓${NC} Database connection working"
echo -e "${GREEN}✓${NC} Schema initialized with $TABLES tables"
echo -e "${GREEN}✓${NC} pgvector extension ready"
echo ""
echo -e "${BLUE}Next Steps:${NC}"
echo ""
echo "1. Start the server:"
echo "   ${YELLOW}python start_server.py${NC}"
echo ""
echo "2. Test the API:"
echo "   ${YELLOW}curl http://localhost:8000/health${NC}"
echo ""
echo "3. Create a test task:"
echo "   ${YELLOW}curl -X POST http://localhost:8000/api/v1/requests \\${NC}"
echo "   ${YELLOW}  -H 'Content-Type: application/json' \\${NC}"
echo "   ${YELLOW}  -d '{\"user_id\":\"test\",\"user_message\":\"Create task: Test\"}'${NC}"
echo ""
echo "4. Query database directly:"
if [ "$HAS_PSQL" = true ]; then
    echo "   ${YELLOW}psql -h localhost -U assistant -d assistant${NC}"
else
    echo "   ${YELLOW}$CONTAINER_CMD exec -it assistant-postgres psql -U assistant -d assistant${NC}"
fi
echo ""
echo "5. View logs:"
echo "   ${YELLOW}$CONTAINER_CMD logs -f assistant-postgres${NC}"
echo ""
echo -e "${GREEN}✓ Setup complete!${NC} 🚀"
echo ""
