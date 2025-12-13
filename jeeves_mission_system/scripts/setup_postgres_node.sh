#!/bin/bash
#
# PostgreSQL + pgvector Container Setup Script
# Sets up only the PostgreSQL node/container
#

set -e  # Exit on error

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================================================${NC}"
echo -e "${BLUE}PostgreSQL + pgvector Node Setup${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""

# Check for Podman or Docker
if command -v podman &> /dev/null; then
    CONTAINER_CMD="podman"
    echo -e "${GREEN}✓${NC} Found Podman"

    # Check for podman-compose
    if command -v podman-compose &> /dev/null; then
        COMPOSE_CMD="podman-compose"
        USE_COMPOSE=true
        echo -e "${GREEN}✓${NC} Found podman-compose"
    else
        echo -e "${YELLOW}⚠${NC}  podman-compose not found, using podman directly"
        USE_COMPOSE=false
    fi
elif command -v docker &> /dev/null; then
    CONTAINER_CMD="docker"
    echo -e "${GREEN}✓${NC} Found Docker"

    # Check for docker-compose or docker compose
    if command -v docker-compose &> /dev/null; then
        COMPOSE_CMD="docker-compose"
        USE_COMPOSE=true
        echo -e "${GREEN}✓${NC} Found docker-compose"
    elif docker compose version &> /dev/null; then
        COMPOSE_CMD="docker compose"
        USE_COMPOSE=true
        echo -e "${GREEN}✓${NC} Found docker compose (plugin)"
    else
        echo -e "${YELLOW}⚠${NC}  docker-compose not found, using docker directly"
        USE_COMPOSE=false
    fi
else
    echo -e "${RED}✗${NC} Neither Podman nor Docker found!"
    echo "Please install Podman or Docker first."
    exit 1
fi

echo ""

# Configuration
CONTAINER_NAME="assistant-postgres"
POSTGRES_IMAGE="docker.io/pgvector/pgvector:pg16"
POSTGRES_DB="${POSTGRES_DB:-assistant}"
POSTGRES_USER="${POSTGRES_USER:-assistant}"
POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-dev_password_change_in_production}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"

echo -e "${YELLOW}Configuration:${NC}"
echo "  Database: $POSTGRES_DB"
echo "  User: $POSTGRES_USER"
echo "  Port: $POSTGRES_PORT"
echo ""

# Function to test PostgreSQL authentication
test_postgres_auth() {
    local container=$1
    local user=$2
    local db=$3
    local password=$4

    # Try to connect and run a simple query
    PGPASSWORD="$password" $CONTAINER_CMD exec "$container" \
        psql -U "$user" -d "$db" -c "SELECT 1" &> /dev/null
    return $?
}

# Function to reset PostgreSQL volume
reset_postgres_volume() {
    echo ""
    echo -e "${YELLOW}Resetting PostgreSQL volume due to credential mismatch...${NC}"

    # Stop and remove container
    $CONTAINER_CMD stop "$CONTAINER_NAME" &> /dev/null || true
    $CONTAINER_CMD rm -f "$CONTAINER_NAME" &> /dev/null || true

    # Remove volume
    local volume_name
    if [ "$CONTAINER_CMD" = "podman" ]; then
        volume_name="assistant-7agent_postgres-data"
    else
        volume_name="assistant-7agent_postgres-data"
    fi

    $CONTAINER_CMD volume rm "$volume_name" &> /dev/null || true
    echo -e "${GREEN}✓${NC} Volume reset complete"
}

# Track if we need to create a new container
NEED_NEW_CONTAINER=false

