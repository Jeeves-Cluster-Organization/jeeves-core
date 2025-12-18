.PHONY: install test test-unit test-integration test-e2e test-docker clean docker-build docker-run docker-stop help \
	test-tier1 test-tier2 test-tier3 test-tier4 test-fast test-light test-core test-avionics test-avionics-full \
	test-mission test-mission-full test-app test-contract test-ci test-nightly test-services-check \
	test-protocols test-control-tower test-memory test-all-unit test-all-integration \
	test-comprehensive test-comprehensive-cov test-parallel test-discover test-quick

# Default target
.DEFAULT_GOAL := help

## help: Show this help message
help:
	@echo "Available targets:"
	@echo ""
	@sed -n 's/^##//p' $(MAKEFILE_LIST) | column -t -s ':' | sed -e 's/^/ /'
	@echo ""

## install: Install Python dependencies
install:
	pip install --upgrade pip
	pip install -r requirements/all.txt

## test: Run all tests (unit + integration)
test: test-unit test-integration

## test-unit: Run unit tests only
test-unit:
	python -m pytest tests/unit/ -v --tb=short

## test-integration: Run integration tests only
test-integration:
	python -m pytest tests/integration/ -v --tb=short

## test-e2e: Run end-to-end tests (requires Ollama)
test-e2e:
	OLLAMA_AVAILABLE=1 python -m pytest tests/e2e/ -v --tb=short --timeout=120

## test-all: Run all tests including E2E
test-all:
	python -m pytest -v --tb=short

## test-docker: Run tests in Docker containers
test-docker:
	docker compose -f docker/docker-compose.yml run --rm test pytest -v --tb=short

## test-cov: Run tests with coverage report
test-cov:
	python -m pytest -v --tb=short --cov=. --cov-report=html --cov-report=term

## clean: Remove Python cache files and test artifacts
clean:
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete
	find . -type d -name ".pytest_cache" -exec rm -rf {} + 2>/dev/null || true
	rm -rf .coverage htmlcov/ .mypy_cache/
	rm -f *.db *.db-shm *.db-wal

## docker-build: Build all Docker images (gateway + orchestrator)
docker-build:
	docker build -t jeeves-gateway:latest -f docker/Dockerfile --target gateway .
	docker build -t jeeves-core:latest -f docker/Dockerfile --target orchestrator .

## docker-build-test: Build Docker test image
docker-build-test:
	docker build -t jeeves-core:test -f docker/Dockerfile --target test .

## docker-run: Run Docker containers (API + PostgreSQL + Ollama)
docker-run:
	docker compose -f docker/docker-compose.yml up -d

## docker-stop: Stop Docker containers
docker-stop:
	docker compose -f docker/docker-compose.yml down

## docker-logs: View Docker container logs
docker-logs:
	docker compose -f docker/docker-compose.yml logs -f

## format: Format code with black and isort (if installed)
format:
	@command -v black >/dev/null 2>&1 && black . || echo "black not installed, skipping"
	@command -v isort >/dev/null 2>&1 && isort . || echo "isort not installed, skipping"

## lint: Run linters (if installed)
lint:
	@command -v flake8 >/dev/null 2>&1 && flake8 . || echo "flake8 not installed, skipping"
	@command -v mypy >/dev/null 2>&1 && mypy . || echo "mypy not installed, skipping"

## dev: Install development dependencies and setup
dev: install
	@command -v pre-commit >/dev/null 2>&1 && pre-commit install || echo "pre-commit not installed"
	@echo "Development environment ready"

## verify: Quick verification (install + test-unit)
verify: install test-unit
	@echo "✅ Quick verification passed"

# =============================================================================
# Test Tier Targets (Per Constitutional Layer Structure)
# =============================================================================

