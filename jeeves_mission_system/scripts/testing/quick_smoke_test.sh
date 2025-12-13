#!/bin/bash
# ==============================================================================
# Quick Smoke Test
# ==============================================================================
#
# Purpose: Fast smoke test to verify basic functionality
# Usage: ./scripts/testing/quick_smoke_test.sh
#
# This runs the most critical tests only (< 30 seconds)
#
# ==============================================================================

set -e

echo "🚀 Running quick smoke test..."
echo ""

# Test 1: Check .env exists
echo "1. Checking .env configuration..."
if [ -f .env ]; then
    echo "   ✓ .env exists"
else
    echo "   ✗ .env not found"
    echo "   Run: cp .env.example .env"
    exit 1
fi

# Test 2: Check Python imports
echo "2. Checking Python imports..."
python3 -c "
from config.settings import settings
from orchestrator.orchestrator_compat import Orchestrator
from llm.factory import LLMFactory
from database.client import DatabaseClient
print('   ✓ All imports successful')
" || {
    echo "   ✗ Import errors detected"
    exit 1
}

# Test 3: Run fast unit tests
echo "3. Running fast unit tests..."
MOCK_LLM_ENABLED=true pytest tests/unit/test_database.py tests/unit/test_circuit_breaker.py -v --tb=short -q || {
    echo "   ✗ Some tests failed"
    exit 1
}
echo "   ✓ Unit tests passed"

# Test 4: Check if Ollama is accessible (optional)
echo "4. Checking Ollama connectivity (optional)..."
if curl -s -f http://localhost:11434/api/tags > /dev/null 2>&1; then
    echo "   ✓ Ollama is running"
else
    echo "   ⚠ Ollama not running (optional for development)"
fi

echo ""
echo "✅ Smoke test passed! System is healthy."
echo ""
echo "Next steps:"
echo "  - Full validation: ./scripts/testing/validate_single_node.sh"
echo "  - Mock LLM tests: python scripts/testing/test_single_node.py --mock"
echo "  - Live LLM tests: python scripts/testing/test_single_node.py --live"
echo ""
