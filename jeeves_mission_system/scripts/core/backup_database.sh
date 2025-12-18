#!/bin/bash
# Database backup script for Jeeves

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPTS_DIR="$(dirname "$SCRIPT_DIR")"

source "$SCRIPTS_DIR/lib/common.sh"

# Configuration
PROJECT_ROOT="$(dirname "$SCRIPTS_DIR")"
BACKUP_DIR="${BACKUP_DIR:-${HOME}/.local/share/jeeves/backups}"
DB_PATH="${DATABASE_PATH:-${PROJECT_ROOT}/jeeves.db}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-7}"
DATE_STAMP="$(date +%Y%m%d-%H%M%S)"

mkdir -p "$BACKUP_DIR"

if [ ! -f "$DB_PATH" ]; then
    print_error "Database not found: $DB_PATH"
    exit 1
fi

print_info "Starting database backup..."
print_info "Database: $DB_PATH"
print_info "Backup directory: $BACKUP_DIR"

BACKUP_FILE="${BACKUP_DIR}/jeeves-${DATE_STAMP}.db"

sqlite3 "$DB_PATH" ".backup '${BACKUP_FILE}'"

if [ $? -eq 0 ]; then
    print_info "Backup created: $BACKUP_FILE"
    print_info "Backup size: $(du -h "$BACKUP_FILE" | cut -f1)"

    print_info "Verifying backup integrity..."
    INTEGRITY_CHECK=$(sqlite3 "$BACKUP_FILE" "PRAGMA integrity_check;" 2>&1)

    if [ "$INTEGRITY_CHECK" = "ok" ]; then
        print_success "Backup integrity verified"
    else
        print_error "Backup integrity check failed: $INTEGRITY_CHECK"
        rm -f "$BACKUP_FILE"
        exit 1
    fi

    print_info "Cleaning up backups older than $RETENTION_DAYS days..."
    find "$BACKUP_DIR" -name "jeeves-*.db" -type f -mtime +${RETENTION_DAYS} -delete 2>/dev/null || true

    BACKUP_COUNT=$(find "$BACKUP_DIR" -name "jeeves-*.db" -type f | wc -l)
    print_info "Total backups retained: $BACKUP_COUNT"
    print_success "Backup completed successfully"
else
    print_error "Backup failed"
    exit 1
fi
