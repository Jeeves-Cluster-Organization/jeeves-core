#!/usr/bin/env python3
"""
Centralized diagnostic utilities for the 7-agent assistant system.

This module provides common diagnostic functions used across various diagnostic scripts.
Consolidates functionality from diagnose_windows.py, test_llm_connection.py, etc.

Usage:
    from scripts.lib.diagnostics import DiagnosticRunner, SystemCheck, LLMCheck

    runner = DiagnosticRunner()
    runner.add_check(SystemCheck())
    runner.add_check(LLMCheck())
    runner.run()
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple


@dataclass
class DiagnosticResult:
    """Result of a diagnostic check."""

    name: str
    passed: bool
    message: str
    details: Optional[Dict[str, Any]] = None
    severity: str = "info"  # info, warning, error


class DiagnosticCheck(ABC):
    """Base class for diagnostic checks."""

    @abstractmethod
    def name(self) -> str:
        """Return the name of this check."""
        pass

    @abstractmethod
    def run(self) -> DiagnosticResult:
        """Execute the diagnostic check."""
        pass


class DiagnosticRunner:
    """Runs a collection of diagnostic checks and reports results."""

    def __init__(self, verbose: bool = False):
        self.checks: List[DiagnosticCheck] = []
        self.verbose = verbose
        self.results: List[DiagnosticResult] = []

    def add_check(self, check: DiagnosticCheck):
        """Add a diagnostic check to the runner."""
        self.checks.append(check)

    def run(self) -> bool:
        """Run all checks and return True if all passed."""
        self.print_header("Running Diagnostic Checks")

        all_passed = True
        for check in self.checks:
            result = check.run()
            self.results.append(result)

            status = "[OK]" if result.passed else "[X]"
            color = self._get_color(result)

            print(f"  {color}{status}{self._reset()} {result.name}")

            if self.verbose or not result.passed:
                print(f"       {result.message}")
                if result.details:
                    for key, value in result.details.items():
                        print(f"         {key}: {value}")

            if not result.passed:
                all_passed = False

        self.print_summary(all_passed)
        return all_passed

    def print_header(self, title: str):
        """Print a formatted header."""
        print("=" * 70)
        print(title)
        print("=" * 70)
        print()

    def print_summary(self, all_passed: bool):
        """Print summary of diagnostic results."""
        print()
        print("=" * 70)

        passed_count = sum(1 for r in self.results if r.passed)
        total_count = len(self.results)

        if all_passed:
            print(f"{self._green()}✓ All checks passed ({passed_count}/{total_count}){self._reset()}")
        else:
            failed_count = total_count - passed_count
            print(f"{self._red()}✗ {failed_count} check(s) failed ({passed_count}/{total_count} passed){self._reset()}")

            print("\nFailed Checks:")
            for result in self.results:
                if not result.passed:
                    print(f"  - {result.name}: {result.message}")

        print("=" * 70)

    def _get_color(self, result: DiagnosticResult) -> str:
        """Get ANSI color code based on result."""
        if result.passed:
            return self._green()
        elif result.severity == "warning":
            return self._yellow()
        else:
            return self._red()

    @staticmethod
    def _green() -> str:
        return "\033[0;32m"

    @staticmethod
    def _yellow() -> str:
        return "\033[1;33m"

    @staticmethod
    def _red() -> str:
        return "\033[0;31m"

    @staticmethod
    def _reset() -> str:
        return "\033[0m"


# Common Diagnostic Checks

class ProjectStructureCheck(DiagnosticCheck):
    """Check if project structure is correct."""

    def __init__(self, expected_dirs: Optional[List[str]] = None):
        self.expected_dirs = expected_dirs or [
            'agents', 'api', 'config', 'database', 'tools', 'tests',
            'memory', 'llm', 'scripts'
        ]

    def name(self) -> str:
        return "Project Structure"

    def run(self) -> DiagnosticResult:
        """Check if expected directories exist."""
        missing = []
        for dir_name in self.expected_dirs:
            if not os.path.isdir(dir_name):
                missing.append(dir_name)

        if missing:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message=f"Missing directories: {', '.join(missing)}",
                severity="error"
            )

        return DiagnosticResult(
            name=self.name(),
            passed=True,
            message=f"All {len(self.expected_dirs)} expected directories found"
        )


class PythonImportCheck(DiagnosticCheck):
    """Check if Python modules can be imported."""

    def __init__(self, modules: List[str]):
        self.modules = modules

    def name(self) -> str:
        return "Python Module Imports"

    def run(self) -> DiagnosticResult:
        """Try to import each module."""
        failed_imports = []

        for module_name in self.modules:
            try:
                spec = importlib.util.find_spec(module_name)
                if spec is None:
                    failed_imports.append(module_name)
            except Exception as e:
                failed_imports.append(f"{module_name} ({str(e)})")

        if failed_imports:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message=f"Failed to import: {', '.join(failed_imports)}",
                severity="error"
            )

        return DiagnosticResult(
            name=self.name(),
            passed=True,
            message=f"All {len(self.modules)} modules can be imported"
        )


class GitStatusCheck(DiagnosticCheck):
    """Check git repository status."""

    def name(self) -> str:
        return "Git Repository Status"

    def run(self) -> DiagnosticResult:
        """Check git status and latest commit."""
        try:
            result = subprocess.run(
                ['git', 'log', '--oneline', '-1'],
                capture_output=True,
                text=True,
                timeout=5
            )

            if result.returncode == 0:
                commit_info = result.stdout.strip()
                return DiagnosticResult(
                    name=self.name(),
                    passed=True,
                    message=f"Latest commit: {commit_info}",
                    details={"commit": commit_info}
                )
            else:
                return DiagnosticResult(
                    name=self.name(),
                    passed=False,
                    message="Git command failed",
                    severity="warning"
                )
        except Exception as e:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message=f"Could not check git status: {str(e)}",
                severity="warning"
            )


class FileConflictCheck(DiagnosticCheck):
    """Check for file conflicts (e.g., memory.py shadowing memory/ package)."""

    def __init__(self, conflicts: Optional[List[Tuple[str, str]]] = None):
        # List of (file_path, package_name) tuples
        self.conflicts = conflicts or [
            ('memory.py', 'memory/'),
            ('agents.py', 'agents/'),
            ('config.py', 'config/'),
        ]

    def name(self) -> str:
        return "File Conflict Detection"

    def run(self) -> DiagnosticResult:
        """Check for files that shadow packages."""
        found_conflicts = []

        for file_path, package_name in self.conflicts:
            if Path(file_path).exists():
                found_conflicts.append(f"{file_path} shadows {package_name}")

        if found_conflicts:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message="File/package conflicts found",
                details={"conflicts": found_conflicts},
                severity="error"
            )

        return DiagnosticResult(
            name=self.name(),
            passed=True,
            message="No file/package conflicts detected"
        )


class DatabaseCheck(DiagnosticCheck):
    """Check PostgreSQL database connection and schema."""

    def __init__(self):
        pass  # Uses settings for PostgreSQL connection

    def name(self) -> str:
        return "PostgreSQL Database"

    def run(self) -> DiagnosticResult:
        """Check if PostgreSQL database is accessible."""
        import asyncio

        async def _check():
            try:
                from jeeves_avionics.database.postgres_client import PostgreSQLClient
                from jeeves_avionics.settings import settings

                client = PostgreSQLClient(
                    database_url=settings.get_postgres_url(),
                    pool_size=2,
                    max_overflow=0,
                )
                await client.connect()

                # Check table count
                result = await client.fetch_one(
                    "SELECT COUNT(*) AS count FROM pg_tables WHERE schemaname = 'public'"
                )
                table_count = result.get('count', 0) if result else 0

                # Check pgvector extension
                result = await client.fetch_one(
                    "SELECT extversion FROM pg_extension WHERE extname = 'vector'"
                )
                pgvector_version = result.get('extversion') if result else None

                await client.disconnect()

                if table_count > 0:
                    details = {
                        "host": f"{settings.postgres_host}:{settings.postgres_port}",
                        "database": settings.postgres_database,
                        "tables": table_count,
                    }
                    if pgvector_version:
                        details["pgvector"] = pgvector_version

                    return DiagnosticResult(
                        name=self.name(),
                        passed=True,
                        message=f"PostgreSQL OK ({table_count} tables)",
                        details=details
                    )
                else:
                    return DiagnosticResult(
                        name=self.name(),
                        passed=False,
                        message="No tables found in database",
                        details={"hint": "Run: python scripts/database/init.py"},
                        severity="error"
                    )
            except Exception as e:
                return DiagnosticResult(
                    name=self.name(),
                    passed=False,
                    message=f"Database error: {str(e)}",
                    details={"hint": "Check PostgreSQL is running"},
                    severity="error"
                )

        return asyncio.run(_check())


class LlamaServerConnectionCheck(DiagnosticCheck):
    """Check llama-server connection and health."""

    def __init__(self, host: str = "http://localhost:8080"):
        self.host = host

    def name(self) -> str:
        return "llama-server Connection"

    def run(self) -> DiagnosticResult:
        """Test llama-server connection via health endpoint."""
        try:
            import httpx

            with httpx.Client(timeout=5.0) as client:
                response = client.get(f"{self.host}/health")
                if response.status_code == 200:
                    return DiagnosticResult(
                        name=self.name(),
                        passed=True,
                        message=f"Connected to llama-server at {self.host}",
                        details={"host": self.host, "status": "healthy"}
                    )
                else:
                    return DiagnosticResult(
                        name=self.name(),
                        passed=False,
                        message=f"llama-server returned status {response.status_code}",
                        details={"hint": "Check llama-server logs"},
                        severity="warning"
                    )

        except ImportError:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message="httpx package not installed",
                details={"hint": "Install with: pip install httpx"},
                severity="warning"
            )
        except Exception as e:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message=f"Connection failed: {str(e)}",
                details={"hint": "Make sure llama-server is running: docker compose up -d llama-server"},
                severity="warning"
            )


class EnvironmentVariableCheck(DiagnosticCheck):
    """Check required environment variables."""

    def __init__(self, required_vars: Optional[List[str]] = None,
                 optional_vars: Optional[List[str]] = None):
        self.required_vars = required_vars or []
        self.optional_vars = optional_vars or []

    def name(self) -> str:
        return "Environment Variables"

    def run(self) -> DiagnosticResult:
        """Check if required environment variables are set."""
        missing_required = []
        missing_optional = []

        for var in self.required_vars:
            if not os.getenv(var):
                missing_required.append(var)

        for var in self.optional_vars:
            if not os.getenv(var):
                missing_optional.append(var)

        if missing_required:
            return DiagnosticResult(
                name=self.name(),
                passed=False,
                message=f"Missing required variables: {', '.join(missing_required)}",
                details={"missing_required": missing_required, "missing_optional": missing_optional},
                severity="error"
            )

        message_parts = []
        if self.required_vars:
            message_parts.append(f"All {len(self.required_vars)} required variables set")
        if missing_optional:
            message_parts.append(f"{len(missing_optional)} optional variables not set")

        return DiagnosticResult(
            name=self.name(),
            passed=True,
            message=", ".join(message_parts) if message_parts else "No variables to check",
            details={"missing_optional": missing_optional} if missing_optional else None,
            severity="info" if missing_optional else "info"
        )


# Utility functions

def check_python_version(min_version: Tuple[int, int] = (3, 8)) -> DiagnosticResult:
    """Check if Python version meets minimum requirement."""
    current = sys.version_info
    required = min_version

    if (current.major, current.minor) >= required:
        return DiagnosticResult(
            name="Python Version",
            passed=True,
            message=f"Python {current.major}.{current.minor}.{current.micro}",
            details={"version": f"{current.major}.{current.minor}.{current.micro}"}
        )
    else:
        return DiagnosticResult(
            name="Python Version",
            passed=False,
            message=f"Python {current.major}.{current.minor} < required {required[0]}.{required[1]}",
            severity="error"
        )


def check_dependencies(requirements_file: str = "requirements.txt") -> DiagnosticResult:
    """Check if all dependencies from requirements.txt are installed."""
    try:
        result = subprocess.run(
            [sys.executable, "-m", "pip", "check"],
            capture_output=True,
            text=True,
            timeout=10
        )

        if result.returncode == 0:
            return DiagnosticResult(
                name="Dependencies",
                passed=True,
                message="All dependencies satisfied"
            )
        else:
            return DiagnosticResult(
                name="Dependencies",
                passed=False,
                message="Dependency conflicts detected",
                details={"output": result.stdout},
                severity="warning"
            )
    except Exception as e:
        return DiagnosticResult(
            name="Dependencies",
            passed=False,
            message=f"Could not check dependencies: {str(e)}",
            severity="warning"
        )
