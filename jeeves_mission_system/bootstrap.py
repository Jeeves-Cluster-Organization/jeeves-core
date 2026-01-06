"""Composition Root - Build AppContext and inject dependencies.

ADR-001 Decision 3: Singleton Elimination via AppContext
This module is the COMPOSITION ROOT for the Jeeves application.
It is the ONLY place where concrete implementations are instantiated
and wired together.

All configuration is passed via AppContext - no global state.

Usage:
    from jeeves_mission_system.bootstrap import create_app_context

    # At application startup
    app_context = create_app_context()

    # Use in API server
    app.state.context = app_context

    # Pass to components
    service = MyService(context=app_context)

    # Access configuration via context
    bounds = app_context.core_config.context_bounds
"""

import os
from typing import Any, Dict, List, Optional, TYPE_CHECKING

from jeeves_avionics.context import AppContext, SystemClock
from jeeves_avionics.logging import configure_logging
from jeeves_avionics.settings import Settings, get_settings
from jeeves_avionics.feature_flags import FeatureFlags, get_feature_flags
from jeeves_protocols.config import CoreConfig, ContextBounds, OrchestrationFlags
from jeeves_protocols import get_capability_resource_registry

from jeeves_control_tower import ControlTower, ResourceQuota

# Type alias for LLMGateway (imported conditionally)
LLMGatewayType = Any

# Import context variable for per-request PID tracking
from contextvars import ContextVar

# Request context for per-request PID tracking
# Set this before LLM calls to enable Control Tower resource tracking
request_pid_context: ContextVar[Optional[str]] = ContextVar("request_pid_context", default=None)


def set_request_pid(pid: str) -> None:
    """Set the current request PID for resource tracking.

    Call this at the start of request processing to enable
    per-request resource tracking through the LLM Gateway.

    Args:
        pid: Process ID (envelope_id) for the current request
    """
    request_pid_context.set(pid)


def clear_request_pid() -> None:
    """Clear the current request PID after processing completes."""
    request_pid_context.set(None)


def get_request_pid() -> Optional[str]:
    """Get the current request PID for resource tracking.

    Returns:
        Current PID or None if not in a request context
    """
    return request_pid_context.get()

if TYPE_CHECKING:
    from jeeves_protocols import (
        LanguageConfigProtocol,
        NodeProfilesProtocol,
        AgentToolAccessProtocol,
        ConfigRegistryProtocol,
    )


def _parse_bool(value: str, default: bool = True) -> bool:
    """Parse boolean from environment variable string."""
    return value.lower() == "true" if value else default


def _parse_optional_int(env_var: str) -> Optional[int]:
    """Parse an optional integer from environment variable."""
    val = os.getenv(env_var)
    return int(val) if val else None


def create_core_config_from_env() -> CoreConfig:
    """Create CoreConfig from environment variables.

    Environment Variables:
        CORE_MAX_ITERATIONS: Max pipeline iterations
        CORE_MAX_LLM_CALLS: Max LLM calls per request
        CORE_MAX_AGENT_HOPS: Max agent transitions
        CORE_ENABLE_TELEMETRY: Enable telemetry
        CORE_ENABLE_CHECKPOINTS: Enable checkpointing
        CORE_DEBUG_MODE: Enable debug mode
        CORE_MAX_INPUT_TOKENS: Max input tokens
        CORE_MAX_OUTPUT_TOKENS: Max output tokens
        CORE_MAX_CONTEXT_TOKENS: Max context window tokens
        CORE_RESERVED_TOKENS: Reserved tokens for system

    Returns:
        CoreConfig with environment-parsed values
    """
    return CoreConfig(
        max_iterations=int(os.getenv("CORE_MAX_ITERATIONS", "3")),
        max_llm_calls=int(os.getenv("CORE_MAX_LLM_CALLS", "10")),
        max_agent_hops=int(os.getenv("CORE_MAX_AGENT_HOPS", "21")),
        enable_telemetry=_parse_bool(os.getenv("CORE_ENABLE_TELEMETRY", "true")),
        enable_checkpoints=_parse_bool(os.getenv("CORE_ENABLE_CHECKPOINTS", "false"), default=False),
        debug_mode=_parse_bool(os.getenv("CORE_DEBUG_MODE", "false"), default=False),
        context_bounds=ContextBounds(
            max_input_tokens=int(os.getenv("CORE_MAX_INPUT_TOKENS", "4096")),
            max_output_tokens=int(os.getenv("CORE_MAX_OUTPUT_TOKENS", "2048")),
            max_context_tokens=int(os.getenv("CORE_MAX_CONTEXT_TOKENS", "16384")),
            reserved_tokens=int(os.getenv("CORE_RESERVED_TOKENS", "512")),
        ),
    )