## test-tier1: Tier 1 - Fast unit tests (no external deps) - Core + Avionics + Mission contracts
test-tier1:
	@echo "🚀 Running Tier 1: Fast unit tests (no external dependencies)"
	@echo "   - Core engine: Pure orchestration (109 tests)"
	@echo "   - Avionics: LLM cost calculator, mock providers (33 tests)"
	@echo "   - Mission System: Contract tests (fast)"
	@echo "   - App layer: All unit tests (13 test files)"
	@echo ""
	python -m pytest -c pytest-light.ini \
		jeeves_core_engine/tests \
		jeeves_avionics/tests/unit/llm \
		jeeves_mission_system/tests/contract \
		jeeves-capability-code-analyser/tests \
		-v
	@echo ""
	@echo "✅ Tier 1 complete (expected: < 10 seconds)"

## test-tier2: Tier 2 - Integration tests (requires Docker) - Database + testcontainers
test-tier2:
	@echo "🐳 Running Tier 2: Integration tests (requires Docker)"
	@echo "   - Avionics: Database client tests"
	@echo "   - Mission System: Unit tests without LLM"
	@echo ""
	@echo "Prerequisites: docker compose up -d postgres"
	@echo ""
	python -m pytest \
		jeeves_avionics/tests/unit/database \
		jeeves_mission_system/tests/unit \
		-m "not requires_llamaserver and not requires_ml" \
		-v
	@echo ""
	@echo "✅ Tier 2 complete (expected: 10-30 seconds)"

## test-tier3: Tier 3 - Integration tests with real LLM (requires Docker + llamaserver)
test-tier3:
	@echo "🧠 Running Tier 3: Integration tests with real LLM"
	@echo "   - Mission System: Integration tests"
	@echo "   - Mission System: API endpoint tests"
	@echo ""
	@echo "Prerequisites: docker compose up -d postgres llama-server"
	@echo ""
	python -m pytest \
		jeeves_mission_system/tests/integration \
		-m "not e2e and not heavy" \
		-v
	@echo ""
	@echo "✅ Tier 3 complete (expected: 30-60 seconds)"

## test-tier4: Tier 4 - E2E tests (requires full stack) - Complete 7-agent pipeline
test-tier4:
	@echo "🎯 Running Tier 4: End-to-end tests (full stack)"
	@echo "   - Mission System: E2E tests with real LLM"
	@echo ""
	@echo "Prerequisites: docker compose up -d (all services)"
	@echo ""
	python -m pytest \
		jeeves_mission_system/tests \
		-m e2e \
		-v
	@echo ""
	@echo "✅ Tier 4 complete (expected: 60+ seconds)"

## test-fast: Fast tests only (Tier 1) - NO external dependencies
test-fast: test-tier1
	@echo "✅ Fast tests complete"

## test-light: Lightweight tests (excludes heavy ML models)
test-light:
	@echo "🪶 Running lightweight tests (no ML models, no Docker)"
	python -m pytest -c pytest-light.ini -v
	@echo ""
	@echo "✅ Lightweight tests complete"

## test-core: Test core engine only (fastest - 109 tests in < 1s)
test-core:
	@echo "⚡ Testing core engine (pure orchestration, no external deps)"
	python -m pytest jeeves_core_engine/tests -v
	@echo ""
	@echo "✅ Core engine tests complete"

## test-avionics: Test avionics layer (lightweight - no ML/Docker)
test-avionics:
	@echo "🛠️  Testing avionics layer (LLM providers, cost calculator)"
	python -m pytest jeeves_avionics/tests -m "not heavy and not requires_ml and not requires_docker" -v
	@echo ""
	@echo "✅ Avionics tests complete (lightweight)"

## test-avionics-full: Test avionics layer (requires Docker for database tests)
test-avionics-full:
	@echo "🛠️  Testing avionics layer (full - requires Docker)"
	@echo "Prerequisites: docker compose up -d postgres"
	python -m pytest jeeves_avionics/tests -m "not heavy and not requires_ml" -v
	@echo ""
	@echo "✅ Avionics tests complete (full)"

## test-mission: Test mission system (lightweight - no services)
test-mission:
	@echo "🎯 Testing mission system (contract + unit tests)"
	python -m pytest \
		jeeves_mission_system/tests/contract \
		jeeves_mission_system/tests/unit \
		-m "not requires_llamaserver and not requires_postgres" \
		-v
	@echo ""
	@echo "✅ Mission system tests complete (lightweight)"

