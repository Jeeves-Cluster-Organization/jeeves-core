# Avionics Tests

**Constitutional Layer**: Infrastructure (depends on Core Engine)

**Location**: `jeeves_avionics/tests/`

---

## Overview

Avionics tests validate infrastructure adapters and service implementations. Tests are organized into **unit tests** (with mocks) and **integration tests** (with real services like PostgreSQL).

### Constitutional Compliance

Per [Avionics Constitution](../CONSTITUTION.md):

- **R1: Adapter Pattern** - Tests validate protocol implementations, not business logic
- **R4: Swappable Implementations** - Tests ensure any adapter can be replaced
- **R5: Defensive Error Handling** - Tests verify proper error categorization (transient vs permanent)

---

## Test Structure

```
tests/
├── conftest.py                    # Database, LLM, and core mocks
├── pytest.ini                     # Configuration with markers
├── fixtures/
│   ├── database.py                # PostgreSQL testcontainers
│   ├── llm.py                     # MockLLMProvider
│   └── mocks/
│       └── core_mocks.py          # MockCoreEnvelope (avoids core dependency)
└── unit/
    ├── database/                  # Database client tests
    │   ├── test_database.py       # PostgreSQL integration (requires Docker)
    │   └── test_connection_manager.py  # Connection pooling
    ├── llm/                       # LLM provider tests
    │   ├── test_llm_providers.py  # Provider factory, cost calculator
    │   └── test_cost_calculator.py  # Token cost tracking
    ├── gateway/                   # FastAPI gateway tests
    └── memory/                    # Memory services (L1-L4)
        ├── services/              # EmbeddingService (requires ML models)
        ├── repositories/          # Event, trace repositories
        └── adapters/              # SQL adapters
```

---

## Dependencies

### Lightweight (Unit Tests)

```bash
pip install pytest pytest-asyncio pydantic pydantic-settings sqlalchemy
```

### Integration Tests

```bash
# Database tests (requires Docker)
pip install testcontainers psycopg2-binary pgvector

# ML model tests (heavy - 1.5GB+ download)
pip install sentence-transformers

# LLM provider tests
pip install openai anthropic  # Optional: only for Azure/OpenAI tests
```

---

## Running Tests

### Quick Start (Lightweight Tests Only)

```bash
# Run tests without heavy dependencies
pytest jeeves_avionics/tests -m "not heavy and not requires_ml and not requires_docker"

# Expected: ~33 tests pass (cost calculator, mock providers)
```

### Full Test Suite (Requires Docker)

```bash
# Start PostgreSQL
docker compose up -d postgres

# Run all tests
pytest jeeves_avionics/tests

# Expected: More tests pass (database integration tests)
```

### Test Tiers

**Tier 1: Fast Unit Tests (No External Services)**
```bash
pytest jeeves_avionics/tests/unit/llm -m "not requires_azure and not requires_openai"
# Cost calculator, mock provider tests
# Runtime: < 1 second
```

**Tier 2: Integration Tests (Requires Docker)**
```bash
pytest jeeves_avionics/tests -m "requires_docker"
# Database client, connection manager tests
# Runtime: 5-10 seconds (container startup)
```

**Tier 3: Heavy ML Tests (Requires ML Models)**
```bash
pytest jeeves_avionics/tests -m "requires_ml"
# Embedding service tests
# Runtime: 60+ seconds (first run downloads 1.5GB model)
```

---

## Test Markers

Avionics tests use the following markers:

### Dependency Markers
- **`@pytest.mark.unit`** - Unit tests with mocked dependencies (fast)
- **`@pytest.mark.integration`** - Integration tests with real services
- **`@pytest.mark.heavy`** - Tests with heavy dependencies (ML models, large downloads)
- **`@pytest.mark.requires_ml`** - Requires ML models (sentence-transformers)
- **`@pytest.mark.requires_docker`** - Requires Docker/testcontainers
- **`@pytest.mark.requires_azure`** - Requires Azure OpenAI SDK and credentials
- **`@pytest.mark.requires_openai`** - Requires OpenAI package and API key
- **`@pytest.mark.requires_anthropic`** - Requires Anthropic package and API key

### Performance Markers
- **`@pytest.mark.slow`** - Slow-running tests (> 5 seconds)

### Example

```python
@pytest.mark.heavy
@pytest.mark.requires_ml
@pytest.mark.integration
class TestEmbeddingService:
    """Tests requiring sentence-transformers (1.5GB download)."""

    def test_embed_text(self, service):
        embedding = service.embed("Test text")
        assert len(embedding) == 384  # MiniLM-L6-v2 dimensionality
```