# Check if already running
if $CONTAINER_CMD ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    if $CONTAINER_CMD ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo -e "${GREEN}✓${NC} PostgreSQL container already running"

        # Verify it's accessible
        if $CONTAINER_CMD exec $CONTAINER_NAME pg_isready -U $POSTGRES_USER &> /dev/null; then
            echo -e "${GREEN}✓${NC} PostgreSQL is accepting connections"

            # Test authentication with current credentials
            echo -e "${YELLOW}Testing authentication...${NC}"
            if test_postgres_auth "$CONTAINER_NAME" "$POSTGRES_USER" "$POSTGRES_DB" "$POSTGRES_PASSWORD"; then
                echo -e "${GREEN}✓${NC} Authentication successful"
            else
                echo -e "${RED}✗${NC} Authentication failed with current credentials!"
                echo ""
                echo -e "${YELLOW}This usually happens when the PostgreSQL volume was created with different credentials.${NC}"
                echo "PostgreSQL only sets credentials on first initialization."
                echo ""
                echo "Options:"
                echo "  1) Reset the volume (DELETES ALL DATA)"
                echo "  2) Manually update credentials in the running container"
                echo "  3) Exit and investigate"
                echo ""
                read -p "Choose option [1/2/3]: " -n 1 -r
                echo ""
                case $REPLY in
                    1)
                        reset_postgres_volume
                        NEED_NEW_CONTAINER=true
                        ;;
                    2)
                        echo ""
                        echo "To manually update the password, run:"
                        echo "  $CONTAINER_CMD exec -it $CONTAINER_NAME psql -U postgres -c \"ALTER USER $POSTGRES_USER WITH PASSWORD '$POSTGRES_PASSWORD';\""
                        echo ""
                        echo "Then re-run this script."
                        exit 0
                        ;;
                    *)
                        echo ""
                        echo "You can also use: ./scripts/reset_postgres_volume.sh --force"
                        exit 1
                        ;;
                esac
            fi
        else
            echo -e "${YELLOW}⚠${NC}  PostgreSQL is running but not ready yet, waiting..."
            for i in {1..30}; do
                if $CONTAINER_CMD exec $CONTAINER_NAME pg_isready -U $POSTGRES_USER &> /dev/null; then
                    echo -e "${GREEN}✓${NC} PostgreSQL is ready now"
                    break
                fi
                sleep 1
            done
        fi
    else
        echo -e "${YELLOW}⚠${NC}  PostgreSQL container exists but is not running, starting..."
        $CONTAINER_CMD start $CONTAINER_NAME

        echo "Waiting for PostgreSQL to be ready..."
        for i in {1..30}; do
            if $CONTAINER_CMD exec $CONTAINER_NAME pg_isready -U $POSTGRES_USER &> /dev/null; then
                echo -e "${GREEN}✓${NC} PostgreSQL is ready"
                break
            fi
            if [ $i -eq 30 ]; then
                echo -e "${RED}✗${NC} PostgreSQL failed to start"
                echo "Check logs with: $CONTAINER_CMD logs $CONTAINER_NAME"
                exit 1
            fi
            sleep 1
        done

        # Test authentication after starting
        echo -e "${YELLOW}Testing authentication...${NC}"
        if test_postgres_auth "$CONTAINER_NAME" "$POSTGRES_USER" "$POSTGRES_DB" "$POSTGRES_PASSWORD"; then
            echo -e "${GREEN}✓${NC} Authentication successful"
        else
            echo -e "${RED}✗${NC} Authentication failed!"
            echo ""
            echo "The volume has different credentials. Choose an option:"
            echo "  1) Reset the volume (DELETES ALL DATA)"
            echo "  2) Exit"
            echo ""
            read -p "Choose option [1/2]: " -n 1 -r
            echo ""
            case $REPLY in
                1)
                    reset_postgres_volume
                    NEED_NEW_CONTAINER=true
                    ;;
                *)
                    echo "Run: ./scripts/reset_postgres_volume.sh"
                    exit 1
                    ;;
            esac
        fi
    fi
else
    NEED_NEW_CONTAINER=true
fi

