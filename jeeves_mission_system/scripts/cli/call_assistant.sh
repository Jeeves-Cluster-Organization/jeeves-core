#!/usr/bin/env bash
#
# Call the 7-Agent Personal Assistant from the command line.
#
# Usage:
#   ./call_assistant.sh -m "Hey, list all the tasks I have"
#   ./call_assistant.sh --message "Add a task: Review PR #42" --session-id my-session
#   ./call_assistant.sh -m "What's in my journal?" --user-id alice --verbose
#
# Options:
#   -m, --message       Message to send to the assistant (required)
#   -u, --user-id       User ID for the request (default: cli-user)
#   -s, --session-id    Session ID to maintain conversation context (optional)
#   -h, --host          API host (default: localhost)
#   -p, --port          API port (default: 8000)
#   -v, --verbose       Show detailed output
#   --help              Show this help message

set -euo pipefail

# Default values
MESSAGE=""
USER_ID="cli-user"
SESSION_ID=""
HOST="localhost"
PORT="8000"
VERBOSE=false

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Helper functions
print_success() {
    echo -e "${GREEN}$1${NC}"
}

print_error() {
    echo -e "${RED}$1${NC}" >&2
}

print_info() {
    echo -e "${CYAN}$1${NC}"
}

print_warning() {
    echo -e "${YELLOW}$1${NC}"
}

show_help() {
    cat << EOF
Call the 7-Agent Personal Assistant from the command line.

Usage:
  $0 -m "Hey, list all the tasks I have"
  $0 --message "Add a task: Review PR #42" --session-id my-session
  $0 -m "What's in my journal?" --user-id alice --verbose

Options:
  -m, --message       Message to send to the assistant (required)
  -u, --user-id       User ID for the request (default: cli-user)
  -s, --session-id    Session ID to maintain conversation context (optional)
  -h, --host          API host (default: localhost)
  -p, --port          API port (default: 8000)
  -v, --verbose       Show detailed output
  --help              Show this help message

Examples:
  ./call_assistant.sh -m "List all my tasks"
  ./call_assistant.sh -m "Add task: Fix bug" -s session-123
  ./call_assistant.sh -m "Show my journal" -u alice -v

Server setup:
  Make sure the server is running:
    uvicorn api.server:app --reload

  Or in mock mode for testing:
    MOCK_MODE=true uvicorn api.server:app --reload
EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -m|--message)
            MESSAGE="$2"
            shift 2
            ;;
        -u|--user-id)
            USER_ID="$2"
            shift 2
            ;;
        -s|--session-id)
            SESSION_ID="$2"
            shift 2
            ;;
        -h|--host)
            HOST="$2"
            shift 2
            ;;
        -p|--port)
            PORT="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --help)
            show_help
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            echo ""
            show_help
            exit 1
            ;;
    esac
done

# Validate required arguments
if [ -z "$MESSAGE" ]; then
    print_error "Error: Message is required"
    echo ""
    show_help
    exit 1
fi

# Build API URL
API_URL="http://${HOST}:${PORT}/api/v1/requests"
HEALTH_URL="http://${HOST}:${PORT}/health"

if [ "$VERBOSE" = true ]; then
    print_info "API URL: $API_URL"
    print_info "User ID: $USER_ID"
    print_info "Message: $MESSAGE"
    [ -n "$SESSION_ID" ] && print_info "Session ID: $SESSION_ID"
    echo ""
fi

# Build request body
if [ -n "$SESSION_ID" ]; then
    REQUEST_BODY=$(cat <<EOF
{
  "user_message": $(echo "$MESSAGE" | jq -Rs .),
  "user_id": "$USER_ID",
  "session_id": "$SESSION_ID"
}
EOF
)
else
    REQUEST_BODY=$(cat <<EOF
{
  "user_message": $(echo "$MESSAGE" | jq -Rs .),
  "user_id": "$USER_ID"
}
EOF
)
fi

if [ "$VERBOSE" = true ]; then
    print_info "Request body:"
    echo "$REQUEST_BODY" | jq .
    echo ""
fi

# Check if jq is installed
if ! command -v jq &> /dev/null; then
    print_error "Error: jq is required but not installed"
    print_info "Install with: sudo apt-get install jq (Ubuntu/Debian) or brew install jq (macOS)"
    exit 1
fi

# Check if server is healthy
if [ "$VERBOSE" = true ]; then
    print_info "Checking server health..."
fi

if ! HEALTH_RESPONSE=$(curl -s -f "$HEALTH_URL" 2>&1); then
    print_error "Error: Cannot connect to assistant server at http://${HOST}:${PORT}"
    print_error "Make sure the server is running with: uvicorn api.server:app --reload"
    echo ""
    print_error "Error details: $HEALTH_RESPONSE"
    exit 1
fi

if [ "$VERBOSE" = true ]; then
    print_success "Server is healthy"
    echo ""
fi

# Send request to assistant
print_info "Sending message to assistant..."
echo ""

RESPONSE=$(curl -s -X POST "$API_URL" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_BODY" \
    --max-time 30)

CURL_EXIT_CODE=$?

if [ $CURL_EXIT_CODE -ne 0 ]; then
    print_error "Error: Failed to communicate with assistant (curl exit code: $CURL_EXIT_CODE)"
    exit 1
fi

if [ "$VERBOSE" = true ]; then
    print_info "Response received:"
    echo "$RESPONSE" | jq .
    echo ""
fi

# Extract response fields
RESPONSE_TEXT=$(echo "$RESPONSE" | jq -r '.response_text // empty')
STATUS=$(echo "$RESPONSE" | jq -r '.status // "unknown"')
REQUEST_ID=$(echo "$RESPONSE" | jq -r '.request_id // "unknown"')
CLARIFICATION_NEEDED=$(echo "$RESPONSE" | jq -r '.clarification_needed // false')
CLARIFICATION_QUESTION=$(echo "$RESPONSE" | jq -r '.clarification_question // empty')

# Display response
print_success "=== Assistant Response ==="
echo ""

if [ -n "$RESPONSE_TEXT" ]; then
    echo "$RESPONSE_TEXT"
else
    print_warning "No response text received"
fi

echo ""

# Display status and metadata
if [ "$VERBOSE" = true ]; then
    print_info "Status: $STATUS"
    print_info "Request ID: $REQUEST_ID"
    echo ""
fi

# Handle clarification if needed
if [ "$CLARIFICATION_NEEDED" = "true" ] && [ -n "$CLARIFICATION_QUESTION" ]; then
    echo ""
    print_warning "=== Clarification Needed ==="
    echo "$CLARIFICATION_QUESTION"
    echo ""
    print_info "Hint: Use --session-id '$REQUEST_ID' to continue this conversation"
fi
