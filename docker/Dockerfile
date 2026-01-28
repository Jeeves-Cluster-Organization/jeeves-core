# ==============================================================================
# Jeeves Core - Multi-stage Dockerfile
# ==============================================================================
#
# Targets:
#   - base:        Base Python image with dependencies
#   - go-builder:  Go binary builder (envelope CLI)
#   - gateway:     FastAPI HTTP gateway (avionics/gateway)
#   - orchestrator: Full 7-agent orchestration runtime
#   - test:        Test runner with all dependencies
#
# Usage:
#   docker build -t jeeves-gateway:latest -f docker/Dockerfile --target gateway .
#   docker build -t jeeves-orchestrator:latest -f docker/Dockerfile --target orchestrator .
#   docker build -t jeeves-test:latest -f docker/Dockerfile --target test .
#
# ==============================================================================

# ------------------------------------------------------------------------------
# Stage 1: Go Builder (envelope CLI)
# ------------------------------------------------------------------------------
FROM golang:1.24-alpine AS go-builder

WORKDIR /build

# Copy Go module files
COPY go.mod go.sum ./

# Download Go dependencies
RUN go mod download

# Copy Go source code
COPY cmd/ ./cmd/
COPY commbus/ ./commbus/
COPY coreengine/ ./coreengine/

# Build envelope binary
RUN CGO_ENABLED=0 GOOS=linux go build -o /envelope ./cmd/envelope

# ------------------------------------------------------------------------------
# Stage 2: Base Python Image
# ------------------------------------------------------------------------------
FROM python:3.11-slim AS base

# Set environment variables
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PYTHONPATH=/app \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1

WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r jeeves && useradd -r -g jeeves jeeves

# Copy envelope binary from Go builder
COPY --from=go-builder /envelope /usr/local/bin/envelope
RUN chmod +x /usr/local/bin/envelope

# Install base Python dependencies
RUN pip install --upgrade pip setuptools wheel

# ------------------------------------------------------------------------------
# Stage 3: Dependencies Layer
# ------------------------------------------------------------------------------
FROM base AS dependencies

# Install core Python packages (without requirements file, install individually)
RUN pip install \
    # Web framework
    fastapi>=0.109.0 \
    uvicorn[standard]>=0.27.0 \
    httpx>=0.26.0 \
    # Async support
    aiofiles>=23.2.1 \
    aiohttp>=3.9.0 \
    # gRPC
    grpcio>=1.60.0 \
    grpcio-tools>=1.60.0 \
    protobuf>=4.25.0 \
    # Data validation
    pydantic>=2.5.0 \
    pydantic-settings>=2.1.0 \
    # Database
    asyncpg>=0.29.0 \
    psycopg[binary]>=3.1.0 \
    sqlalchemy>=2.0.0 \
    alembic>=1.13.0 \
    # Logging/Observability
    structlog>=24.1.0 \
    prometheus-client>=0.19.0 \
    opentelemetry-api>=1.22.0 \
    opentelemetry-sdk>=1.22.0 \
    opentelemetry-exporter-otlp-proto-grpc>=1.22.0 \
    opentelemetry-instrumentation-fastapi>=0.43b0 \
    opentelemetry-instrumentation-grpc>=0.43b0 \
    # Utilities
    python-dotenv>=1.0.0 \
    orjson>=3.9.0 \
    tenacity>=8.2.0 \
    tiktoken>=0.5.0 \
    # ML/AI support
    numpy>=1.26.0 \
    scipy>=1.12.0 \
    # Redis for distributed infrastructure
    redis>=5.0.0

# ------------------------------------------------------------------------------
# Stage 4: Gateway Target
# ------------------------------------------------------------------------------
FROM dependencies AS gateway

# Copy Python packages (L0 Foundation)
COPY protocols/ ./protocols/
COPY shared/ ./shared/

