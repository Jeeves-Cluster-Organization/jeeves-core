#!/bin/bash
# Database backup script for 7-Agent Assistant
# Uses SQLite's .backup command for safe, atomic backups

set -e  # Exit on error

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BACKUP_DIR="${BACKUP_DIR:-${HOME}/.local/share/assistant-7agent/backups}"
DB_PATH="${DATABASE_PATH:-${PROJECT_ROOT}/assistant.db}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-7}"
DATE_STAMP="$(date +%Y%m%d-%H%M%S)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Ensure backup directory exists
mkdir -p "$BACKUP_DIR"

# Check if database exists
if [ ! -f "$DB_PATH" ]; then
    log_error "Database not found: $DB_PATH"
    exit 1
fi

log_info "Starting database backup..."
log_info "Database: $DB_PATH"
log_info "Backup directory: $BACKUP_DIR"

# Backup filename
BACKUP_FILE="${BACKUP_DIR}/assistant-${DATE_STAMP}.db"

# Perform backup using SQLite's .backup command (safer than cp)
# This ensures a consistent snapshot even if writes are happening
sqlite3 "$DB_PATH" ".backup '${BACKUP_FILE}'"

if [ $? -eq 0 ]; then
    log_info "Backup created: $BACKUP_FILE"

    # Get backup file size
    BACKUP_SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    log_info "Backup size: $BACKUP_SIZE"

    # Verify backup integrity
    log_info "Verifying backup integrity..."
    INTEGRITY_CHECK=$(sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" 2>&1)

    if [ "$INTEGRITY_CHECK" = "ok" ]; then
        log_info "✓ Backup integrity verified"
    else
        log_error "Backup integrity check failed: $INTEGRITY_CHECK"
        rm -f "$BACKUP_FILE"
        exit 1
    fi

    # Cleanup old backups
    log_info "Cleaning up backups older than $RETENTION_DAYS days..."
    find "$BACKUP_DIR" -name "assistant-*.db" -type f -mtime +${RETENTION_DAYS} -delete 2>/dev/null || true

    # List remaining backups
    BACKUP_COUNT=$(find "$BACKUP_DIR" -name "assistant-*.db" -type f | wc -l)
    log_info "Total backups retained: $BACKUP_COUNT"

    log_info "✓ Backup completed successfully"
else
    log_error "Backup failed"
    exit 1
fi