def create_orchestration_flags_from_env() -> OrchestrationFlags:
    """Create OrchestrationFlags from environment variables.

    Environment Variables:
        ORCH_ENABLE_PARALLEL_AGENTS: Enable parallel agent execution
        ORCH_ENABLE_CHECKPOINTS: Enable checkpoint creation
        ORCH_ENABLE_DISTRIBUTED: Enable distributed mode
        ORCH_ENABLE_TELEMETRY: Enable telemetry
        ORCH_MAX_CONCURRENT_AGENTS: Max concurrent agents
        ORCH_CHECKPOINT_INTERVAL: Checkpoint interval in seconds

    Returns:
        OrchestrationFlags with environment-parsed values
    """
    return OrchestrationFlags(
        enable_parallel_agents=_parse_bool(os.getenv("ORCH_ENABLE_PARALLEL_AGENTS", "false"), default=False),
        enable_checkpoints=_parse_bool(os.getenv("ORCH_ENABLE_CHECKPOINTS", "false"), default=False),
        enable_distributed=_parse_bool(os.getenv("ORCH_ENABLE_DISTRIBUTED", "false"), default=False),
        enable_telemetry=_parse_bool(os.getenv("ORCH_ENABLE_TELEMETRY", "true")),
        max_concurrent_agents=int(os.getenv("ORCH_MAX_CONCURRENT_AGENTS", "4")),
        checkpoint_interval_seconds=int(os.getenv("ORCH_CHECKPOINT_INTERVAL", "30")),
    )


def core_config_to_resource_quota(core_config: CoreConfig) -> ResourceQuota:
    """Convert CoreConfig to Control Tower ResourceQuota.

    Control Tower's ResourceQuota is the canonical source for resource limits.
    This adapter ensures consistency with legacy CoreConfig values.

    Args:
        core_config: CoreConfig from environment/config

    Returns:
        ResourceQuota for Control Tower
    """
    return ResourceQuota(
        max_input_tokens=core_config.context_bounds.max_input_tokens,
        max_output_tokens=core_config.context_bounds.max_output_tokens,
        max_context_tokens=core_config.context_bounds.max_context_tokens,
        max_llm_calls=core_config.max_llm_calls,
        max_tool_calls=50,  # Default, not in CoreConfig
        max_agent_hops=core_config.max_agent_hops,
        max_iterations=core_config.max_iterations,
        timeout_seconds=300,  # Default timeout
        soft_timeout_seconds=240,
    )


