"""Capability Wiring - Register capabilities with CapabilityResourceRegistry.

This module is the capability registration entry point. External capability
packages call register functions here to wire themselves into the infrastructure.

The infrastructure queries the registry at runtime to:
- Get service configuration (is_readonly, requires_confirmation, etc.)
- Create orchestrators via registered factories
- Initialize tools via registered initializers
- Load prompts and agents

Usage:
    from jeeves_core.capability_wiring import register_capability

    register_capability(
        capability_id="my_capability",
        service_config=DomainServiceConfig(...),
        mode_config=DomainModeConfig(...),
        tools_config=CapabilityToolsConfig(initializer=my_catalog_fn),
        service_class=MyService,
        pipeline_config=MY_PIPELINE,
    )

    # Or use environment-based auto-discovery:
    wire_capabilities()  # Discovers and registers capabilities

Constitutional Alignment:
- Constitution R4: Capabilities register their own resources
- No hardcoded capability knowledge in infrastructure
- No backward compat: one path, no dual fields, no shims
"""

import os
from typing import Any, Callable, Dict, List, Optional

from jeeves_core.protocols import (
    get_capability_resource_registry,
    DomainServiceConfig,
    DomainModeConfig,
    DomainAgentConfig,
    CapabilityOrchestratorConfig,
    CapabilityToolsConfig,
    AgentLLMConfig,
    PipelineConfig,
)


def register_capability(
    capability_id: str,
    service_config: DomainServiceConfig,
    *,
    mode_config: Optional[DomainModeConfig] = None,
    agents: Optional[List[DomainAgentConfig]] = None,
    tools_config: Optional[CapabilityToolsConfig] = None,
    agent_llm_configs: Optional[Dict[str, AgentLLMConfig]] = None,
    orchestrator_config: Optional[CapabilityOrchestratorConfig] = None,
    service_class: Optional[type] = None,
    pipeline_config: Optional[PipelineConfig] = None,
    service_kwargs: Optional[Dict[str, Any]] = None,
) -> Optional[Callable]:
    """Register a capability with all required configuration in one call.

    This is the main entry point for capability registration. External
    capability packages call this to wire themselves into the infrastructure.

    Orchestrator registration (mutually exclusive):
    - orchestrator_config: Custom factory for non-CapabilityService orchestrators
    - service_class + pipeline_config: Auto-generates factory for CapabilityService subclasses

    Args:
        capability_id: Unique identifier for the capability
        service_config: Service configuration with behavior metadata
        mode_config: Optional mode configuration
        agents: Optional agent definitions for governance
        tools_config: Optional tools configuration
        agent_llm_configs: Optional dict of agent_name -> AgentLLMConfig
        orchestrator_config: Custom orchestrator factory config
        service_class: CapabilityService subclass for auto factory
        pipeline_config: PipelineConfig (required with service_class)
        service_kwargs: Extra kwargs passed to service_class.__init__

    Returns:
        create_from_app_context factory if service_class provided, else None.

    Raises:
        ValueError: If both orchestrator_config and service_class are provided.
    """
    if orchestrator_config is not None and service_class is not None:
        raise ValueError(
            "Cannot provide both orchestrator_config and service_class. "
            "Use orchestrator_config for custom factories, or "
            "service_class + pipeline_config for CapabilityService subclasses."
        )

    registry = get_capability_resource_registry()

    # 1. Register mode if provided
    if mode_config is not None:
        registry.register_mode(capability_id, mode_config)

    # 2. Register service configuration
    registry.register_service(
        capability_id=capability_id,
        service_config=service_config,
    )

    # 3. Register agent definitions if provided
    if agents is not None:
        registry.register_agents(capability_id, agents)

    # 4. Register tools
    if tools_config is not None:
        registry.register_tools(capability_id=capability_id, config=tools_config)

    # 5. Register orchestrator
    if orchestrator_config is not None:
        # Custom factory (non-CapabilityService orchestrators)
        registry.register_orchestrator(
            capability_id=capability_id,
            config=orchestrator_config,
        )
    elif service_class is not None and pipeline_config is not None:
        # Auto-generated factory for CapabilityService subclasses
        _extra = service_kwargs or {}

        def _auto_factory(llm_factory, tool_executor, log, persistence, kernel_client):
            return service_class(
                llm_provider_factory=llm_factory,
                tool_executor=tool_executor,
                logger=log,
                pipeline_config=pipeline_config,
                kernel_client=kernel_client,
                persistence=persistence,
                **_extra,
            )

        registry.register_orchestrator(
            capability_id=capability_id,
            config=CapabilityOrchestratorConfig(factory=_auto_factory),
        )

    # 6. Register LLM configurations if provided
    if agent_llm_configs:
        from jeeves_core.capability_registry import get_capability_registry

        llm_registry = get_capability_registry()
        for agent_name, config in agent_llm_configs.items():
            llm_registry.register(
                capability_id=capability_id,
                agent_name=agent_name,
                config=config,
            )

    # Return create_from_app_context factory if service_class provided
    if service_class is not None and pipeline_config is not None:
        def create_from_app_context(app_context):
            from jeeves_core.wiring import create_tool_executor

            tools_cfg = registry.get_tools(capability_id)
            tool_catalog = tools_cfg.get_catalog() if tools_cfg else None
            tool_executor = create_tool_executor(tool_catalog) if tool_catalog else None

            return service_class(
                llm_provider_factory=app_context.llm_provider_factory,
                tool_executor=tool_executor,
                logger=app_context.logger,
                pipeline_config=pipeline_config,
                kernel_client=app_context.kernel_client,
                persistence=app_context.db,
                **(service_kwargs or {}),
            )

        return create_from_app_context

    return None


