#!/bin/bash
#
# PostgreSQL Volume Reset Script
# Handles credential mismatch issues when the PostgreSQL volume was created with different credentials
#
# Usage:
#   ./scripts/reset_postgres_volume.sh          # Interactive mode
#   ./scripts/reset_postgres_volume.sh --force  # Force reset without prompting
#

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================================================${NC}"
echo -e "${BLUE}PostgreSQL Volume Reset Tool${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""

# Parse arguments
FORCE=false
if [ "$1" == "--force" ] || [ "$1" == "-f" ]; then
    FORCE=true
fi

# Detect container runtime
if command -v podman &> /dev/null; then
    CONTAINER_CMD="podman"
    COMPOSE_CMD="podman-compose"
    COMPOSE_FILE="podman-compose.yml"
    VOLUME_NAME="assistant-7agent_postgres-data"
    echo -e "${GREEN}[OK]${NC} Using Podman"
elif command -v docker &> /dev/null; then
    CONTAINER_CMD="docker"
    if docker compose version &> /dev/null 2>&1; then
        COMPOSE_CMD="docker compose"
    else
        COMPOSE_CMD="docker-compose"
    fi
    COMPOSE_FILE="docker-compose.yml"
    VOLUME_NAME="assistant-7agent_postgres-data"
    echo -e "${GREEN}[OK]${NC} Using Docker"
else
    echo -e "${RED}[ERR]${NC} Neither Podman nor Docker found!"
    exit 1
fi

CONTAINER_NAME="assistant-postgres"

echo ""
echo -e "${YELLOW}This script will:${NC}"
echo "  1. Stop the PostgreSQL container"
echo "  2. Remove the PostgreSQL container"
echo "  3. Delete the PostgreSQL data volume (ALL DATA WILL BE LOST)"
echo "  4. Recreate the container with current credentials from .env"
echo ""
echo -e "${RED}WARNING: This will DELETE ALL DATABASE DATA!${NC}"
echo ""

# Check if volume exists
if ! $CONTAINER_CMD volume inspect "$VOLUME_NAME" &> /dev/null 2>&1; then
    echo -e "${YELLOW}[INFO]${NC} Volume $VOLUME_NAME does not exist."
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
echo -e "${YELLOW}Step 1: Stopping PostgreSQL container...${NC}"
if $CONTAINER_CMD ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    $CONTAINER_CMD stop "$CONTAINER_NAME"
    echo -e "${GREEN}[OK]${NC} Container stopped"
else
    echo -e "${YELLOW}[INFO]${NC} Container not running"
fi

echo ""
echo -e "${YELLOW}Step 2: Removing PostgreSQL container...${NC}"
if $CONTAINER_CMD ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    $CONTAINER_CMD rm -f "$CONTAINER_NAME"
    echo -e "${GREEN}[OK]${NC} Container removed"
else
    echo -e "${YELLOW}[INFO]${NC} Container does not exist"
fi

echo ""
echo -e "${YELLOW}Step 3: Deleting PostgreSQL data volume...${NC}"
if $CONTAINER_CMD volume inspect "$VOLUME_NAME" &> /dev/null 2>&1; then
    $CONTAINER_CMD volume rm "$VOLUME_NAME"
    echo -e "${GREEN}[OK]${NC} Volume deleted"
else
    echo -e "${YELLOW}[INFO]${NC} Volume does not exist"
fi

echo ""
echo -e "${YELLOW}Step 4: Recreating PostgreSQL container...${NC}"

# Load environment variables from .env if it exists
if [ -f ".env" ]; then
    set -a
    source .env
    set +a
    echo -e "${GREEN}[OK]${NC} Loaded credentials from .env"
fi

# Use compose to recreate
if command -v $COMPOSE_CMD &> /dev/null; then
    echo "Using $COMPOSE_CMD to start PostgreSQL..."
    if [ "$CONTAINER_CMD" = "podman" ]; then
        $COMPOSE_CMD -f "$COMPOSE_FILE" up -d postgres
    else
        $COMPOSE_CMD -f "$COMPOSE_FILE" up -d postgres
    fi
else
    echo -e "${YELLOW}[INFO]${NC} Compose not available, using direct container run..."

    POSTGRES_DB="${POSTGRES_DATABASE:-assistant}"
    POSTGRES_USER="${POSTGRES_USER:-assistant}"
    POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-dev_password_change_in_production}"
    POSTGRES_PORT="${POSTGRES_PORT:-5432}"

    $CONTAINER_CMD run -d \
        --name "$CONTAINER_NAME" \
        -e POSTGRES_DB="$POSTGRES_DB" \
        -e POSTGRES_USER="$POSTGRES_USER" \
        -e POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
        -e "POSTGRES_INITDB_ARGS=-E UTF8 --locale=en_US.UTF-8" \
        -v "$VOLUME_NAME:/var/lib/postgresql/data:Z" \
        -v "$(pwd)/database/schemas:/docker-entrypoint-initdb.d:ro,Z" \
        -p "$POSTGRES_PORT:5432" \
        --restart unless-stopped \
        docker.io/pgvector/pgvector:pg16
fi

echo ""
echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
for i in {1..30}; do
    if $CONTAINER_CMD exec "$CONTAINER_NAME" pg_isready -U "${POSTGRES_USER:-assistant}" &> /dev/null 2>&1; then
        echo -e "${GREEN}[OK]${NC} PostgreSQL is ready"
        break
    fi
    if [ $i -eq 30 ]; then
        echo -e "${RED}[ERR]${NC} PostgreSQL failed to start in time"
        echo "Check logs with: $CONTAINER_CMD logs $CONTAINER_NAME"
        exit 1
    fi
    sleep 1
    echo -n "."
done

echo ""
echo -e "${BLUE}======================================================================${NC}"
echo -e "${GREEN}[OK] PostgreSQL volume reset complete!${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""
echo "The PostgreSQL container has been recreated with fresh credentials."
echo "You can now initialize the database schema by running:"
echo ""
echo "  python init_db.py --backend postgres"
echo ""
echo "Or use the full setup script:"
echo ""
echo "  ./scripts/setup_postgres.sh"
echo ""