def create_app_context(
    settings: Optional[Settings] = None,
    feature_flags: Optional[FeatureFlags] = None,
    core_config: Optional[CoreConfig] = None,
    orchestration_flags: Optional[OrchestrationFlags] = None,
) -> AppContext:
    """Create AppContext once per process.

    COMPOSITION ROOT: This is the ONLY place where concrete
    implementations are instantiated and wired together.

    Args:
        settings: Optional pre-configured settings. Uses get_settings() if None.
        feature_flags: Optional pre-configured flags. Uses get_feature_flags() if None.
        core_config: Optional CoreConfig. Parses from env if None.
        orchestration_flags: Optional OrchestrationFlags. Parses from env if None.

    Returns:
        AppContext with all dependencies wired.

    Usage:
        # At application startup
        app_context = create_app_context()

        # Or with custom config
        app_context = create_app_context(
            core_config=CoreConfig(max_iterations=5),
        )
    """
    # Build settings (from env/files)
    if settings is None:
        settings = get_settings()

    # Build feature flags
    if feature_flags is None:
        feature_flags = get_feature_flags()

    # Build core config from environment
    if core_config is None:
        core_config = create_core_config_from_env()

    # Build orchestration flags from environment
    if orchestration_flags is None:
        orchestration_flags = create_orchestration_flags_from_env()

    # Configure logging based on settings
    configure_logging(
        level=settings.log_level,
        json_output=True,
        enable_otel=feature_flags.enable_tracing,
    )

    # Build root logger (avionics provides structlog wrapper)
    from jeeves_avionics.logging import create_logger
    root_logger = create_logger("jeeves")

    # Get default service from capability registry (layer extraction support, Avionics R4)
    capability_registry = get_capability_resource_registry()
    default_service = capability_registry.get_default_service() or "jeeves"

    # Initialize OpenTelemetry if tracing is enabled
    otel_adapter = None
    if feature_flags.enable_tracing:
        from jeeves_avionics.observability.otel_adapter import (
            init_global_otel,
            get_global_otel_adapter,
        )
        # Initialize global OTEL adapter with service name from registry
        # Note: For production, configure exporter via OTEL_EXPORTER_JAEGER_ENDPOINT
        init_global_otel(
            service_name=default_service,
            service_version="1.0.0",
            exporter=None,  # Uses ConsoleSpanExporter by default
        )
        otel_adapter = get_global_otel_adapter()
        root_logger.info(
            "otel_initialized",
            service_name=default_service,
            enabled=otel_adapter is not None and otel_adapter.enabled,
        )

    # Build Control Tower with resource quota from CoreConfig
    default_quota = core_config_to_resource_quota(core_config)
    control_tower_logger = create_logger("control_tower")

    control_tower = ControlTower(
        logger=control_tower_logger,
        default_quota=default_quota,
        default_service=default_service,  # From capability registry
        otel_adapter=otel_adapter,  # Pass OTEL adapter for distributed tracing
    )

    root_logger.info(
        "control_tower_created",
        default_service=default_service,
        max_llm_calls=default_quota.max_llm_calls,
        max_iterations=default_quota.max_iterations,
        max_agent_hops=default_quota.max_agent_hops,
    )

    return AppContext(
        settings=settings,
        feature_flags=feature_flags,
        logger=root_logger,
        clock=SystemClock(),
        config_registry=None,
        core_config=core_config,
        orchestration_flags=orchestration_flags,
        vertical_registry={},
        control_tower=control_tower,
    )


def create_avionics_dependencies(
    app_context: AppContext,
    language_config: Optional["LanguageConfigProtocol"] = None,
    node_profiles: Optional["NodeProfilesProtocol"] = None,
    access_checker: Optional["AgentToolAccessProtocol"] = None,
):
    """Create and inject dependencies into avionics layer.

    This function sets up the dependency injection for avionics
    components that need capability-owned implementations.

    ADR-001 Decision 1: Layer Violation Resolution via Protocol Injection

    Args:
        app_context: The AppContext with core dependencies
        language_config: Optional LanguageConfigProtocol implementation
        node_profiles: Optional NodeProfilesProtocol implementation
        access_checker: Optional AgentToolAccessProtocol implementation

    Returns:
        Dict with created dependencies (for use by capability layer)
    """
    from jeeves_avionics.llm.factory import LLMFactory

    # Create LLM factory with node profiles for distributed routing
    llm_factory = LLMFactory(
        settings=app_context.settings,
        node_profiles=node_profiles,
    )

    # Initialize LLM Gateway if feature flag is enabled
    llm_gateway = None
    if app_context.feature_flags.use_llm_gateway:
        from jeeves_avionics.llm.gateway import LLMGateway
        from jeeves_avionics.logging import create_logger

        gateway_logger = create_logger("llm_gateway")

        # Configure fallback providers based on primary provider
        fallback_providers = []
        primary_provider = app_context.settings.llm_provider
        if primary_provider == "llamaserver":
            fallback_providers = ["openai"]  # Fallback to cloud if local fails
        elif primary_provider == "openai":
            fallback_providers = ["anthropic"]  # Fallback between cloud providers

        llm_gateway = LLMGateway(
            settings=app_context.settings,
            fallback_providers=fallback_providers,
            logger=gateway_logger,
        )

        # Wire resource tracking callback to Control Tower if available
        if app_context.control_tower is not None:
            def create_resource_callback(control_tower: ControlTower):
                """Create a resource tracking callback for the gateway.

                Uses the request_pid_context ContextVar to get the current
                request's PID for per-request resource tracking.

                Args:
                    control_tower: Control Tower instance for resource tracking

                Returns:
                    Callback function that records LLM usage and checks quota
                """
                def track_resources(tokens_in: int, tokens_out: int) -> Optional[str]:
                    # Get current process ID from request context
                    pid = get_request_pid()
                    if pid is None:
                        # No request context - allow but don't track
                        return None

                    # Record the LLM call and check quota
                    return control_tower.record_llm_call(
                        pid=pid,
                        tokens_in=tokens_in,
                        tokens_out=tokens_out,
                    )
                return track_resources

            llm_gateway.set_resource_callback(create_resource_callback(app_context.control_tower))

        gateway_logger.info(
            "llm_gateway_initialized",
            primary_provider=primary_provider,
            fallback_providers=fallback_providers,
            resource_tracking_enabled=app_context.control_tower is not None,
        )

    return {
        "llm_factory": llm_factory,
        "llm_gateway": llm_gateway,
        "language_config": language_config,
        "node_profiles": node_profiles,
        "access_checker": access_checker,
    }


