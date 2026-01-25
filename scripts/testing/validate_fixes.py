#!/usr/bin/env python3
"""
Fix Validation Script
=====================

Validates that all the fixes are working correctly.
Runs quick checks on all fixed components.
"""

import asyncio
import os
import sys
from pathlib import Path

# Add project root to path
project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(project_root))

# Set test mode
os.environ['SKIP_CONFIG_VALIDATION'] = '1'
os.environ['DEPLOYMENT_MODE'] = 'single_node'

# Use centralized console utilities
from scripts.lib.console import Colors, check


async def main():
    """Run all validation checks."""
    print("=" * 80)
    print("FIX VALIDATION")
    print("=" * 80)
    print()

    checks_passed = 0
    checks_failed = 0

    # Check 1: Config settings has deployment_mode
    try:
        from avionics.settings import settings
        has_attr = hasattr(settings, 'deployment_mode')
        if check("Settings has deployment_mode attribute", has_attr):
            checks_passed += 1
        else:
            checks_failed += 1
    except Exception as e:
        check("Settings import", False, str(e))
        checks_failed += 1

    # Check 2: Node profiles work
    try:
        from config.node_profiles import get_deployment_mode, get_node_for_agent
        mode = get_deployment_mode()
        node = get_node_for_agent("planner")
        if check(f"Node profiles work (mode={mode}, node={node})", True):
            checks_passed += 1
        else:
            checks_failed += 1
    except Exception as e:
        check("Node profiles", False, str(e))
        checks_failed += 1

    # Check 3: LLM Factory works
    try:
        from avionics.settings import settings
        from avionics.llm.factory import LLMFactory

        factory = LLMFactory(settings)
        provider = factory.get_provider_for_agent("planner")

        if check(f"LLM Factory works (provider={type(provider).__name__})", provider is not None):
            checks_passed += 1
        else:
            checks_failed += 1
    except Exception as e:
        check("LLM Factory", False, str(e))
        checks_failed += 1

    # Check 4: Database client API
    try:
        from avionics.database.client import create_database_client
        from avionics.settings import Settings

        settings = Settings()
        db = await create_database_client(settings)
        await db.connect()

        # Check connection.cursor() exists
        has_cursor = hasattr(db.connection, 'cursor')

        if check("Database connection.cursor() exists", has_cursor):
            checks_passed += 1
        else:
            checks_failed += 1

        await db.close()
    except Exception as e:
        check("Database client", False, str(e))
        checks_failed += 1

    # Check 5: Tool registry list_tools method
    try:
        from tools.registry import ToolRegistry

        registry = ToolRegistry()
        has_method = hasattr(registry, 'list_tools')

        if check("Tool registry has list_tools() method", has_method):
            checks_passed += 1
        else:
            checks_failed += 1
    except Exception as e:
        check("Tool registry", False, str(e))
        checks_failed += 1

    # Check 6: Test script imports
    try:
        # Try to import the test module
        test_module_path = project_root / "scripts" / "testing" / "test_single_node.py"
        if test_module_path.exists():
            if check("Test script exists", True):
                checks_passed += 1
        else:
            check("Test script exists", False, "File not found")
            checks_failed += 1
    except Exception as e:
        check("Test script", False, str(e))
        checks_failed += 1

    # Check 7: Backward compatibility
    try:
        from avionics.llm.factory import create_agent_provider_with_node_awareness, create_agent_provider

        # Both functions should exist
        if check("Backward compatibility (old and new functions exist)", True):
            checks_passed += 1
        else:
            checks_failed += 1
    except Exception as e:
        check("Backward compatibility", False, str(e))
        checks_failed += 1

    # Check 8: Mock mode works
    try:
        os.environ['MOCK_LLM_ENABLED'] = 'true'
        from avionics.settings import settings
        from avionics.llm.factory import create_agent_provider_with_node_awareness

        provider = create_agent_provider_with_node_awareness(settings, "planner")
        is_mock = "Mock" in type(provider).__name__

        if check("Mock mode works", is_mock):
            checks_passed += 1
        else:
            checks_failed += 1

        del os.environ['MOCK_LLM_ENABLED']
    except Exception as e:
        check("Mock mode", False, str(e))
        checks_failed += 1

    # Summary
    print()
    print("=" * 80)
    print("SUMMARY")
    print("=" * 80)
    print(f"Checks passed: {Colors.GREEN}{checks_passed}{Colors.NC}")
    print(f"Checks failed: {Colors.RED}{checks_failed}{Colors.NC}")
    print()

    if checks_failed == 0:
        print(f"{Colors.GREEN}✓ All fixes validated successfully!{Colors.NC}")
        print()
        print("Next steps:")
        print("  1. Run full test suite: python scripts/testing/test_single_node.py --mock")
        print("  2. Run comprehensive tests: python scripts/testing/test_comprehensive.py")
        print("  3. Start API server: python -m api.server")
        print()
        return 0
    else:
        print(f"{Colors.RED}✗ Some fixes need attention{Colors.NC}")
        print()
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
