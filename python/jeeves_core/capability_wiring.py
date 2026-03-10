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

import logging
import os
from importlib.metadata import entry_points as _entry_points
from typing import Any, Callable, Dict, List, Optional

from jeeves_core.logging import create_logger
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

logger = logging.getLogger("jeeves_core.capability_wiring")


# =============================================================================
# PYTHON-ONLY VALIDATION
# =============================================================================

def _validate_pipeline_config(pipeline_config: PipelineConfig) -> None:
    """Validate Python-only pipeline config fields at registration time.

    Kernel-side validation (routing targets, parallel groups, bounds) is
    handled by Rust PipelineConfig::validate(). This checks only fields
    that stay in Python and never cross IPC.
    """
    errors = []

    # 1. Tool dispatch consistency (Python-only dispatch, not in Rust)
    for agent in pipeline_config.agents:
        if agent.tool_dispatch and not agent.tool_source_agent:
            errors.append(
                f"Stage '{agent.name}': tool_dispatch='{agent.tool_dispatch}' "
                f"requires tool_source_agent"
            )
        if agent.tool_source_agent:
            all_output_keys = {a.output_key for a in pipeline_config.agents}
            if agent.tool_source_agent not in all_output_keys:
                errors.append(
                    f"Stage '{agent.name}': tool_source_agent='{agent.tool_source_agent}' "
                    f"not found in output_keys {sorted(all_output_keys)}"
                )

    if errors:
        raise ValueError(
            f"Pipeline '{pipeline_config.name}' validation failed:\n"
            + "\n".join(f"  - {e}" for e in errors)
        )

    # 2. Warnings (non-fatal, logged)
    seen_keys: set[str] = set()
    for agent in pipeline_config.agents:
        if agent.output_key in seen_keys:
            logger.warning(
                "Stage '%s': duplicate output_key '%s' — "
                "ensure these stages are mutually exclusive (conditional routing), "
                "not concurrent. Concurrent writers to the same envelope slot "
                "cause data races.",
                agent.name, agent.output_key,
            )
        seen_keys.add(agent.output_key)

    for agent in pipeline_config.agents:
        if agent.has_llm and not agent.prompt_key:
            logger.warning(
                "Stage '%s': has_llm=True but no prompt_key — "
                "agent will receive no system prompt",
                agent.name,
            )


# =============================================================================
# CONVENIENCE DEFAULTS
# =============================================================================

def _default_agent_metadata(pipeline_config: PipelineConfig) -> list[DomainAgentConfig]:
    """Generate minimal governance metadata from pipeline stages.

    Returns basic DomainAgentConfig entries with neutral defaults.
    Capabilities wanting descriptions, layer semantics, or tool lists
    should pass agents= explicitly to register_capability().
    """
    return [
        DomainAgentConfig(
            name=ac.name,
            description=ac.name,
            layer="agent",
            tools=sorted(ac.allowed_tools) if ac.allowed_tools else [],
        )
        for ac in pipeline_config.agents
    ]


def _default_llm_configs(pipeline_config: PipelineConfig) -> dict[str, AgentLLMConfig]:
    """Generate default LLM configs from pipeline agent configs.

    Uses AgentLLMConfig.from_env() for LLM agents (reads model from env),
    and a null config for non-LLM agents.
    """
    configs: dict[str, AgentLLMConfig] = {}
    for ac in pipeline_config.agents:
        if ac.has_llm:
            configs[ac.name] = AgentLLMConfig.from_env(
                ac.name,
                temperature=ac.temperature or 0.3,
                max_tokens=ac.max_tokens or 2000,
            )
        else:
            configs[ac.name] = AgentLLMConfig(
                agent_name=ac.name,
                model="",
                temperature=0.0,
                max_tokens=0,
                context_window=0,
                timeout_seconds=60,
            )
    return configs


