"""Feature flags for phased rollout of new capabilities.

This module provides runtime toggles for new features being developed
across the phased implementation roadmap. Feature flags allow us to:
- Deploy code without immediately enabling it
- Test new features in production with gradual rollout
- Quick rollback by toggling flags without code changes
- A/B testing and canary deployments

Orchestrator config (CyclicConfig, ContextBoundsConfig) lives in
jeeves_core.config.orchestrator — not here.
"""

from pydantic import model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict
from typing import Optional, TYPE_CHECKING

if TYPE_CHECKING:
    from jeeves_core.protocols import LoggerProtocol


class FeatureFlags(BaseSettings):
    """Runtime toggles for new features across implementation phases.

    All flags default to False (disabled) for safety. Enable via environment
    variables (FEATURE_USE_REDIS_STATE=true) or in .env file.

    Phase ownership:
    - Phase 1: use_redis_state
    - Phase 3: enable_distributed_mode
    - Phase 4: enable_tracing
    """

    # Phase 1: Redis State
    use_redis_state: bool = False
    """Enable Redis for shared state (rate limiting, locks, cache).

    When enabled:
    - Rate limiter uses Redis (distributed-safe)
    - Embedding cache stored in Redis
    - Distributed locks available

    Requires: REDIS_URL environment variable
    Rollback: Set to False to use in-memory state
    """

    # Phase 3: LAN Deployment + Node Registry
    enable_distributed_mode: bool = False
    """Enable multi-node deployment across local network.

    When enabled:
    - LLM requests routed to GPU nodes based on model availability
    - Node registry tracks available hardware
    - Automatic failover on node failure

    Requires: REDIS_URL for node coordination
    Rollback: Set to False to use single-node mode
    """

    # Phase 4: Observability + Production Hardening
    enable_tracing: bool = False
    """Enable OpenTelemetry distributed tracing.

    When enabled:
    - Full request traces with per-agent spans
    - Cost and latency metrics attached to spans
    - Export to Jaeger/OTLP endpoint

    Requires: OTEL_EXPORTER_JAEGER_ENDPOINT environment variable
    Rollback: Set to False for zero tracing overhead
    """

    # Development/Testing flags
    enable_debug_logging: bool = False
    """Enable verbose debug logging for development.

    When enabled:
    - Log level set to DEBUG for all components
    - Additional diagnostic information in logs
    - Performance impact: ~10% latency increase
    """

    # Phase 2.3: Agent reasoning observability (config-gated)
    emit_agent_reasoning: bool = False
    """Enable emission of agent reasoning/CoT events.

    When enabled:
    - Agent reasoning text is emitted as observable events
    - Truncated to max 500 chars for streaming efficiency
    - Lives entirely in mission/infrastructure layer (no core changes)

    Rollback: Set to False to disable reasoning emission
    """

    model_config = SettingsConfigDict(
        env_prefix="FEATURE_",
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore"
    )

    def log_status(self, logger: "LoggerProtocol") -> None:
        """Log current feature flag status using structured logging."""
        logger.info(
            "feature_flags_status",
            use_redis_state=self.use_redis_state,
            enable_distributed_mode=self.enable_distributed_mode,
            enable_tracing=self.enable_tracing,
            enable_debug_logging=self.enable_debug_logging,
            emit_agent_reasoning=self.emit_agent_reasoning,
        )

    @model_validator(mode="after")
    def validate_dependencies(self) -> "FeatureFlags":
        """Validate that dependent features are properly enabled."""
        errors = []

        if self.enable_distributed_mode and not self.use_redis_state:
            errors.append(
                "enable_distributed_mode requires use_redis_state=True "
                "(Redis needed for node coordination)"
            )

        if errors:
            raise ValueError(
                "Feature flag dependency errors:\n"
                + "\n".join(f"  - {e}" for e in errors)
            )

        return self


# Lazy initialization - no module-level instantiation (ADR-001 compliance)
_feature_flags: Optional[FeatureFlags] = None


def get_feature_flags() -> FeatureFlags:
    """Get the global feature flags instance.

    Creates a new FeatureFlags instance lazily if none exists.
    Prefer dependency injection over this global getter for testability.
    """
    global _feature_flags
    if _feature_flags is None:
        _feature_flags = FeatureFlags()
    return _feature_flags


def set_feature_flags(flags: FeatureFlags) -> None:
    """Set the global feature flags instance.

    Use at bootstrap time to inject pre-configured flags.
    """
    global _feature_flags
    _feature_flags = flags


def reset_feature_flags() -> None:
    """Reset the global feature flags instance.

    Forces re-creation on next get_feature_flags() call.
    """
    global _feature_flags
    _feature_flags = None
