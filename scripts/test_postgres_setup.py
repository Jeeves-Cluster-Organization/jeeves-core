#!/usr/bin/env python3
"""
Test script to verify PostgreSQL + pgvector setup.

Usage:
    python scripts/test_postgres_setup.py
"""

import asyncio
import sys
from pathlib import Path

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from jeeves_avionics.settings import Settings
from jeeves_avionics.database.postgres_client import PostgreSQLClient
from jeeves_memory_module.services.embedding_service import EmbeddingService
# Direct import for low-level testing only - production code should use
# create_vector_adapter() from jeeves_mission_system.adapters
from jeeves_memory_module.repositories.pgvector_repository import PgVectorRepository

from jeeves_avionics.logging import get_current_logger


async def test_postgres_connection():
    """Test PostgreSQL connection."""
    print("\n" + "="*60)
    print("TEST 1: PostgreSQL Connection")
    print("="*60)

    settings = Settings()
    # Note: database_backend is now a read-only property always returning "postgres"

    try:
        client = PostgreSQLClient(
            database_url=settings.get_postgres_url(),
            pool_size=5,
            max_overflow=2,
        )

        await client.connect()
        print("✓ Connected to PostgreSQL")

        # Test query
        result = await client.fetch_one("SELECT version()")
        if result:
            version = result.get('version', 'Unknown')
            print(f"✓ PostgreSQL version: {version[:60]}...")

        await client.disconnect()
        print("✓ Disconnected successfully")

        return True

    except Exception as e:
        print(f"✗ Connection failed: {e}")
        return False


async def test_pgvector_extension():
    """Test pgvector extension."""
    print("\n" + "="*60)
    print("TEST 2: pgvector Extension")
    print("="*60)

    settings = Settings()
    # Note: database_backend is now a read-only property always returning "postgres"

    try:
        client = PostgreSQLClient(database_url=settings.get_postgres_url())
        await client.connect()

        # Check pgvector extension
        result = await client.fetch_one(
            "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector'"
        )

        if result:
            print(f"✓ pgvector extension installed: version {result['extversion']}")
        else:
            print("✗ pgvector extension not found")
            await client.disconnect()
            return False

        # Test vector operations
        test_query = """
        SELECT '[1,2,3]'::vector <-> '[4,5,6]'::vector AS distance
        """
        result = await client.fetch_one(test_query)

        if result:
            print(f"✓ Vector operations working: distance = {result['distance']:.4f}")

        await client.disconnect()
        return True

    except Exception as e:
        print(f"✗ pgvector test failed: {e}")
        return False


async def test_schema_initialization():
    """Test schema initialization."""
    print("\n" + "="*60)
    print("TEST 3: Schema Initialization")
    print("="*60)

    settings = Settings()
    # Note: database_backend is now a read-only property always returning "postgres"

    try:
        client = PostgreSQLClient(database_url=settings.get_postgres_url())
        await client.connect()

        # Check if tables exist
        tables_query = """
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'public'
        ORDER BY tablename
        """

        result = await client.fetch_all(tables_query)
        table_names = [row['tablename'] for row in result]

        expected_tables = [
            'sessions', 'requests', 'execution_plans', 'tool_executions',
            'responses', 'memory_retrievals', 'knowledge_facts',
            'response_corrections', 'messages', 'agent_scratchpads',
            'intent_macros', 'flow_interrupts'
        ]

        missing_tables = [t for t in expected_tables if t not in table_names]

        if missing_tables:
            print(f"⚠ Missing tables: {missing_tables}")
            print("  Initializing schema...")

            # Initialize core schema (infrastructure owns this)
            await client.initialize_schema("database/schemas/001_postgres_schema.sql")

            # Initialize capability schemas from registry (constitutional pattern)
            from jeeves_protocols import get_capability_resource_registry
            registry = get_capability_resource_registry()
            for schema_path in registry.get_schemas():
                if Path(schema_path).exists():
                    print(f"  Loading capability schema: {schema_path}")
                    await client.initialize_schema(schema_path)

            # Recheck
            result = await client.fetch_all(tables_query)
            table_names = [row['tablename'] for row in result]
            missing_tables = [t for t in expected_tables if t not in table_names]

            if not missing_tables:
                print("✓ Schema initialized successfully")
            else:
                print(f"✗ Still missing tables: {missing_tables}")
                await client.disconnect()
                return False
        else:
            print(f"✓ All {len(expected_tables)} tables exist")

        # Check vector columns
        vector_cols_query = """
        SELECT table_name, column_name
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND udt_name = 'vector'
        ORDER BY table_name, column_name
        """

        result = await client.fetch_all(vector_cols_query)

        if result:
            print(f"✓ Vector columns found:")
            for row in result:
                print(f"  - {row['table_name']}.{row['column_name']}")
        else:
            print("⚠ No vector columns found")

        await client.disconnect()
        return True

    except Exception as e:
        print(f"✗ Schema test failed: {e}")
        return False