# =============================================================================
# REGISTRATION
# =============================================================================

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
        ValueError: If both orchestrator_config and service_class are provided,
                    or if pipeline validation fails.
    """
    if orchestrator_config is not None and service_class is not None:
        raise ValueError(
            f"Capability '{capability_id}': cannot provide both orchestrator_config "
            f"and service_class. Use orchestrator_config for custom factories, or "
            f"service_class + pipeline_config for CapabilityService subclasses."
        )

    # Validate Python-only fields before any registry calls
    if pipeline_config is not None:
        _validate_pipeline_config(pipeline_config)

    # Apply convenience defaults
    if not service_config.pipeline_stages and pipeline_config is not None:
        service_config.pipeline_stages = [a.name for a in pipeline_config.agents]

    if agents is None and pipeline_config is not None:
        agents = _default_agent_metadata(pipeline_config)

    if agent_llm_configs is None and pipeline_config is not None:
        agent_llm_configs = _default_llm_configs(pipeline_config)

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


# =============================================================================
# DISCOVERY
# =============================================================================

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


def _discover_entry_points() -> list[tuple[str, Callable]]:
    """Discover capabilities via jeeves_core.capabilities entry points.

    Entry points are defined in pyproject.toml as:
        [project.entry-points."jeeves_core.capabilities"]
        hello_world = "jeeves_capability_hello_world.capability.wiring:register_capability"

    Returns:
        List of (name, callable) tuples.
    """
    eps = list(_entry_points(group="jeeves_core.capabilities"))
    result = []
    for ep in eps:
        try:
            func = ep.load()
            result.append((ep.name, func))
        except Exception:
            logger.exception(
                "Failed to load entry point '%s' (%s)",
                ep.name, ep.value,
            )
    return result


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
            f"Example: 'my_capability.wiring' or 'my_capability.capability.wiring'"
        )
    try:
        __import__(module_path)
        return True
    except ImportError:
        logger.exception(
            "Failed to import capability module '%s'",
            module_path,
        )
        return False


def wire_capabilities() -> None:
    """Discover and wire all capabilities with the CapabilityResourceRegistry.

    Call this during application startup before the API server
    queries the registry.

    Discovery (in order):
    1. AIRFRAME_CAPABILITIES env var (comma-separated dotted module paths, import-based)
    2. jeeves_core.capabilities entry points (callable-based, module:register_func)

    At least one mechanism must succeed.
    """
    registered = []

    # 1. AIRFRAME_CAPABILITIES env var (import-based)
    discovered = _discover_capabilities()
    for module_path in discovered:
        if _try_import_capability(module_path):
            registered.append(module_path)

    # 2. Entry points (callable-based)
    entry_points_found = _discover_entry_points()
    for ep_name, register_fn in entry_points_found:
        try:
            register_fn()
            registered.append(f"entry_point:{ep_name}")
        except Exception:
            logger.exception(
                "Entry point '%s' register function failed",
                ep_name,
            )

    if not registered:
        # Build a helpful error message
        if discovered and not entry_points_found:
            msg = (
                f"No capabilities registered. "
                f"AIRFRAME_CAPABILITIES listed {discovered} but none imported successfully. "
                f"Check module paths and installed packages."
            )
        elif not discovered and not entry_points_found:
            msg = (
                "No capabilities discovered. Set AIRFRAME_CAPABILITIES env var "
                "to a comma-separated list of capability wiring module paths, "
                "or install packages with jeeves_core.capabilities entry points."
            )
        else:
            msg = (
                f"No capabilities registered. "
                f"Discovered {len(discovered)} env modules and "
                f"{len(entry_points_found)} entry points, but all failed."
            )
        raise RuntimeError(msg)

    # Log registration
    registry = get_capability_resource_registry()
    capabilities = registry.list_capabilities()

    _wire_logger = create_logger("capability_wiring")
    _wire_logger.info(
        "capabilities_wired",
        discovered_env=discovered,
        discovered_entry_points=[name for name, _ in entry_points_found],
        registered=registered,
        capabilities=capabilities,
        default_service=registry.get_default_service(),
    )


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