def get_agent_config(capability_id: str, agent_name: str) -> AgentLLMConfig:
    """Registry-backed lookup for agent LLM configuration.

    Args:
        capability_id: Capability that owns the agent.
        agent_name: Agent name to look up.

    Returns:
        AgentLLMConfig for the agent.

    Raises:
        KeyError: If agent not found in registry.
    """
    from jeeves_core.capability_registry import get_capability_registry

    registry = get_capability_registry()
    config = registry.get_agent_config(agent_name)
    if config is None:
        raise KeyError(f"Agent '{agent_name}' not found in capability '{capability_id}'")
    return config


def _discover_capabilities() -> List[str]:
    """Discover capabilities from environment or entry points.

    Returns list of capability module paths to import.
    """
    # Check environment for explicit capability list
    capabilities_env = os.getenv("AIRFRAME_CAPABILITIES", "")
    if capabilities_env:
        return [c.strip() for c in capabilities_env.split(",") if c.strip()]

    # No hardcoded fallbacks — env-only discovery.
    # Set AIRFRAME_CAPABILITIES to register capabilities.
    return []


def _try_import_capability(module_path: str) -> bool:
    """Try to import and register a capability module.

    The module should call register_capability() when imported.
    Module path must be a dotted path (e.g. 'game_capability.wiring'),
    blocking bare stdlib imports like 'os' or 'subprocess'.

    Returns:
        True if successfully imported, False otherwise.
    """
    if "." not in module_path:
        raise ValueError(
            f"Capability module path must be dotted (got {module_path!r}). "
            f"Example: 'game_capability.wiring'"
        )
    try:
        __import__(module_path)
        return True
    except ImportError:
        return False


def wire_capabilities() -> None:
    """Discover and wire all capabilities with the CapabilityResourceRegistry.

    Call this during application startup before the API server
    queries the registry.

    Discovery: AIRFRAME_CAPABILITIES environment variable (comma-separated module paths).
    Fails loud if no capabilities are discovered.
    """
    discovered = _discover_capabilities()
    registered = []

    for module_path in discovered:
        if _try_import_capability(module_path):
            registered.append(module_path)

    if not registered:
        raise RuntimeError(
            "No capabilities registered. Set AIRFRAME_CAPABILITIES env var "
            "to a comma-separated list of capability wiring module paths."
        )

    # Log registration
    registry = get_capability_resource_registry()
    capabilities = registry.list_capabilities()

    try:
        from jeeves_core.logging import create_logger
        logger = create_logger("capability_wiring")
        logger.info(
            "capabilities_wired",
            discovered=discovered,
            registered=registered,
            capabilities=capabilities,
            default_service=registry.get_default_service(),
        )
    except ImportError:
        pass


def wire_infra_routers(app_context) -> None:
    """Register infrastructure API routers (governance, etc.).

    These are infrastructure concerns (P6 observability) not owned by any capability.
    They use the same registry mechanism as capability routers.

    Args:
        app_context: AppContext with tool_health_service for governance DI.
    """
    from jeeves_core.gateway.routers.governance import router as gov_router, get_tool_health_service

    registry = get_capability_resource_registry()

    def _create_governance_deps(db, event_manager, orchestrator, **kwargs):
        return {get_tool_health_service: lambda: app_context.tool_health_service}

    registry.register_api_router(
        "_infra_governance",
        gov_router,
        deps_factory=_create_governance_deps,
        feature_flag=None,
    )


__all__ = [
    "register_capability",
    "get_agent_config",
    "wire_capabilities",
    "wire_infra_routers",
]
