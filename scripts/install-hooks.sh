#!/usr/bin/env sh
#
# Install git hooks for local CI checks.
# Safe to run multiple times (idempotent).
#

set -e

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK_SRC="$REPO_ROOT/scripts/pre-push"
HOOK_DST="$REPO_ROOT/.git/hooks/pre-push"

if [ ! -f "$HOOK_SRC" ]; then
    echo "Error: $HOOK_SRC not found." >&2
    exit 1
fi

cp "$HOOK_SRC" "$HOOK_DST"
chmod +x "$HOOK_DST"

echo "Installed pre-push hook to $HOOK_DST"
