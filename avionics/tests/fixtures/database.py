"""Database fixtures for avionics tests.

Provides PostgreSQL container management and database fixtures
for testing avionics database components.
"""

import os
import uuid
from datetime import datetime, timezone
from pathlib import Path
from uuid import uuid4

import pytest

from shared.testing import is_running_in_docker


# ============================================================
# Constants
# ============================================================

TEST_USER_ID = "test-user-001"
TEST_HIGH_CONFIDENCE = 0.95


# ============================================================
# PostgreSQL Container Classes
# ============================================================

class ComposePostgresContainer:
    """Fake container class for compose PostgreSQL service.

    Provides same interface as testcontainers PostgresContainer.
    """

    def __init__(self, host: str, port: int, user: str, password: str, dbname: str):
        self.host = host
        self.port = port
        self.user = user
        self.password = password
        self.dbname = dbname

    def get_connection_url(self) -> str:
        return f"postgresql+asyncpg://{self.user}:{self.password}@{self.host}:{self.port}/{self.dbname}"


# ============================================================
# PostgreSQL Fixtures
# ============================================================

@pytest.fixture(scope="session")
def postgres_container():
    """PostgreSQL container for all tests.

    Uses compose service if running in Docker, otherwise testcontainers.
    Container is shared across all tests to minimize startup overhead.
    """
    if is_running_in_docker():
        postgres_host = os.environ.get("POSTGRES_HOST", "postgres")
        postgres_port = int(os.environ.get("POSTGRES_PORT", "5432"))
        postgres_user = os.environ.get("POSTGRES_USER", "assistant")
        postgres_password = os.environ.get("POSTGRES_PASSWORD", "dev_password_change_in_production")
        postgres_db = os.environ.get("POSTGRES_DATABASE", "assistant")

        yield ComposePostgresContainer(
            host=postgres_host,
            port=postgres_port,
            user=postgres_user,
            password=postgres_password,
            dbname=postgres_db,
        )
        return

    try:
        from testcontainers.postgres import PostgresContainer
    except ImportError:
        pytest.skip("testcontainers not available - requires Docker/Podman")

    try:
        with PostgresContainer(
            image="pgvector/pgvector:pg16",
            username="postgres",
            password="postgres",
            dbname="test",
        ) as postgres:
            postgres.get_connection_url()
            yield postgres
    except Exception as e:
        pytest.skip(f"PostgreSQL container failed: {e}")


@pytest.fixture(scope="function")
async def pg_test_db(postgres_container):
    """Fresh PostgreSQL database for each test.

    Creates unique database per test for isolation.
    Applies consolidated PostgreSQL schema automatically.
    """
    from avionics.database.postgres_client import PostgreSQLClient
    from sqlalchemy import text

    db_name = f"test_{uuid.uuid4().hex[:8]}"
    admin_url = postgres_container.get_connection_url()

    admin_client = PostgreSQLClient(admin_url)
    await admin_client.connect()

    try:
        async with admin_client.engine.connect() as conn:
            conn_with_autocommit = await conn.execution_options(isolation_level="AUTOCOMMIT")
            await conn_with_autocommit.execute(text(f"CREATE DATABASE {db_name}"))
    finally:
        await admin_client.disconnect()

    test_url = admin_url.rsplit("/", 1)[0] + f"/{db_name}"
    db = PostgreSQLClient(test_url)
    await db.connect()

    # Load core schema (infrastructure owns this)
    schemas_dir = Path(__file__).parent.parent.parent.parent / "database" / "schemas"
    if schemas_dir.exists():
        schema_file = schemas_dir / "001_postgres_schema.sql"
        if schema_file.exists():
            await db.initialize_schema(str(schema_file))

    # Load capability schemas from registry (constitutional pattern)
    # Avionics R3: No Domain Logic - registry lookup instead of hardcoded paths
    from protocols import get_capability_resource_registry
    registry = get_capability_resource_registry()
    for schema_path in registry.get_schemas():
        if Path(schema_path).exists():
            await db.initialize_schema(schema_path)

    yield db

    await db.disconnect()

    admin_client = PostgreSQLClient(admin_url)
    await admin_client.connect()
    try:
        async with admin_client.engine.connect() as conn:
            conn_with_autocommit = await conn.execution_options(isolation_level="AUTOCOMMIT")
            await conn_with_autocommit.execute(
                text(f"SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}'")
            )
            await conn_with_autocommit.execute(text(f"DROP DATABASE IF EXISTS {db_name}"))
    finally:
        await admin_client.disconnect()


# ============================================================
# FK Prerequisites Helper Functions
# ============================================================

async def create_test_prerequisites(db, user_id: str = TEST_USER_ID) -> tuple:
    """Create prerequisite records (session -> request -> plan) for FK constraints.

    Args:
        db: PostgreSQLClient instance
        user_id: User ID for the records

    Returns:
        tuple: (session_id, request_id, plan_id) as strings
    """
    session_id = str(uuid4())
    request_id = str(uuid4())
    plan_id = str(uuid4())
    now = datetime.now(timezone.utc)

    await db.insert("sessions", {
        "session_id": session_id,
        "user_id": user_id,
        "created_at": now,
        "last_activity": now,
    })

    await db.insert("requests", {
        "request_id": request_id,
        "user_id": user_id,
        "session_id": session_id,
        "user_message": "Test request",
        "received_at": now,
        "status": "processing",
    })

    await db.insert("execution_plans", {
        "plan_id": plan_id,
        "request_id": request_id,
        "intent": "test",
        "confidence": TEST_HIGH_CONFIDENCE,
        "plan_json": "{}",
        "created_at": now,
    })

    return session_id, request_id, plan_id


async def create_session_only(db, user_id: str = TEST_USER_ID) -> str:
    """Create a session record only.

    Args:
        db: PostgreSQLClient instance
        user_id: User ID for the session

    Returns:
        str: The created session_id
    """
    session_id = str(uuid4())
    now = datetime.now(timezone.utc)

    await db.insert("sessions", {
        "session_id": session_id,
        "user_id": user_id,
        "created_at": now,
        "last_activity": now,
    })

    return session_id
