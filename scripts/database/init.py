#!/usr/bin/env python3
"""
Database Initialization Script (PostgreSQL-only)

Initializes the PostgreSQL database with the 7-agent assistant schema.
Can be used standalone or called from other scripts.

Usage:
    python init_db.py [--force] [--verify]

Examples:
    python init_db.py
    python init_db.py --verify
    python init_db.py --force  # WARNING: Drops and recreates schema
"""

import asyncio
import argparse
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

try:
    from avionics.database.postgres_client import PostgreSQLClient
    from avionics.settings import settings
except ImportError as e:
    print(f"❌ Error: Failed to import required modules: {e}")
    print("Make sure you're running from the project root directory")
    print("or have the correct PYTHONPATH set")
    sys.exit(1)


class DatabaseInitializer:
    """Handles PostgreSQL database initialization with schema validation."""

    def __init__(self, force: bool = False, verify: bool = False):
        self.force = force
        self.verify = verify
        self.schemas_dir = Path(__file__).parent.parent.parent.parent / "avionics" / "database" / "schemas"
        self.postgres_schema_path = self.schemas_dir / "001_postgres_schema.sql"

    def print_info(self, message: str):
        """Print info message."""
        print(f"[INFO]  {message}")

    def print_success(self, message: str):
        """Print success message."""
        print(f"[OK] {message}")

    def print_warning(self, message: str):
        """Print warning message."""
        print(f"[WARN]  {message}")

    def print_error(self, message: str):
        """Print error message."""
        print(f"❌ {message}")

    def check_prerequisites(self) -> bool:
        """Check if all prerequisites are met."""
        if not self.postgres_schema_path.exists():
            self.print_error(f"Schema file not found: {self.postgres_schema_path}")
            return False
        return True

    async def initialize_postgres(self) -> bool:
        """Initialize PostgreSQL database with schema."""
        try:
            self.print_info("Initializing PostgreSQL database")
            self.print_info(f"Database: {settings.postgres_database}")
            self.print_info(f"Host: {settings.postgres_host}:{settings.postgres_port}")
            self.print_info(f"Using schema: {self.postgres_schema_path}")

            # Create PostgreSQL client
            client = PostgreSQLClient(
                database_url=settings.get_postgres_url(),
                pool_size=5,
                max_overflow=2,
            )

            # Connect
            await client.connect()
            self.print_success("Connected to PostgreSQL")

            # Check if schema exists
            result = await client.fetch_one(
                "SELECT COUNT(*) AS count FROM pg_tables WHERE schemaname = 'public' AND tablename = 'sessions'"
            )

            if result and result.get('count', 0) > 0:
                if self.force:
                    self.print_warning("Schema exists. Forcing reinitialization...")
                    await client.execute("DROP SCHEMA public CASCADE")
                    await client.execute("CREATE SCHEMA public")
                    self.print_success("Existing schema dropped")
                else:
                    self.print_warning("Schema already exists")
                    self.print_info("Use --force to reinitialize (WARNING: This will DELETE all data!)")
                    await client.disconnect()
                    return False

            # Initialize schema
            await client.initialize_schema(str(self.postgres_schema_path))
            self.print_success("PostgreSQL schema applied successfully")

            # Verify pgvector extension
            result = await client.fetch_one(
                "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector'"
            )

            if result:
                self.print_success(f"pgvector extension installed: version {result['extversion']}")
            else:
                self.print_warning("pgvector extension not found (vector search may not work)")

            # Close connection
            await client.disconnect()
            return True

        except Exception as e:
            self.print_error(f"Failed to initialize PostgreSQL: {e}")
            import traceback
            traceback.print_exc()
            return False

    async def verify_schema(self) -> bool:
        """Verify the PostgreSQL database schema is correct."""
        if not self.verify:
            return True

        self.print_info("Verifying database schema...")

        try:
            client = PostgreSQLClient(
                database_url=settings.get_postgres_url(),
                pool_size=2,
                max_overflow=1,
            )
            await client.connect()

            # Expected core tables
            expected_tables = [
                "sessions",
                "requests",
                "execution_plans",
                "tool_executions",
                "responses",
                "memory_retrievals",
                "knowledge_facts",
                "response_corrections",
                "messages",
                "agent_scratchpads",
                "intent_macros",
                "flow_interrupts",
                "domain_events",
                "agent_traces",
                "semantic_chunks",
                "session_state",
                "graph_nodes",
                "graph_edges",
            ]

            # Check each table exists
            missing_tables = []
            for table in expected_tables:
                result = await client.fetch_one(
                    "SELECT COUNT(*) AS count FROM pg_tables WHERE schemaname = 'public' AND tablename = $1",
                    table
                )
                if not result or result.get('count', 0) == 0:
                    missing_tables.append(table)

            if missing_tables:
                self.print_error(f"Missing tables: {', '.join(missing_tables)}")
                await client.disconnect()
                return False

            self.print_success(f"All {len(expected_tables)} core tables verified")

            # Check indices count
            result = await client.fetch_one(
                "SELECT COUNT(*) AS count FROM pg_indexes WHERE schemaname = 'public' AND indexname LIKE 'idx_%'"
            )
            index_count = result["count"] if result else 0
            self.print_success(f"Found {index_count} performance indices")

            await client.disconnect()
            return True

        except Exception as e:
            self.print_error(f"Verification failed: {e}")
            import traceback
            traceback.print_exc()
            return False

    async def run(self) -> bool:
        """Run the complete initialization process."""
        print("=" * 70)
        print("PostgreSQL Database Initialization")
        print("=" * 70)
        print()

        # Check prerequisites
        if not self.check_prerequisites():
            return False

        # Initialize PostgreSQL
        if not await self.initialize_postgres():
            return False

        # Verify schema if requested
        if not await self.verify_schema():
            return False

        # Print summary
        print()
        print("=" * 70)
        self.print_success("PostgreSQL initialization complete!")
        print("=" * 70)
        print()
        self.print_info(f"Database: {settings.postgres_database}")
        self.print_info(f"Host: {settings.postgres_host}:{settings.postgres_port}")
        print()

        return True


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Initialize the PostgreSQL database for the 7-agent assistant",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s
  %(prog)s --verify
  %(prog)s --force  # WARNING: Drops and recreates schema
        """
    )

    parser.add_argument(
        "--force",
        action="store_true",
        help="Force reinitialization (drops existing schema)"
    )

    parser.add_argument(
        "--verify",
        action="store_true",
        help="Verify schema after initialization"
    )

    args = parser.parse_args()

    # Create initializer and run
    initializer = DatabaseInitializer(
        force=args.force,
        verify=args.verify,
    )

    # Run async initialization
    success = asyncio.run(initializer.run())

    # Exit with appropriate code
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
