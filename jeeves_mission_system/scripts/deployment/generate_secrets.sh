#!/bin/bash
# Generate Secure Secrets for Production Deployment
# Creates strong random passwords and tokens for .env file
#
# Usage: ./scripts/deployment/generate_secrets.sh [--output FILE]
#
# Default output: .env.generated

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_FILE="$PROJECT_ROOT/.env.generated"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --output)
            OUTPUT_FILE="$2"
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
    echo -e "${BLUE}  Generate Production Secrets${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""
}

print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_ok() {
    echo -e "${GREEN}[OK]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Check if openssl is available
check_openssl() {
    if ! command -v openssl &> /dev/null; then
        echo "Error: openssl not found"
        echo "Please install openssl to generate secure random passwords"
        exit 1
    fi
}

# Generate secure password (base64, 64 chars)
generate_password() {
    openssl rand -base64 48 | tr -d '=' | tr '+/' '-_'
}

# Generate hex token (64 chars)
generate_token() {
    openssl rand -hex 32
}

# Create .env file with generated secrets
create_env_file() {
    print_info "Generating secrets..."

    local POSTGRES_PW=$(generate_password)
    local WEBSOCKET_TOKEN=$(generate_token)

    print_ok "Generated PostgreSQL password (${#POSTGRES_PW} chars)"
    print_ok "Generated WebSocket auth token (${#WEBSOCKET_TOKEN} chars)"

    # Copy example file as template if it exists
    if [[ -f "$PROJECT_ROOT/.env.production.example" ]]; then
        cp "$PROJECT_ROOT/.env.production.example" "$OUTPUT_FILE"
        print_info "Using .env.production.example as template"
    else
        print_warning ".env.production.example not found, creating minimal .env"
        cat > "$OUTPUT_FILE" << EOF
# Auto-generated production environment configuration
# Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")

# Database
DATABASE_BACKEND=postgres
VECTOR_BACKEND=pgvector

# PostgreSQL
POSTGRES_HOST=postgres
POSTGRES_PORT=5432
POSTGRES_DATABASE=assistant
POSTGRES_USER=assistant
POSTGRES_PASSWORD=$POSTGRES_PW

# LLM Provider
LLM_PROVIDER=ollama
OLLAMA_HOST=http://ollama:11434
DEFAULT_MODEL=qwen2.5:7b-instruct

# API
API_HOST=0.0.0.0
API_PORT=8000
API_RELOAD=false

# CORS (CHANGE THIS to your actual domain!)
CORS_ORIGINS=https://yourdomain.com

# WebSocket
WEBSOCKET_AUTH_REQUIRED=true
WEBSOCKET_AUTH_TOKEN=$WEBSOCKET_TOKEN

# Security
ENABLE_CONFIRMATIONS=true
LOG_LEVEL=INFO
LOG_FORMAT=json

# Features
KANBAN_ENABLED=true
CHAT_ENABLED=true
MEMORY_ENABLED=true
META_VALIDATION_ENABLED=true

# Deployment
NODE_ID=node-01
NODE_NAME=assistant-primary
DEPLOYMENT_MODE=single_node
EOF
    fi

    # Replace placeholder passwords in template
    if [[ -f "$PROJECT_ROOT/.env.production.example" ]]; then
        sed -i.bak "s|POSTGRES_PASSWORD=.*|POSTGRES_PASSWORD=$POSTGRES_PW|" "$OUTPUT_FILE"
        sed -i.bak "s|WEBSOCKET_AUTH_TOKEN=.*|WEBSOCKET_AUTH_TOKEN=$WEBSOCKET_TOKEN|" "$OUTPUT_FILE"
        rm -f "$OUTPUT_FILE.bak"
    fi

    print_ok "Secrets file created: $OUTPUT_FILE"
}

# Print next steps
print_next_steps() {
    echo ""
    echo -e "${BLUE}=================================================${NC}"
    echo -e "${BLUE}  Next Steps${NC}"
    echo -e "${BLUE}=================================================${NC}"
    echo ""
    echo "1. Review the generated secrets:"
    echo "   cat $OUTPUT_FILE"
    echo ""
    echo "2. Update CORS_ORIGINS with your actual domain"
    echo ""
    echo "3. Rename to .env (backup existing .env first!):"
    echo "   mv .env .env.backup  # If .env exists"
    echo "   mv $OUTPUT_FILE .env"
    echo ""
    echo "4. Start deployment:"
    echo "   docker compose up -d"
    echo ""
    echo -e "${YELLOW}⚠️  IMPORTANT:${NC}"
    echo "- Never commit .env to version control"
    echo "- Store backup of .env in secure location (password manager)"
    echo "- Rotate secrets regularly"
    echo ""
}

# Main execution
main() {
    print_header

    # Check prerequisites
    check_openssl

    # Create secrets file
    create_env_file

    # Print next steps
    print_next_steps

    echo -e "${GREEN}✅ Secrets generated successfully!${NC}"
    echo ""
}

# Run main
main
