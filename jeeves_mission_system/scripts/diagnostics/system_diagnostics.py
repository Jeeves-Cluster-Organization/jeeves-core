#!/usr/bin/env python3
"""
Comprehensive system diagnostic script for the 7-agent assistant.

This script consolidates and improves upon the functionality of:
- diagnose_windows.py
- test_llm_connection.py
- verify_temperature_fix.py

Usage:
    python scripts/diagnostics/system_diagnostics.py [--full] [--platform windows|linux]

Examples:
    # Quick diagnostic (essential checks only)
    python scripts/diagnostics/system_diagnostics.py

    # Full diagnostic (all checks)
    python scripts/diagnostics/system_diagnostics.py --full

    # Platform-specific checks
    python scripts/diagnostics/system_diagnostics.py --platform windows
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Add project root to path
PROJECT_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(PROJECT_ROOT))

from scripts.lib.diagnostics import (
    DiagnosticRunner,
    ProjectStructureCheck,
    PythonImportCheck,
    GitStatusCheck,
    FileConflictCheck,
    DatabaseCheck,
    LlamaServerConnectionCheck,
    EnvironmentVariableCheck,
    check_python_version,
    check_dependencies,
)


def run_essential_diagnostics() -> bool:
    """Run essential diagnostic checks."""
    runner = DiagnosticRunner(verbose=False)

    # Add essential checks
    runner.results.append(check_python_version(min_version=(3, 8)))

    runner.add_check(ProjectStructureCheck())
    runner.add_check(FileConflictCheck())
    runner.add_check(PythonImportCheck([
        'agents',
        'config',
        'database',
        'tools',
        'llm',
        'memory',
    ]))
    runner.add_check(DatabaseCheck())

    return runner.run()


def run_full_diagnostics(verbose: bool = False) -> bool:
    """Run comprehensive diagnostic checks."""
    runner = DiagnosticRunner(verbose=verbose)

    # Python environment
    runner.results.append(check_python_version(min_version=(3, 8)))
    runner.results.append(check_dependencies())

    # Project structure
    runner.add_check(ProjectStructureCheck())
    runner.add_check(FileConflictCheck())

    # Python imports
    runner.add_check(PythonImportCheck([
        'agents',
        'agents.orchestrator',
        'agents.planner',
        'agents.executor',
        'agents.validator',
        'agents.meta_validator',
        'config',
        'config.settings',
        'database',
        'database.client',
        'tools',
        'tools.registry',
        'llm',
        'llm.factory',
        'llm.provider',
        'memory',
        'memory.manager',
    ]))

    # Git status
    runner.add_check(GitStatusCheck())

    # Database
    runner.add_check(DatabaseCheck())

    # Environment variables (optional for local setup)
    runner.add_check(EnvironmentVariableCheck(
        required_vars=[],  # No vars are strictly required for local mode
        optional_vars=['LLM_PROVIDER', 'OPENAI_API_KEY', 'ANTHROPIC_API_KEY']
    ))

    # llama-server connection (optional)
    runner.add_check(LlamaServerConnectionCheck())

    return runner.run()


def run_windows_diagnostics() -> bool:
    """Run Windows-specific diagnostic checks."""
    runner = DiagnosticRunner(verbose=True)

    print("Running Windows-specific diagnostics...")
    print()

    # All the basic checks
    runner.results.append(check_python_version(min_version=(3, 8)))

    runner.add_check(ProjectStructureCheck())
    runner.add_check(FileConflictCheck())

    # Focus on common Windows import issues
    runner.add_check(PythonImportCheck([
        'memory',
        'memory.manager',
        'memory.services',
        'memory.services.embedding_service',
        'memory.adapters',
        'memory.adapters.sql_adapter',
        'database.migrations.run_migration',
    ]))

    runner.add_check(GitStatusCheck())
    runner.add_check(DatabaseCheck())

    result = runner.run()

    if not result:
        print()
        print("=" * 70)
        print("WINDOWS TROUBLESHOOTING TIPS")
        print("=" * 70)
        print()
        print("If you see import errors:")
        print("  1. Clear Python caches:")
        print("     Remove-Item -Path .pytest_cache -Recurse -Force -ErrorAction SilentlyContinue")
        print("     Get-ChildItem -Path . -Recurse -Filter '__pycache__' | Remove-Item -Recurse -Force")
        print()
        print("  2. Verify you're in the project root:")
        print("     Get-Location  # Should show: ...\\assistant-7agent-1")
        print()
        print("  3. Check for memory.py file conflicts:")
        print("     Get-ChildItem -Path . -Filter 'memory.py'")
        print("     # Should return nothing (memory/ package should exist, not memory.py)")
        print()
        print("  4. Re-pull latest changes if needed:")
        print("     git pull origin main")
        print()

    return result


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="System diagnostics for 7-agent assistant",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        '--full',
        action='store_true',
        help='Run full diagnostic suite (all checks)'
    )

    parser.add_argument(
        '--platform',
        choices=['windows', 'linux', 'macos'],
        help='Run platform-specific diagnostics'
    )

    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Show detailed output for all checks'
    )

    args = parser.parse_args()

    # Run appropriate diagnostic suite
    if args.platform == 'windows':
        success = run_windows_diagnostics()
    elif args.full:
        success = run_full_diagnostics(verbose=args.verbose)
    else:
        success = run_essential_diagnostics()

    # Exit with appropriate code
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
