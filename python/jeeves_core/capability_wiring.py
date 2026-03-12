"""Capability Wiring - Register capabilities with CapabilityResourceRegistry.

This module is the capability registration entry point. External capability
packages call register functions here to wire themselves into the infrastructure.

PipelineConfig IS the deployment spec. register_capability(id, pipeline) is
sufficient — everything else is derived, like K8s deriving ReplicaSets from
a Deployment spec.

Usage:
    from jeeves_core.capability_wiring import register_capability

    register_capability(
        "my_capability",
        my_pipeline,
        service_class=MyService,
        is_default=True,
    )

    # Or use environment-based auto-discovery:
    wire_capabilities()  # Discovers and registers capabilities
"""

import logging
import os
from importlib.metadata import entry_points as _entry_points
from typing import Any, Callable, Dict, List, Optional, Union

from jeeves_core.logging import create_logger
from jeeves_core.protocols import (
    get_capability_resource_registry,
    DomainServiceConfig,
    ModeConfig,
    DomainAgentConfig,
    CapabilityOrchestratorConfig,
    ToolsConfig,
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

    # 2. Cross-reference validation (routing targets, Gate constraints)
    cross_ref_errors = pipeline_config.validate()
    if cross_ref_errors:
        raise ValueError(
            f"Pipeline '{pipeline_config.name}' validation failed:\n"
            + "\n".join(f"  - {e}" for e in cross_ref_errors)
        )

    # 3. Warnings (non-fatal, logged)
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

def _default_agent_metadata_from_agents(all_agents: list) -> list[DomainAgentConfig]:
    """Generate minimal governance metadata from collected agents.

    Returns basic DomainAgentConfig entries with neutral defaults.
    Layer is "llm" for LLM agents, "deterministic" otherwise — useful
    for governance health APIs. Capabilities wanting descriptions or
    custom layer semantics should pass agents= explicitly.
    """
    return [
        DomainAgentConfig(
            name=ac.name,
            description=ac.name,
            layer="llm" if ac.has_llm else "deterministic",
            tools=sorted(ac.allowed_tools) if ac.allowed_tools else [],
        )
        for ac in all_agents
    ]


def _default_llm_configs_from_agents(all_agents: list) -> dict[str, AgentLLMConfig]:
    """Generate default LLM configs from collected agents.

    Uses AgentLLMConfig.from_env() for LLM agents (reads model from env),
    and a null config for non-LLM agents.
    """
    configs: dict[str, AgentLLMConfig] = {}
    for ac in all_agents:
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

def _collect_all_agents(pipelines: List[PipelineConfig]) -> list:
    """Union of all agents across all pipelines, order-preserving dedup (first-seen wins)."""
    all_agents = []
    seen_names: set[str] = set()
    for p in pipelines:
        for a in p.agents:
            if a.name not in seen_names:
                all_agents.append(a)
                seen_names.add(a.name)
    return all_agents


def register_capability(
    capability_id: str,
    pipeline: Union[PipelineConfig, List[PipelineConfig]],
    *,
    # Orchestrator (choose one or neither)
    service_class: Optional[type] = None,
    orchestrator_factory: Optional[Callable] = None,
    service_kwargs: Optional[Dict[str, Any]] = None,

    # Gateway policy (real choices capabilities make)
    capabilities: Optional[List[str]] = None,
    is_default: bool = False,
    is_readonly: bool = True,
    requires_confirmation: bool = False,
    default_session_title: str = "Session",

    # Optional overrides (auto-derived from pipeline if not provided)
    tools_config: Optional[ToolsConfig] = None,
    mode_config: Optional[ModeConfig] = None,
    agents: Optional[List[DomainAgentConfig]] = None,
    agent_llm_configs: Optional[Dict[str, AgentLLMConfig]] = None,
) -> Optional[Callable]:
    """Register a capability with PipelineConfig as the deployment spec.

    PipelineConfig IS the spec. Everything else is derived — like K8s deriving
    ReplicaSets from a Deployment, or Temporal deriving task queues from
    workflow registration.

    Args:
        capability_id: Unique identifier for the capability.
        pipeline: PipelineConfig or list of PipelineConfigs (game-mvp has 4).

        service_class: CapabilityService subclass for auto factory.
        orchestrator_factory: Custom factory callable (mutually exclusive with service_class).
        service_kwargs: Extra kwargs passed to service_class.__init__.

        capabilities: Capability names (default: pipeline names).
        is_default: Whether this is the default service.
        is_readonly: Whether the service is read-only.
        requires_confirmation: Whether actions require user confirmation.
        default_session_title: Default session title.

        tools_config: Optional tools configuration.
        mode_config: Optional mode configuration.
        agents: Optional agent definitions (default: derived from pipeline stages).
        agent_llm_configs: Optional LLM configs (default: derived from pipeline stages).

    Returns:
        create_from_app_context factory if service_class provided, else None.
    """
    if orchestrator_factory is not None and service_class is not None:
        raise ValueError(
            f"Capability '{capability_id}': cannot provide both orchestrator_factory "
            f"and service_class. Use orchestrator_factory for custom factories, or "
            f"service_class for CapabilityService subclasses."
        )

    # Normalize to list
    pipelines = pipeline if isinstance(pipeline, list) else [pipeline]

    # Validate each pipeline
    for p in pipelines:
        _validate_pipeline_config(p)

    # Union of all agents across all pipelines (order-preserving dedup)
    all_agents = _collect_all_agents(pipelines)

    # Build DomainServiceConfig internally from kwargs
    # service_type and max_concurrent use defaults — runtime concerns pushed to Rust later
    service_config = DomainServiceConfig(
        service_id=f"{capability_id}_service",
        capabilities=capabilities or [p.name for p in pipelines],
        is_default=is_default,
        is_readonly=is_readonly,
        requires_confirmation=requires_confirmation,
        default_session_title=default_session_title,
    )

    # Derive DomainAgentConfig from stages if not provided
    if agents is None:
        agents = _default_agent_metadata_from_agents(all_agents)

    # Derive AgentLLMConfig from stages if not provided
    if agent_llm_configs is None:
        agent_llm_configs = _default_llm_configs_from_agents(all_agents)

    # Derive tool_ids from allowed_tools union if tools_config provided but empty
    if tools_config is not None and not tools_config.tool_ids:
        all_tools: set[str] = set()
        for a in all_agents:
            if a.allowed_tools:
                all_tools |= a.allowed_tools
        if all_tools:
            tools_config.tool_ids = sorted(all_tools)

    registry = get_capability_resource_registry()

    # 1. Register mode if provided
    if mode_config is not None:
        registry.register_mode(capability_id, mode_config)

    # 2. Register service configuration
    registry.register_service(
        capability_id=capability_id,
        service_config=service_config,
    )

    # 3. Register agent definitions
    if agents is not None:
        registry.register_agents(capability_id, agents)

    # 4. Register tools
    if tools_config is not None:
        registry.register_tools(capability_id=capability_id, config=tools_config)

    # 5. Register orchestrator
    # Use first pipeline as the "primary" for service_class factory
    primary_pipeline = pipelines[0]

    if orchestrator_factory is not None:
        registry.register_orchestrator(
            capability_id=capability_id,
            config=CapabilityOrchestratorConfig(factory=orchestrator_factory),
        )
    elif service_class is not None:
        _extra = service_kwargs or {}

        def _auto_factory(llm_factory, tool_executor, log, persistence, kernel_client):
            return service_class(
                llm_provider_factory=llm_factory,
                tool_executor=tool_executor,
                logger=log,
                pipeline_config=primary_pipeline,
                kernel_client=kernel_client,
                persistence=persistence,
                **_extra,
            )

        registry.register_orchestrator(
            capability_id=capability_id,
            config=CapabilityOrchestratorConfig(factory=_auto_factory),
        )

    # 6. Register LLM configurations
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
    if service_class is not None:
        def create_from_app_context(app_context):
            from jeeves_core.wiring import create_tool_executor

            tools_cfg = registry.get_tools(capability_id)
            tool_catalog = tools_cfg.get_catalog() if tools_cfg else None
            tool_executor = create_tool_executor(tool_catalog) if tool_catalog else None

            return service_class(
                llm_provider_factory=app_context.llm_provider_factory,
                tool_executor=tool_executor,
                logger=app_context.logger,
                pipeline_config=primary_pipeline,
                kernel_client=app_context.kernel_client,
                persistence=app_context.db,
                **(service_kwargs or {}),
            )

        return create_from_app_context

    return None


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
    "wire_capabilities",
    "wire_infra_routers",
]