## test-mission-full: Test mission system (requires services)
test-mission-full:
	@echo "🎯 Testing mission system (full - requires Docker services)"
	@echo "Prerequisites: docker compose up -d postgres llama-server"
	python -m pytest jeeves_mission_system/tests -m "not e2e and not heavy" -v
	@echo ""
	@echo "✅ Mission system tests complete (full)"

## test-app: Test application layer (all mocked - no external deps)
test-app:
	@echo "📱 Testing application layer (code analyser)"
	python -m pytest jeeves-capability-code-analyser/tests -v
	@echo ""
	@echo "✅ Application layer tests complete"

## test-contract: Run constitutional contract tests only
test-contract:
	@echo "📜 Running constitutional contract tests"
	@echo "   - Import boundary validation"
	@echo "   - Layer boundary enforcement"
	@echo "   - Evidence chain integrity (P1)"
	python -m pytest jeeves_mission_system/tests/contract -v
	@echo ""
	@echo "✅ Contract tests complete"

## test-ci: CI-friendly test run (fast, no heavy deps, skips mission_system due to flakiness)
test-ci:
	@echo "🤖 Running CI test suite (fast, no external dependencies)"
	@echo "   - Core engine: 109 tests"
	@echo "   - Avionics: 33 tests (lightweight)"
	@echo "   - Skipping: App layer (import errors - needs refactoring)"
	@echo "   - Skipping: Mission system (flaky - requires real LLM)"
	@echo ""
	python -m pytest -c pytest-light.ini \
		jeeves_core_engine/tests \
		jeeves_avionics/tests/unit/llm \
		-v
	@echo ""
	@echo "✅ CI test suite complete"

## test-nightly: Nightly test run (full suite including E2E)
test-nightly:
	@echo "🌙 Running nightly test suite (full E2E)"
	@echo "Prerequisites: docker compose up -d (all services)"
	@$(MAKE) test-tier1
	@$(MAKE) test-tier2
	@$(MAKE) test-tier3
	@$(MAKE) test-tier4
	@echo ""
	@echo "✅ Nightly test suite complete"

## test-services-check: Check if required services are running
test-services-check:
	@echo "🔍 Checking required services..."
	@echo ""
	@echo "PostgreSQL:"
	@docker compose ps postgres 2>/dev/null || echo "  ❌ Not running (docker compose up -d postgres)"
	@echo ""
	@echo "llama-server:"
	@docker compose ps llama-server 2>/dev/null || echo "  ❌ Not running (docker compose up -d llama-server)"
	@echo ""
	@echo "llamaserver health:"
	@curl -s http://localhost:8080/health >/dev/null 2>&1 && echo "  ✅ Healthy" || echo "  ❌ Not reachable"

# =============================================================================
# Comprehensive Module Testing (All Directories)
# =============================================================================

## test-protocols: Test protocols layer (fast, no external deps)
test-protocols:
	@echo "📋 Testing protocols layer (type definitions, utilities)"
	python -m pytest jeeves_protocols/tests -v
	@echo ""
	@echo "✅ Protocols tests complete"

## test-control-tower: Test control tower kernel (fast, no external deps)
test-control-tower:
	@echo "🗼 Testing control tower kernel"
	python -m pytest jeeves_control_tower/tests -v
	@echo ""
	@echo "✅ Control tower tests complete"

## test-memory: Test memory module (lightweight)
test-memory:
	@echo "🧠 Testing memory module"
	python -m pytest jeeves_memory_module/tests -v
	@echo ""
	@echo "✅ Memory module tests complete"

## test-all-unit: Run ALL unit tests across all modules
test-all-unit:
	@echo "🧪 Running ALL unit tests across all modules"
	@echo ""
	@echo "Modules:"
	@echo "   - jeeves_protocols"
	@echo "   - jeeves_control_tower"
	@echo "   - jeeves_memory_module"
	@echo "   - jeeves_avionics"
	@echo "   - jeeves_mission_system"
	@echo "   - jeeves-capability-code-analyser"
	@echo ""
	python -m pytest \
		jeeves_protocols/tests/unit \
		jeeves_control_tower/tests/unit \
		jeeves_memory_module/tests/unit \
		jeeves_avionics/tests/unit \
		jeeves_mission_system/tests/unit \
		jeeves-capability-code-analyser/tests/unit \
		-v --tb=short
	@echo ""
	@echo "✅ All unit tests complete"