def create_tool_executor_with_access(
    tool_registry,
    app_context: AppContext,
    access_checker: Optional["AgentToolAccessProtocol"] = None,
):
    """Create ToolExecutor with access control.

    Args:
        tool_registry: ToolRegistryProtocol implementation
        app_context: AppContext for logger
        access_checker: Optional AgentToolAccessProtocol for access control

    Returns:
        ToolExecutor configured with access control
    """
    from jeeves_avionics.wiring import ToolExecutor

    return ToolExecutor(
        registry=tool_registry,
        logger=app_context.get_bound_logger("tool_executor"),
        access_checker=access_checker,
    )


def create_distributed_infrastructure(
    app_context: AppContext,
    redis_url: Optional[str] = None,
    postgres_client: Optional[Any] = None,
) -> Dict[str, Any]:
    """Create distributed infrastructure when enable_distributed_mode is true.

    Creates Redis client and RedisDistributedBus for horizontal scaling.
    Optionally wires checkpoint adapter when enable_checkpoints is true.

    Args:
        app_context: AppContext with feature flags and logger
        redis_url: Optional Redis URL (uses REDIS_URL env var if not provided)
        postgres_client: Optional PostgreSQLClient for checkpoint persistence

    Returns:
        Dict containing:
        - redis_client: AsyncRedis client instance
        - distributed_bus: RedisDistributedBus instance
        - worker_coordinator: WorkerCoordinator instance
        - checkpoint_adapter: PostgresCheckpointAdapter (if checkpoints enabled)

    Raises:
        RuntimeError: If distributed mode is enabled but Redis unavailable
    """
    from jeeves_avionics.logging import create_logger

    result: Dict[str, Any] = {
        "redis_client": None,
        "distributed_bus": None,
        "worker_coordinator": None,
        "checkpoint_adapter": None,
    }

    # Only create if distributed mode is enabled
    if not app_context.feature_flags.enable_distributed_mode:
        return result

    # Verify Redis state flag is also enabled (required dependency)
    if not app_context.feature_flags.use_redis_state:
        app_context.logger.warning(
            "distributed_mode_requires_redis",
            message="enable_distributed_mode requires use_redis_state=true",
        )
        return result

    # Get Redis URL from param or environment
    import os
    actual_redis_url = redis_url or os.getenv("REDIS_URL", "redis://localhost:6379")

    try:
        # Import Redis async client
        import redis.asyncio as redis_async
        from jeeves_avionics.distributed import RedisDistributedBus
        from jeeves_mission_system.services.worker_coordinator import WorkerCoordinator

        # Create async Redis client wrapper
        # Note: The actual client is created as a coroutine factory
        # since async operations can't be awaited at module import time
        class AsyncRedisWrapper:
            """Wrapper for async Redis client to match RedisDistributedBus interface."""

            def __init__(self, url: str):
                self._url = url
                self._client = None

            @property
            def redis(self):
                """Get the underlying Redis client, creating if needed."""
                if self._client is None:
                    self._client = redis_async.from_url(
                        self._url,
                        decode_responses=True,
                    )
                return self._client

            async def close(self):
                """Close the Redis connection."""
                if self._client:
                    await self._client.close()
                    self._client = None

        redis_client = AsyncRedisWrapper(actual_redis_url)
        result["redis_client"] = redis_client

        # Create distributed bus
        bus_logger = create_logger("distributed_bus")
        distributed_bus = RedisDistributedBus(
            redis_client=redis_client,
            logger=bus_logger,
        )
        result["distributed_bus"] = distributed_bus

        # Create checkpoint adapter if checkpoints enabled and postgres available
        checkpoint_adapter = None
        if app_context.feature_flags.enable_checkpoints:
            if postgres_client is not None:
                from jeeves_avionics.checkpoint import PostgresCheckpointAdapter

                checkpoint_logger = create_logger("checkpoint_adapter")
                checkpoint_adapter = PostgresCheckpointAdapter(
                    postgres_client=postgres_client,
                    logger=checkpoint_logger,
                )
                result["checkpoint_adapter"] = checkpoint_adapter

                app_context.logger.info(
                    "checkpoint_adapter_created",
                    message="PostgresCheckpointAdapter wired for distributed checkpointing",
                )
            else:
                app_context.logger.warning(
                    "checkpoint_adapter_skipped",
                    message="enable_checkpoints=true but no postgres_client provided",
                )

        # Create worker coordinator with Control Tower and checkpoint integration
        coordinator_logger = create_logger("worker_coordinator")
        worker_coordinator = WorkerCoordinator(
            distributed_bus=distributed_bus,
            checkpoint_adapter=checkpoint_adapter,
            runtime=None,  # Runtime provided separately per worker
            logger=coordinator_logger,
            control_tower=app_context.control_tower,
        )
        result["worker_coordinator"] = worker_coordinator

        app_context.logger.info(
            "distributed_infrastructure_created",
            redis_url=actual_redis_url.split("@")[-1],  # Redact credentials
            has_control_tower=app_context.control_tower is not None,
            has_checkpoint_adapter=checkpoint_adapter is not None,
        )

    except ImportError as e:
        app_context.logger.error(
            "distributed_import_error",
            error=str(e),
            message="Install redis package: pip install redis",
        )
        raise RuntimeError(
            f"Failed to import distributed dependencies: {e}. "
            "Install with: pip install redis"
        ) from e
    except Exception as e:
        app_context.logger.error(
            "distributed_init_error",
            error=str(e),
            redis_url=actual_redis_url.split("@")[-1],
        )
        raise RuntimeError(f"Failed to create distributed infrastructure: {e}") from e

    return result