# Copy Python packages (L1 Infrastructure)
COPY avionics/ ./avionics/
COPY control_tower/ ./control_tower/

# Copy Python packages (Memory)
COPY memory_module/ ./memory_module/

# Copy proto package and generate gRPC stubs
COPY proto/ ./proto/
RUN python -m grpc_tools.protoc -I. \
    --python_out=. \
    --grpc_python_out=. \
    proto/jeeves.proto

# Set ownership
RUN chown -R jeeves:jeeves /app

USER jeeves

# Gateway configuration
ENV API_HOST=0.0.0.0 \
    API_PORT=8000 \
    LOG_LEVEL=INFO

EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Start gateway
CMD ["python", "-m", "uvicorn", "avionics.gateway.main:app", "--host", "0.0.0.0", "--port", "8000"]

# ------------------------------------------------------------------------------
# Stage 5: Orchestrator Target (Full Runtime)
# ------------------------------------------------------------------------------
FROM dependencies AS orchestrator

# Copy Python packages (L0 Foundation)
COPY protocols/ ./protocols/
COPY shared/ ./shared/

# Copy Python packages (L1 Infrastructure)
COPY avionics/ ./avionics/
COPY control_tower/ ./control_tower/

# Copy Python packages (Memory)
COPY memory_module/ ./memory_module/

# Copy Python packages (L2 Application)
COPY mission_system/ ./mission_system/

# Copy proto package and generate gRPC stubs
COPY proto/ ./proto/
RUN python -m grpc_tools.protoc -I. \
    --python_out=. \
    --grpc_python_out=. \
    proto/jeeves.proto

# Copy configuration files
COPY .env ./

# Set ownership
RUN chown -R jeeves:jeeves /app

USER jeeves

# Orchestrator configuration
ENV API_HOST=0.0.0.0 \
    API_PORT=8000 \
    LOG_LEVEL=INFO

EXPOSE 8000
EXPOSE 50051

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Start orchestrator (mission system API)
CMD ["python", "-m", "uvicorn", "mission_system.api.server:app", "--host", "0.0.0.0", "--port", "8000"]

# ------------------------------------------------------------------------------
# Stage 6: Test Target
# ------------------------------------------------------------------------------
FROM dependencies AS test

# Install test dependencies
RUN pip install \
    pytest>=7.4.0 \
    pytest-asyncio>=0.23.0 \
    pytest-cov>=4.1.0 \
    pytest-timeout>=2.2.0 \
    pytest-xdist>=3.5.0 \
    httpx>=0.26.0 \
    respx>=0.20.0 \
    testcontainers>=3.7.0

# Copy all Python packages
COPY protocols/ ./protocols/
COPY shared/ ./shared/
COPY avionics/ ./avionics/
COPY control_tower/ ./control_tower/
COPY memory_module/ ./memory_module/
COPY mission_system/ ./mission_system/

# Copy test configuration
COPY tests/ ./tests/
COPY conftest.py ./
COPY .env ./
COPY pytest*.ini ./

# Copy coreengine package structure and proto files
# protocols/grpc_stub.py imports from coreengine.proto, so stubs must stay in-place
COPY coreengine/__init__.py ./coreengine/__init__.py
COPY coreengine/proto/__init__.py ./coreengine/proto/__init__.py
COPY coreengine/proto/engine.proto ./coreengine/proto/engine.proto

# Copy top-level proto package (jeeves.proto for gRPC services)
COPY proto/ ./proto/

# Generate gRPC stubs for both proto packages
RUN python -m grpc_tools.protoc -I. \
    --python_out=. \
    --grpc_python_out=. \
    coreengine/proto/engine.proto \
    proto/jeeves.proto

# Set ownership
RUN chown -R jeeves:jeeves /app

USER jeeves

# Test configuration
ENV PYTHONPATH=/app \
    LOG_LEVEL=DEBUG

# Default: run all tests
CMD ["pytest", "-v", "--tb=short"]
