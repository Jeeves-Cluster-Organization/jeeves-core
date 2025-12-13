"""Configuration package for the mission system.

Generic configuration types are defined here.
Capability-specific agent profiles have been moved to capability layer.

Constitutional Reference:
    - Mission System: Provides generic config mechanisms
    - Capability Constitution R6: Domain Config Ownership
    - Agent profiles moved to: jeeves-capability-code-analyser/config/llm_config.py
"""

from jeeves_mission_system.config.constants import (
    FUZZY_MATCH_THRESHOLD,
    FUZZY_MATCH_CONFIDENCE_THRESHOLD,
    TASK_PRIORITY_LOW,
    TASK_PRIORITY_MEDIUM,
    TASK_PRIORITY_HIGH,
    TASK_DEFAULT_PRIORITY,
)

from jeeves_mission_system.config.agent_profiles import (
    LLMProfile,
    ThresholdProfile,
    AgentProfile,
    get_agent_profile,
    get_llm_profile,
    get_thresholds,
    get_latency_budget,
)

from jeeves_mission_system.config.registry import (
    ConfigRegistry,
    ConfigKeys,
    get_config_registry,
    set_config_registry,
    reset_config_registry,
)

__all__ = [
    # Constants
    "FUZZY_MATCH_THRESHOLD",
    "FUZZY_MATCH_CONFIDENCE_THRESHOLD",
    "TASK_PRIORITY_LOW",
    "TASK_PRIORITY_MEDIUM",
    "TASK_PRIORITY_HIGH",
    "TASK_DEFAULT_PRIORITY",
    # Agent Profile Types (generic)
    "LLMProfile",
    "ThresholdProfile",
    "AgentProfile",
    "get_agent_profile",
    "get_llm_profile",
    "get_thresholds",
    "get_latency_budget",
    # Config Registry
    "ConfigRegistry",
    "ConfigKeys",
    "get_config_registry",
    "set_config_registry",
    "reset_config_registry",
]