async def create_memory_manager(
    app_context: AppContext,
    postgres_client: Optional[Any] = None,
    vector_adapter: Optional[Any] = None,
) -> Optional[Any]:
    """Create MemoryManager facade for unified memory operations.

    Wires the MemoryManager with SQL adapter, vector adapter, and cross-ref manager.
    This is the unified memory interface per Memory Module Constitution.

    Args:
        app_context: AppContext with logger and settings
        postgres_client: PostgreSQL client for SQL operations
        vector_adapter: Vector storage adapter (pgvector) for semantic search

    Returns:
        MemoryManager instance, or None if dependencies unavailable
    """
    if postgres_client is None:
        app_context.logger.warning(
            "memory_manager_skipped",
            message="No postgres_client provided - MemoryManager requires database",
        )
        return None

    try:
        from jeeves_memory_module.manager import MemoryManager
        from jeeves_memory_module.adapters.sql_adapter import SQLAdapter
        from jeeves_memory_module.services.xref_manager import CrossRefManager
        from jeeves_avionics.logging import create_logger

        memory_logger = create_logger("memory_manager")

        # Create SQL adapter wrapping the postgres client
        sql_adapter = SQLAdapter(db_client=postgres_client, logger=memory_logger)

        # Create cross-reference manager
        xref_manager = CrossRefManager(db_client=postgres_client, logger=memory_logger)

        # Create MemoryManager facade
        memory_manager = MemoryManager(
            sql_adapter=sql_adapter,
            vector_adapter=vector_adapter,
            xref_manager=xref_manager,
            logger=memory_logger,
        )

        app_context.logger.info(
            "memory_manager_created",
            has_vector_adapter=vector_adapter is not None,
        )

        return memory_manager

    except ImportError as e:
        app_context.logger.error(
            "memory_manager_import_error",
            error=str(e),
        )
        return None
    except Exception as e:
        app_context.logger.error(
            "memory_manager_init_error",
            error=str(e),
        )
        return None


__all__ = [
    "create_app_context",
    "create_avionics_dependencies",
    "create_tool_executor_with_access",
    "create_core_config_from_env",
    "create_orchestration_flags_from_env",
    "core_config_to_resource_quota",
    # Distributed infrastructure
    "create_distributed_infrastructure",
    # Memory management
    "create_memory_manager",
    # Per-request PID context for resource tracking
    "set_request_pid",
    "clear_request_pid",
    "get_request_pid",
    "request_pid_context",
]