if [ "$NEED_NEW_CONTAINER" = true ]; then
    echo -e "${YELLOW}Starting PostgreSQL container...${NC}"

    if [ "$USE_COMPOSE" = true ]; then
        # Use compose - specify file to avoid merging docker-compose.yml and podman-compose.yml
        echo "Using $COMPOSE_CMD..."
        if [ "$CONTAINER_CMD" = "podman" ]; then
            $COMPOSE_CMD -f podman-compose.yml up -d postgres
        else
            $COMPOSE_CMD -f docker-compose.yml up -d postgres
        fi
    else
        # Use direct container command
        echo "Using $CONTAINER_CMD run..."

        if [ "$CONTAINER_CMD" = "podman" ]; then
            IMAGE="docker.io/pgvector/pgvector:pg16"
        else
            IMAGE="pgvector/pgvector:pg16"
        fi

        $CONTAINER_CMD run -d \
            --name $CONTAINER_NAME \
            -e POSTGRES_DB=$POSTGRES_DB \
            -e POSTGRES_USER=$POSTGRES_USER \
            -e POSTGRES_PASSWORD=$POSTGRES_PASSWORD \
            -p $POSTGRES_PORT:5432 \
            $IMAGE
    fi

    echo "Waiting for PostgreSQL to be ready..."
    sleep 3

    # Wait for PostgreSQL to accept connections
    for i in {1..30}; do
        if $CONTAINER_CMD exec $CONTAINER_NAME pg_isready -U $POSTGRES_USER &> /dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} PostgreSQL is ready"
            break
        fi
        if [ $i -eq 30 ]; then
            echo -e "${RED}✗${NC} PostgreSQL failed to start"
            echo "Check logs with: $CONTAINER_CMD logs $CONTAINER_NAME"
            exit 1
        fi
        sleep 1
    done
fi

echo ""

# Verify PostgreSQL version
echo -e "${YELLOW}Verifying PostgreSQL...${NC}"
PG_VERSION=$($CONTAINER_CMD exec $CONTAINER_NAME psql -U $POSTGRES_USER -d $POSTGRES_DB -t -c "SELECT version();" 2>/dev/null | head -1 | xargs)
if [ -n "$PG_VERSION" ]; then
    echo -e "${GREEN}✓${NC} PostgreSQL: $(echo $PG_VERSION | cut -d',' -f1)"
else
    echo -e "${RED}✗${NC} Could not get PostgreSQL version"
fi

# Check pgvector extension
PGVECTOR_CHECK=$($CONTAINER_CMD exec $CONTAINER_NAME psql -U $POSTGRES_USER -d $POSTGRES_DB -t -c "SELECT COUNT(*) FROM pg_available_extensions WHERE name='vector';" 2>/dev/null | xargs)
if [ "$PGVECTOR_CHECK" -gt 0 ]; then
    echo -e "${GREEN}✓${NC} pgvector extension available"

    # Try to install it
    $CONTAINER_CMD exec $CONTAINER_NAME psql -U $POSTGRES_USER -d $POSTGRES_DB -c "CREATE EXTENSION IF NOT EXISTS vector;" &> /dev/null || true

    # Check if installed
    INSTALLED=$($CONTAINER_CMD exec $CONTAINER_NAME psql -U $POSTGRES_USER -d $POSTGRES_DB -t -c "SELECT extversion FROM pg_extension WHERE extname='vector';" 2>/dev/null | xargs)
    if [ -n "$INSTALLED" ]; then
        echo -e "${GREEN}✓${NC} pgvector extension installed: version $INSTALLED"
    fi
else
    echo -e "${RED}✗${NC} pgvector extension not available"
fi

echo ""
echo -e "${BLUE}======================================================================${NC}"
echo -e "${GREEN}✓ PostgreSQL node setup complete!${NC}"
echo -e "${BLUE}======================================================================${NC}"
echo ""
echo "Connection details:"
echo "  Host: localhost"
echo "  Port: $POSTGRES_PORT"
echo "  Database: $POSTGRES_DB"
echo "  User: $POSTGRES_USER"
echo "  Password: $POSTGRES_PASSWORD"
echo ""
echo "Connection string:"
echo "  postgresql://$POSTGRES_USER:$POSTGRES_PASSWORD@localhost:$POSTGRES_PORT/$POSTGRES_DB"
echo ""
echo "Test connection:"
echo "  $CONTAINER_CMD exec -it $CONTAINER_NAME psql -U $POSTGRES_USER -d $POSTGRES_DB"
echo ""
echo "View logs:"
echo "  $CONTAINER_CMD logs -f $CONTAINER_NAME"
echo ""
echo "Stop container:"
echo "  $CONTAINER_CMD stop $CONTAINER_NAME"
echo ""
echo "Remove container:"
echo "  $CONTAINER_CMD rm -f $CONTAINER_NAME"
echo ""