---

## Fixtures Provided

### Database Fixtures

- **`postgres_container`** - PostgreSQL testcontainer (requires Docker)
- **`pg_test_db`** - Fresh database client per test
- **`create_test_prerequisites`** - Creates test schema/tables
- **`create_session_only`** - Creates sessions table only

**Example**:
```python
@pytest.mark.integration
@pytest.mark.requires_docker
async def test_database_client(pg_test_db):
    """Test with real PostgreSQL."""
    session_id = uuid.uuid4()
    await pg_test_db.insert("sessions", {"session_id": session_id, ...})
    result = await pg_test_db.fetch_one("sessions", session_id)
    assert result is not None
```

### LLM Fixtures

- **`mock_llm_provider`** - MockLLMProvider with canned responses
- **`mock_llm_provider_factory`** - Factory for creating mock providers

**Example**:
```python
def test_llm_provider(mock_llm_provider):
    """Test with mock LLM (no API calls)."""
    response = await mock_llm_provider.generate("Test prompt", agent_role="planner")
    assert "steps" in response  # Canned planner response
```

### Core Mocks

- **`MockCoreEnvelope`** - Minimal envelope interface without core dependency
- **`mock_envelope_factory`** - Factory for creating mock envelopes
- **`mock_envelope`** - Basic mock envelope

**Example**:
```python
def test_memory_service(mock_envelope):
    """Test memory service without core engine dependency."""
    await memory_service.save_session(mock_envelope)
    loaded = await memory_service.load_session(mock_envelope.session_id)
    assert loaded.raw_input == mock_envelope.raw_input
```

---

## Test Coverage

### ✅ LLM Provider Tests (41 tests)

**Files**: `unit/llm/test_llm_providers.py`, `test_cost_calculator.py`

**Fast Tests (33 passed)**:
- Cost calculator (16 tests) - llamaserver, OpenAI, Anthropic pricing
- Mock provider (17 tests) - Factory, health checks, canned responses

**Azure Tests (8 require `openai` package)**:
- Azure provider initialization
- Azure endpoint validation
- Azure health checks

**Example**:
```python
def test_llamaserver_is_free():
    """Llamaserver has zero cost."""
    calc = CostCalculator()
    cost = calc.calculate_cost(
        provider="llamaserver",
        model="Qwen2.5-7B-Q4KM",
        prompt_tokens=1000,
        completion_tokens=500
    )
    assert cost.total_cost_usd == 0.0
```

### ⚠️ Memory Service Tests (9 require ML models)

**Files**: `unit/memory/services/test_embedding_service.py`, etc.

**Heavy Dependency**: `sentence-transformers` (1.5GB+ model download)

**What's Tested**:
- Embedding generation for text
- Embedding caching
- Similarity search
- Cross-reference management

**Recommendation**: Skip on lightweight CI, run on nightly builds.

### ⚠️ Database Tests (Require Docker)

**Files**: `unit/database/test_database.py`, `test_connection_manager.py`

**Requires**: Docker for PostgreSQL testcontainers

**What's Tested**:
- Database client CRUD operations
- Connection pooling
- Transaction management
- State backend (in-memory vs PostgreSQL)

**Example**:
```python
@pytest.mark.integration
@pytest.mark.requires_docker
async def test_insert_and_fetch(pg_test_db):
    """Test PostgreSQL insert/fetch with testcontainer."""
    session_id = uuid.uuid4()
    await pg_test_db.insert("sessions", {"session_id": session_id, ...})
    result = await pg_test_db.fetch_one("sessions", session_id)
    assert result["session_id"] == session_id
```

---

## CI/CD Integration

### Fast CI Pipeline (No Heavy Deps)

```yaml
# .github/workflows/test-avionics-fast.yml
jobs:
  test-avionics-fast:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-python@v4
        with:
          python-version: "3.11"
      - run: |
          pip install -r requirements/test.txt
          pytest jeeves_avionics/tests \
            -m "not heavy and not requires_ml and not requires_docker" \
            -v
```

**Expected**: 33 tests pass in < 5 seconds

### Full CI Pipeline (With Docker)

```yaml
# .github/workflows/test-avionics-full.yml
jobs:
  test-avionics-full:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: pgvector/pgvector:pg16
        env:
          POSTGRES_PASSWORD: test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-python@v4
      - run: |
          pip install -r requirements/test.txt
          pytest jeeves_avionics/tests \
            -m "not heavy and not requires_ml" \
            -v
```