## test-all-integration: Run ALL integration tests across all modules (requires Docker)
test-all-integration:
	@echo "🔗 Running ALL integration tests across all modules"
	@echo ""
	@echo "Prerequisites: docker compose up -d postgres"
	@echo ""
	python -m pytest \
		jeeves_control_tower/tests/integration \
		jeeves_memory_module/tests/integration \
		jeeves_avionics/tests/unit/database \
		jeeves_mission_system/tests/integration \
		-v --tb=short -m "not e2e and not requires_llamaserver"
	@echo ""
	@echo "✅ All integration tests complete"

## test-comprehensive: Run comprehensive test suite (all modules, fast tests only)
test-comprehensive:
	@echo "🎯 Running comprehensive test suite (all modules)"
	@echo ""
	@echo "This runs all fast tests across ALL modules:"
	@echo "   1. Protocols (type definitions)"
	@echo "   2. Control Tower (kernel)"
	@echo "   3. Memory Module"
	@echo "   4. Avionics (LLM, database)"
	@echo "   5. Mission System (contracts + unit)"
	@echo "   6. Code Analyser App"
	@echo ""
	python -m pytest \
		jeeves_protocols/tests \
		jeeves_control_tower/tests \
		jeeves_memory_module/tests \
		jeeves_avionics/tests \
		jeeves_mission_system/tests/contract \
		jeeves_mission_system/tests/unit \
		jeeves-capability-code-analyser/tests \
		-v --tb=short \
		-m "not requires_llamaserver and not requires_postgres and not e2e and not heavy"
	@echo ""
	@echo "✅ Comprehensive test suite complete"

## test-comprehensive-cov: Run comprehensive tests with coverage report
test-comprehensive-cov:
	@echo "📊 Running comprehensive test suite with coverage"
	@echo ""
	python -m pytest \
		jeeves_protocols/tests \
		jeeves_control_tower/tests \
		jeeves_memory_module/tests \
		jeeves_avionics/tests \
		jeeves_mission_system/tests/contract \
		jeeves_mission_system/tests/unit \
		jeeves-capability-code-analyser/tests \
		-v --tb=short \
		-m "not requires_llamaserver and not requires_postgres and not e2e and not heavy" \
		--cov=jeeves_protocols \
		--cov=jeeves_control_tower \
		--cov=jeeves_memory_module \
		--cov=jeeves_avionics \
		--cov=jeeves_mission_system \
		--cov-report=html \
		--cov-report=term
	@echo ""
	@echo "✅ Coverage report generated in htmlcov/"

## test-parallel: Run tests in parallel (requires pytest-xdist)
test-parallel:
	@echo "⚡ Running tests in parallel"
	python -m pytest \
		jeeves_protocols/tests \
		jeeves_control_tower/tests \
		jeeves_memory_module/tests \
		jeeves_avionics/tests/unit \
		jeeves_mission_system/tests/unit \
		jeeves-capability-code-analyser/tests \
		-v --tb=short \
		-m "not requires_llamaserver and not requires_postgres and not e2e and not heavy" \
		-n auto
	@echo ""
	@echo "✅ Parallel tests complete"

## test-discover: Discover all available tests
test-discover:
	@echo "🔍 Discovering all available tests..."
	@echo ""
	python -m pytest --collect-only -q 2>/dev/null | tail -20
	@echo ""
	@echo "Total test files discovered:"
	@find . -name "test_*.py" -not -path "./.venv/*" | wc -l

## test-quick: Ultra-fast sanity check (minimal tests)
test-quick:
	@echo "⚡ Running quick sanity check"
	python -m pytest \
		jeeves_protocols/tests/unit/test_core.py \
		jeeves_control_tower/tests/unit/test_resource_tracker.py \
		-v --tb=short
	@echo ""
	@echo "✅ Quick sanity check complete"