async def test_embedding_operations():
    """Test embedding operations."""
    print("\n" + "="*60)
    print("TEST 4: Embedding Operations")
    print("="*60)

    settings = Settings()
    # Note: database_backend is now a read-only property always returning "postgres"

    try:
        client = PostgreSQLClient(database_url=settings.get_postgres_url())
        await client.connect()

        # Initialize embedding service
        embedding_service = EmbeddingService()
        print("✓ Embedding service initialized")

        # Create repository
        repo = PgVectorRepository(
            postgres_client=client,
            embedding_service=embedding_service,
            dimension=384
        )
        print("✓ pgvector repository created")

        # Test embedding generation
        test_text = "This is a test message for embedding"
        embedding = embedding_service.embed(test_text)
        print(f"✓ Generated embedding: dimension={len(embedding)}")

        # Get collection stats
        try:
            stats = await repo.get_collection_stats("tasks")
            print(f"✓ Tasks collection: {stats}")
        except Exception as e:
            print(f"⚠ Collection stats failed: {e}")

        await client.disconnect()
        return True

    except Exception as e:
        print(f"✗ Embedding test failed: {e}")
        return False


async def test_health_check():
    """Test database health check."""
    print("\n" + "="*60)
    print("TEST 5: Health Check")
    print("="*60)

    settings = Settings()
    # Note: database_backend is now a read-only property always returning "postgres"

    try:
        client = PostgreSQLClient(database_url=settings.get_postgres_url())
        await client.connect()

        health = await client.health_check()

        if health['status'] == 'healthy':
            print("✓ Health check passed")
            print(f"  Backend: {health.get('backend', 'unknown')}")
            if 'pool' in health:
                print(f"  Pool: {health['pool']}")
        else:
            print(f"✗ Health check failed: {health.get('error', 'unknown')}")
            await client.disconnect()
            return False

        await client.disconnect()
        return True

    except Exception as e:
        print(f"✗ Health check failed: {e}")
        return False


async def main():
    """Run all tests."""
    print("\n" + "="*60)
    print("PostgreSQL + pgvector Setup Verification")
    print("="*60)

    tests = [
        ("PostgreSQL Connection", test_postgres_connection),
        ("pgvector Extension", test_pgvector_extension),
        ("Schema Initialization", test_schema_initialization),
        ("Embedding Operations", test_embedding_operations),
        ("Health Check", test_health_check),
    ]

    results = []

    for name, test_func in tests:
        try:
            result = await test_func()
            results.append((name, result))
        except Exception as e:
            get_current_logger().error("test_exception", test=name, error=str(e))
            results.append((name, False))

    # Summary
    print("\n" + "="*60)
    print("TEST SUMMARY")
    print("="*60)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"{status}: {name}")

    print("="*60)
    print(f"TOTAL: {passed}/{total} tests passed")
    print("="*60 + "\n")

    if passed == total:
        print("🎉 All tests passed! PostgreSQL + pgvector is ready to use.")
        return 0
    else:
        print("⚠️  Some tests failed. Check the output above for details.")
        print("\nTroubleshooting:")
        print("1. Ensure PostgreSQL container is running: docker ps | grep postgres")
        print("2. Check logs: docker logs assistant-postgres")
        print("3. Verify .env settings: DATABASE_BACKEND=postgres")
        print("4. See docs/setup/POSTGRES_SETUP.md for setup instructions")
        return 1


if __name__ == "__main__":
    try:
        exit_code = asyncio.run(main())
        sys.exit(exit_code)
    except KeyboardInterrupt:
        print("\n\nTest interrupted by user")
        sys.exit(130)
    except Exception as e:
        print(f"\n\nTest suite failed: {e}")
        sys.exit(1)
