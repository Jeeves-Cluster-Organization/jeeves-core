"""
Mission System API - Stable Framework Interface for Capabilities

This module provides the stable, public API that capabilities build on.

**Constitutional Layering:**
- Capabilities import THIS module (top layer)
- This module imports from avionics + core (lower layers)
- This module NEVER imports capabilities

**Capabilities are applications that:**
- Build on top of this framework
- Use these primitives to assemble their services
- Own their own wiring and entry points

**Pattern:**
```python
# In capability (jeeves-capability-code-analyser)
from jeeves_mission_system.api import MissionRuntime, create_mission_runtime

# Capability assembles itself
runtime = create_mission_runtime(settings)
my_service = MyService(
    tool_executor=runtime.tool_executor,
    agent_runtime_factory=runtime.create_agent_runtime,
    ...
)
```
"""

from dataclasses import dataclass
from typing import Any, Callable, Optional

from jeeves_protocols import (
    LLMProviderProtocol,
    PersistenceProtocol,
    ToolRegistryProtocol,
    ToolExecutorProtocol,
    ConfigRegistryProtocol,
)
from jeeves_avionics.settings import Settings, get_settings
from jeeves_mission_system.config.registry import ConfigRegistry
from jeeves_avionics.database.client import DatabaseClientProtocol, create_database_client
from jeeves_avionics.llm.factory import create_agent_provider
from jeeves_avionics.llm.providers import MockProvider
from jeeves_avionics.wiring import ToolExecutor


@dataclass
class MissionRuntime:
    """
    Mission System Runtime - Framework primitives for capabilities.

    This bundles all the infrastructure primitives that capabilities
    need to build their services.

    Capabilities receive this from create_mission_runtime() and use it
    to assemble their domain services.

    Config Registry:
        Capabilities register their configs at bootstrap:
        ```python
        runtime.config_registry.register("language_config", my_lang_config)
        ```
        Tools receive config via dependency injection, not globals.
    """

    # Core services
    persistence: PersistenceProtocol
    tool_executor: ToolExecutorProtocol
    settings: Settings

    # Config registry for capability configuration injection
    config_registry: ConfigRegistryProtocol

    # Factories
    llm_provider_factory: Optional[Callable[[str], LLMProviderProtocol]]
    mock_provider: Optional[LLMProviderProtocol]

    # NOTE: create_agent_runtime removed in v4.0
    # Use Runtime + PipelineConfig instead:
    #   from jeeves_mission_system.contracts_core import create_runtime_from_config
    #   runtime = create_runtime_from_config(config, llm_factory, ...)

    @property
    def use_mock(self) -> bool:
        """Whether mock mode is enabled."""
        return self.mock_provider is not None


def create_mission_runtime(
    tool_registry: ToolRegistryProtocol,
    persistence: Optional[PersistenceProtocol] = None,
    settings: Optional[Settings] = None,
    use_mock: bool = False,
    config_registry: Optional[ConfigRegistryProtocol] = None,
) -> MissionRuntime:
    """
    Create a Mission System runtime for capabilities.

    This is the main entry point for capabilities to get framework primitives.

    Args:
        tool_registry: Tool registry (passed from capability bootstrap)
        persistence: Database client (created if None)
        settings: Application settings (loaded if None)
        use_mock: Whether to use mock LLM provider
        config_registry: Config registry for capability config injection
                        (created if None)

    Returns:
        MissionRuntime with all primitives configured

    Example:
        ```python
        # In capability bootstrap
        from jeeves_mission_system.api import create_mission_runtime
        from config.language_config import LanguageConfig

        # Create runtime with config registry
        runtime = create_mission_runtime(
            tool_registry=my_tool_registry,
            persistence=my_db,
            settings=my_settings,
        )

        # Register capability configs
        runtime.config_registry.register("language_config", LanguageConfig())

        # Use runtime to build capability service
        my_service = MyCapabilityService(
            tool_executor=runtime.tool_executor,
            agent_runtime_factory=runtime.create_agent_runtime,
            llm_factory=runtime.llm_provider_factory,
            config_registry=runtime.config_registry,
            ...
        )
        ```
    """
    if settings is None:
        settings = get_settings()

    if persistence is None:
        # Note: In async context, use await create_database_client(settings)
        # This is a sync placeholder - actual creation should be done by caller
        persistence = None  # Caller must provide persistence

    # Create config registry if not provided (DI pattern)
    if config_registry is None:
        config_registry = ConfigRegistry()

    # Create tool executor
    tool_executor = ToolExecutor(tool_registry)

    # Create LLM provider factory (None if mock)
    llm_factory = None
    if not use_mock:
        def factory(agent_role: str) -> LLMProviderProtocol:
            return create_agent_provider(settings, agent_role)
        llm_factory = factory

    # Create mock provider if needed
    mock_provider = MockProvider() if use_mock else None

    return MissionRuntime(
        persistence=persistence,
        tool_executor=tool_executor,
        settings=settings,
        config_registry=config_registry,
        llm_provider_factory=llm_factory,
        mock_provider=mock_provider,
    )


__all__ = [
    "MissionRuntime",
    "create_mission_runtime",
    "ConfigRegistry",
]
