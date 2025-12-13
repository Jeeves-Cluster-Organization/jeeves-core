#!/bin/bash
# Hardening Cleanup Script
# Generated: 2025-12-03
# Reference: HARDENING_AUDIT.md
#
# IMPORTANT: Review HARDENING_AUDIT.md before running this script
# Run with --dry-run first to see what would be deleted

set -e

DRY_RUN=false
if [[ "$1" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "=== DRY RUN MODE - No files will be deleted ==="
fi

delete_file() {
    if [ -f "$1" ]; then
        if [ "$DRY_RUN" = true ]; then
            echo "[DRY RUN] Would delete: $1"
        else
            echo "Deleting: $1"
            rm "$1"
        fi
    else
        echo "[SKIP] File not found: $1"
    fi
}

echo ""
echo "========================================"
echo "Phase A: Constitution Compliance Cleanup"
echo "========================================"
echo ""

echo "--- 1. Deleting Kanban UI and Tests ---"
delete_file "static/js/kanban.js"
delete_file "static/css/kanban.css"
delete_file "api/templates/kanban.html"
delete_file "tests/frontend/unit/kanban.test.js"
delete_file "tests/frontend/integration/kanban-flow.test.js"
delete_file "scripts/diagnose_kanban_visibility.py"
delete_file "scripts/testing/run_gateway_kanban_tests.sh"

echo ""
echo "--- 2. Deleting Journal UI ---"
delete_file "static/js/journal.js"
delete_file "static/css/journal.css"
delete_file "api/templates/journal.html"

echo ""
echo "--- 3. Deleting Open Loops ---"
delete_file "memory/services/open_loop_service.py"
delete_file "memory/repositories/open_loop_repository.py"
delete_file "tests/unit/memory/test_open_loop_service.py"
delete_file "tests/unit/memory/test_open_loop_repository.py"

echo ""
echo "========================================"
echo "Phase B: Deprecated Code Cleanup"
echo "========================================"
echo ""

echo "--- Deleting ChromaDB VectorAdapter ---"
delete_file "memory/adapters/vector_adapter.py"

echo ""
echo "========================================"
echo "Manual Steps Required"
echo "========================================"
echo ""
echo "The following require manual editing (not automated):"
echo ""
echo "1. database/schemas/postgres_schema.sql"
echo "   - Remove 'tasks' table definition"
echo "   - Remove 'journal_entries' table definition"
echo "   - Remove 'open_loops' table definition"
echo "   - Remove 'kv_store' table definition"
echo ""
echo "2. proto/jeeves.proto"
echo "   - Remove KanbanService definition"
echo "   - Remove JournalService definition"
echo "   - Remove OpenLoopService definition"
echo ""
echo "3. gateway/grpc_client.py"
echo "   - Remove _kanban_stub"
echo "   - Remove _journal_stub"
echo "   - Remove _open_loop_stub"
echo ""
echo "4. api/templates/base.html"
echo "   - Remove Kanban nav link"
echo "   - Remove Journal nav link"
echo ""
echo "5. config/settings.py"
echo "   - Remove kanban_enabled"
echo "   - Remove kanban_default_columns"
echo "   - Remove kanban_column_spacing"
echo "   - Remove sqlite-related config"
echo ""
echo "6. config/agent_tool_access.py"
echo "   - Remove journal permissions"
echo ""
echo "7. config/feature_flags.py"
echo "   - Remove max_open_loops"
echo ""
echo "8. scripts/run/chat.py"
echo "   - Remove journal_tools import"
echo ""
echo "9. api/governance.py"
echo "   - Remove open_loop_service import"
echo ""
echo "10. tests/integration/test_l2_event_emission.py"
echo "    - Remove JournalTools import"
echo ""
echo "11. memory/adapters/__init__.py"
echo "    - Remove VectorAdapter export"
echo ""
echo "12. memory/manager.py"
echo "    - Remove VectorAdapter import and type hint"
echo ""

if [ "$DRY_RUN" = true ]; then
    echo "=== DRY RUN COMPLETE - No files were deleted ==="
else
    echo "=== File deletion complete ==="
    echo "Run 'git status' to see changes"
    echo "Run tests with 'pytest' to verify nothing broke"
fi
