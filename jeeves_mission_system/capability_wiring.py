"""Capability Wiring - Register capabilities with CapabilityResourceRegistry.

This module is the capability registration entry point. It registers
capabilities (services, orchestrators, tools, agents) with the
CapabilityResourceRegistry before the API server starts.

The mission system queries the registry at runtime to:
- Get service configuration (is_readonly, requires_confirmation, etc.)
- Create orchestrators via registered factories
- Initialize tools via registered initializers
- Load prompts and agents

Usage:
    # In API server startup (before lifespan)
    from jeeves_mission_system.capability_wiring import wire_capabilities

    wire_capabilities()

    # Now registry is populated and server can query it

Constitutional Alignment:
- Avionics R4: Capabilities register their own resources
- No hardcoded capability knowledge in mission system
"""

from typing import Any, Callable, Dict, Optional

from jeeves_protocols import (
    get_capability_resource_registry,
    DomainServiceConfig,
    CapabilityOrchestratorConfig,
    CapabilityToolsConfig,
    CapabilityServicerProtocol,
)


# =============================================================================
# CODE ANALYSIS CAPABILITY REGISTRATION
# =============================================================================


def _create_code_analysis_orchestrator_factory() -> Callable[..., Any]:
    """Create the orchestrator factory for code_analysis capability.

    The factory is a callable that creates the orchestrator/servicer
    implementing CapabilityServicerProtocol.

    Returns:
        Factory function that creates the orchestrator
    """
    def factory(
        llm_provider_factory: Callable,
        tool_executor: Any,
        logger: Any,
        persistence: Any,
        control_tower: Any,
    ) -> Any:
        """Create code_analysis orchestrator.

        This factory is called by the mission system to create the
        orchestrator that handles code analysis requests.

        The returned object must:
        1. Implement CapabilityServicerProtocol (process_request method)
        2. Have get_dispatch_handler() method for Control Tower registration
        """
        # Import here to avoid circular imports
        # This is the only place that imports from capability layer
        try:
            from jeeves_mission_system.orchestration.service import (
                CodeAnalysisService,
            )

            return CodeAnalysisService(
                llm_provider_factory=llm_provider_factory,
                tool_executor=tool_executor,
                logger=logger,
                persistence=persistence,
                control_tower=control_tower,
            )
        except ImportError:
            # Capability layer not available - return mock for testing
            logger.warning(
                "code_analysis_import_failed",
                message="CodeAnalysisService not available, using mock",
            )
            return _create_mock_orchestrator(logger)

    return factory


def _create_mock_orchestrator(logger: Any) -> Any:
    """Create a mock orchestrator for testing when capability not available."""

    class MockOrchestrator:
        """Mock orchestrator implementing minimal interface."""

        def __init__(self, logger: Any):
            self._logger = logger

        async def process_request(
            self,
            user_id: str,
            session_id: Optional[str],
            message: str,
            context: Optional[dict],
        ):
            """Mock process_request that yields a single completion event."""
            self._logger.info(
                "mock_orchestrator_request",
                user_id=user_id,
                message_preview=message[:50] if message else "",
            )
            yield {
                "type": "completed",
                "response": "Mock response - CodeAnalysisService not available",
            }

        def get_dispatch_handler(self) -> Callable:
            """Return dispatch handler for Control Tower."""
            async def handler(envelope: Any) -> Any:
                self._logger.info("mock_dispatch_handler", envelope_id=getattr(envelope, 'envelope_id', None))
                return envelope
            return handler

    return MockOrchestrator(logger)


def _create_code_analysis_tools_initializer() -> Callable[..., Dict[str, Any]]:
    """Create the tools initializer for code_analysis capability.

    Returns:
        Initializer function that creates tool instances
    """
    def initializer(db: Any) -> Dict[str, Any]:
        """Initialize code_analysis tools.

        Args:
            db: Database client for tools that need persistence

        Returns:
            Dict with 'catalog' key containing ToolRegistryProtocol
        """
        try:
            from jeeves_avionics.tools.catalog import create_tool_catalog

            catalog = create_tool_catalog(db=db)
            return {"catalog": catalog}
        except ImportError:
            # Return empty catalog
            return {"catalog": None}

    return initializer


def wire_code_analysis_capability() -> None:
    """Register the code_analysis capability with all required configuration.

    This registers:
    - Service configuration with new decoupling fields
    - Orchestrator factory
    - Tools initializer
    """
    registry = get_capability_resource_registry()

    # Register service configuration with ALL new fields
    registry.register_service(
        capability_id="code_analysis",
        service_config=DomainServiceConfig(
            service_id="code_analysis",
            service_type="flow",
            capabilities=["analyze", "search", "explain", "clarification"],
            max_concurrent=10,
            is_default=True,
            # NEW DECOUPLING FIELDS
            is_readonly=True,  # Code analysis doesn't modify files
            requires_confirmation=False,  # No confirmation dialogs needed
            default_session_title="Code Analysis",  # Session title prefix
            pipeline_stages=[  # Agent pipeline stages for observability
                "perception",
                "intent",
                "planner",
                "traverser",
                "critic",
                "integration",
            ],
        ),
    )

    # Register orchestrator factory
    registry.register_orchestrator(
        capability_id="code_analysis",
        config=CapabilityOrchestratorConfig(
            factory=_create_code_analysis_orchestrator_factory(),
            result_type=None,  # Uses generic dict result
        ),
    )

    # Register tools initializer
    registry.register_tools(
        capability_id="code_analysis",
        config=CapabilityToolsConfig(
            initializer=_create_code_analysis_tools_initializer(),
        ),
    )


# =============================================================================
# MAIN WIRING FUNCTION
# =============================================================================


def wire_capabilities() -> None:
    """Wire all capabilities with the CapabilityResourceRegistry.

    Call this during application startup before the API server
    queries the registry.

    Currently registers:
    - code_analysis: Read-only code analysis capability

    Future capabilities would be added here:
    - code_generation: Write capability with confirmation
    - code_review: Read-only with different pipeline
    """
    wire_code_analysis_capability()

    # Log registration
    registry = get_capability_resource_registry()
    capabilities = registry.list_capabilities()

    # Get logger if available
    try:
        from jeeves_avionics.logging import create_logger
        logger = create_logger("capability_wiring")
        logger.info(
            "capabilities_wired",
            capabilities=capabilities,
            default_service=registry.get_default_service(),
        )
    except ImportError:
        pass  # No logging available


__all__ = [
    "wire_capabilities",
    "wire_code_analysis_capability",
]
