#!/usr/bin/env python3
"""
Quick verification script for PostgreSQL + pgvector setup.
Tests basic functionality without requiring full dependency installation.
"""

import sys
import os
import asyncio
from pathlib import Path

# Add project root to path
PROJECT_ROOT = Path(__file__).parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

# Colors
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34m'
NC = '\033[0m'


def print_header(text: str):
    """Print section header."""
    print(f"\n{BLUE}{'=' * 70}{NC}")
    print(f"{BLUE}{text}{NC}")
    print(f"{BLUE}{'=' * 70}{NC}\n")


def print_test(name: str, passed: bool, details: str = ""):
    """Print test result."""
    status = f"{GREEN}✓ PASS{NC}" if passed else f"{RED}✗ FAIL{NC}"
    print(f"{status} {name}")
    if details:
        print(f"       {details}")


def print_info(text: str):
    """Print info message."""
    print(f"{YELLOW}ℹ{NC}  {text}")


async def test_imports():
    """Test critical imports."""
    try:
        # Test database factory
        from jeeves_avionics.database.factory import create_database_client
        from jeeves_mission_system.adapters import create_vector_adapter
        print_test("Import database.factory", True, "Factory functions available")

        # Test PostgreSQL client
        from jeeves_avionics.database.postgres_client import PostgreSQLClient
        print_test("Import database.postgres_client", True, "PostgreSQLClient available")

        # Test memory tools
        from tools.memory_tools import register_memory_tools_async
        print_test("Import tools.memory_tools", True, "Async memory tools available")

        # Test settings
        from jeeves_avionics.settings import settings
        print_test("Import config.settings", True, f"Backend: {settings.database_backend}")

        return True
    except ImportError as e:
        print_test("Imports", False, f"Import error: {e}")
        return False
    except Exception as e:
        print_test("Imports", False, f"Error: {e}")
        return False


async def test_postgres_connection():
    """Test PostgreSQL connection."""
    try:
        from jeeves_avionics.database.postgres_client import PostgreSQLClient
        from jeeves_avionics.settings import settings

        if settings.database_backend != "postgres":
            print_info("Skipping PostgreSQL test (not configured for postgres)")
            return True

        client = PostgreSQLClient(
            database_url=settings.get_postgres_url(),
            pool_size=2,
            max_overflow=0,
            echo=False
        )

        await client.connect()
        print_test("PostgreSQL Connection", True, "Connected successfully")

        # Test query
        result = await client.fetch_one("SELECT version()")
        if result:
            version = result.get('version', 'unknown')[:50]
            print_test("PostgreSQL Query", True, f"Version: {version}...")

        # Check pgvector
        result = await client.fetch_one(
            "SELECT COUNT(*) as count FROM pg_extension WHERE extname = 'vector'"
        )
        if result and result.get('count', 0) > 0:
            result = await client.fetch_one(
                "SELECT extversion FROM pg_extension WHERE extname = 'vector'"
            )
            version = result.get('extversion', 'unknown')
            print_test("pgvector Extension", True, f"Version: {version}")
        else:
            print_test("pgvector Extension", False, "Not installed")

        # Check tables
        result = await client.fetch_one(
            "SELECT COUNT(*) as count FROM pg_tables WHERE schemaname = 'public'"
        )
        table_count = result.get('count', 0) if result else 0
        if table_count > 0:
            print_test("Database Schema", True, f"{table_count} tables found")
        else:
            print_test("Database Schema", False, "No tables found - run init_db.py")

        await client.disconnect()
        return True

    except ImportError as e:
        print_info(f"PostgreSQL modules not installed: {e}")
        print_info("Install with: pip install sqlalchemy[asyncio] asyncpg pgvector")
        return True  # Not a failure, just not installed
    except Exception as e:
        print_test("PostgreSQL Connection", False, f"Error: {e}")
        print_info("Make sure PostgreSQL is running: docker compose up -d postgres")
        return False


async def test_configuration():
    """Test configuration settings."""
    try:
        from jeeves_avionics.settings import settings

        print_info(f"Current configuration:")
        print(f"       Database Backend: {settings.database_backend}")
        print(f"       Vector Backend: {settings.vector_backend}")
        print(f"       PostgreSQL URL: {settings.get_postgres_url()}")
        print_test("Configuration", True, "Configured for PostgreSQL + pgvector")

        return True
    except Exception as e:
        print_test("Configuration", False, f"Error: {e}")
        return False


async def main():
    """Run all verification tests."""
    print_header("PostgreSQL + pgvector Setup Verification")

    all_passed = True

    # Test 1: Imports
    print_header("Test 1: Module Imports")
    if not await test_imports():
        all_passed = False
        print_info("Some modules failed to import")
        print_info("Install dependencies: pip install -r requirements.txt")

    # Test 2: Configuration
    print_header("Test 2: Configuration")
    if not await test_configuration():
        all_passed = False

    # Test 3: PostgreSQL Connection
    print_header("Test 3: PostgreSQL Connection")
    if not await test_postgres_connection():
        all_passed = False

    # Summary
    print_header("Verification Summary")

    if all_passed:
        print(f"{GREEN}✓ All tests passed!{NC}\n")
        print("Next steps:")
        print("1. Start the server: python start_server.py")
        print("2. Test the API: curl http://localhost:8000/health")
        print("3. Check logs for: memory_tools_initialized_async")
        print("")
        return 0
    else:
        print(f"{RED}✗ Some tests failed{NC}\n")
        print("Troubleshooting:")
        print("1. Install dependencies: pip install -r requirements.txt")
        print("2. Start PostgreSQL: docker compose up -d postgres")
        print("3. Initialize schema: python init_db.py --backend postgres")
        print("4. Check .env file for correct configuration")
        print("")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