**Expected**: Database tests pass, ML tests skipped

---

## Common Issues

### Issue 1: testcontainers Fails (No Docker)

**Error**: `ConnectionError: Could not connect to Docker`

**Solution**: Start Docker or skip integration tests:

```bash
pytest jeeves_avionics/tests -m "not requires_docker"
```

### Issue 2: sentence-transformers Download (1.5GB)

**Error**: Slow test execution on first run

**Solution**: Skip ML tests or pre-download models:

```bash
# Skip ML tests
pytest jeeves_avionics/tests -m "not requires_ml"

# Pre-download models
python -c "from sentence_transformers import SentenceTransformer; SentenceTransformer('all-MiniLM-L6-v2')"
```

### Issue 3: Azure Tests Fail (Missing openai Package)

**Error**: `ModuleNotFoundError: No module named 'openai'`

**Solution**: Azure tests are optional, skip them:

```bash
pytest jeeves_avionics/tests -m "not requires_azure"
```

### Issue 4: PostgreSQL Connection Errors

**Error**: `psycopg2.OperationalError: could not connect to server`

**Solution**: Ensure PostgreSQL container is running:

```bash
docker compose up -d postgres
docker compose ps  # Verify postgres is healthy
```

---

## Design Principles

### 1. Adapter Pattern (R1)

Tests validate **adapter interfaces**, not business logic:

```python
# ✅ GOOD: Test adapter implements protocol
def test_openai_adapter_implements_protocol():
    """OpenAI adapter implements LLMProtocol."""
    adapter = OpenAIAdapter(api_key="test")
    assert hasattr(adapter, 'generate')
    assert hasattr(adapter, 'health_check')

# ❌ BAD: Test business logic (belongs in mission system)
def test_planner_agent_uses_openai():
    # This is mission system concern, not avionics
    ...
```

### 2. Swappable Implementations (R4)

Tests ensure adapters are interchangeable:

```python
# ✅ GOOD: Test with any LLM provider
@pytest.mark.parametrize("provider", ["mock", "openai", "anthropic"])
def test_llm_provider_factory(provider):
    """All providers implement same interface."""
    llm = create_llm_provider(provider)
    response = await llm.generate("Test")
    assert isinstance(response, str)
```

### 3. Defensive Error Handling (R5)

Tests verify error categorization:

```python
# ✅ GOOD: Test transient error handling
async def test_rate_limit_retry():
    """Rate limit errors trigger retry."""
    provider = OpenAIAdapter()
    with mock.patch.object(provider, '_call_api', side_effect=RateLimitError):
        with pytest.raises(RateLimitError):
            await provider.generate("Test", max_retries=2)
    # Should have retried 2 times
    assert provider._call_count == 3  # 1 initial + 2 retries
```

---

## Development Workflow

### 1. Run Fast Tests During Development

```bash
# Quick feedback loop (< 1 second)
pytest jeeves_avionics/tests/unit/llm -m "not requires_azure"

# Watch mode
pytest-watch jeeves_avionics/tests/unit/llm
```

### 2. Run Integration Tests Before PR

```bash
# Start services
docker compose up -d postgres

# Run integration tests
pytest jeeves_avionics/tests -m "integration"
```

### 3. Add New Tests

**Template for Unit Test**:

```python
"""Unit tests for [feature]."""

import pytest


@pytest.mark.unit
class TestFeature:
    """Tests for [feature] with mocked dependencies."""

    def test_basic_case(self, mock_llm_provider):
        """Test basic functionality."""
        result = await mock_llm_provider.generate("Test")
        assert result is not None
```

**Template for Integration Test**:

```python
"""Integration tests for [feature]."""

import pytest


@pytest.mark.integration
@pytest.mark.requires_docker
async def test_feature_with_database(pg_test_db):
    """Test with real PostgreSQL."""
    # Test implementation
    ...
```

---

## Related Documentation

- **Constitution**: [../CONSTITUTION.md](../CONSTITUTION.md) - Avionics layer rules
- **Avionics Index**: [../INDEX.md](../INDEX.md) - Component reference
- **Protocols Tests**: [../../jeeves_protocols/tests/](../../jeeves_protocols/tests/)
- **Mission System Tests**: [../../jeeves_mission_system/tests/README.md](../../jeeves_mission_system/tests/README.md)

---

**Last Updated**: 2025-12-14
**Test Files**: 13
**Fast Tests**: 33 (no external deps)
**Integration Tests**: Requires Docker
**Heavy Tests**: Requires ML models (1.5GB)
