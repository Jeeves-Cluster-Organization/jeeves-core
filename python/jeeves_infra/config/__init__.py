"""Configuration package for jeeves_infra.

Config ownership map:
  settings.py       — environment-specific settings from env vars (host, port, adapter)
  feature_flags.py  — runtime feature toggles from env vars (tracing, distributed mode)
  constants.py      — static operational constants (timeouts, limits, fuzzy weights)
  thresholds.py     — governance thresholds (risk levels, token budgets, rate limits)
  agent_profiles.py — generic agent LLM/threshold/latency profiles
  registry.py       — ConfigRegistry for runtime config lookup

Capability-specific agent profiles have been moved to capability layer.
"""

from jeeves_infra.config.constants import (
    # Platform Identity
    PLATFORM_NAME,
    PLATFORM_VERSION,
    PLATFORM_DESCRIPTION,
    # Fuzzy Matching
    FUZZY_MATCH_CONFIDENCE_THRESHOLD,
    FUZZY_MATCH_SUBSTRING_WEIGHT,
    FUZZY_MATCH_WORD_OVERLAP_WEIGHT,
    FUZZY_MATCH_CHAR_SIMILARITY_WEIGHT,
    FUZZY_MATCH_SECONDARY_WEIGHT,
    # Database Query Limits
    DB_DEFAULT_LIMIT,
    DB_RECENT_TASKS_LIMIT,
    DB_CONVERSATION_HISTORY_LIMIT,
    # Timeout Values
    LLM_REQUEST_TIMEOUT,
    DB_QUERY_TIMEOUT,
    TOOL_EXECUTION_TIMEOUT,
    # Error Handling
    MAX_RETRY_ATTEMPTS,
    RETRY_BACKOFF_MULTIPLIER,
    RETRY_INITIAL_DELAY,
    # Response Limits
    VALIDATOR_RESPONSE_WORD_LIMIT,
)

from jeeves_infra.config.agent_profiles import (
    AgentLLMConfig,
    ThresholdProfile,
    AgentProfile,
    get_agent_profile,
    get_llm_profile,
    get_thresholds,
    get_latency_budget,
)

from jeeves_infra.config.registry import (
    ConfigRegistry,
    ConfigKeys,
)

__all__ = [
    # Platform Identity
    "PLATFORM_NAME",
    "PLATFORM_VERSION",
    "PLATFORM_DESCRIPTION",
    # Fuzzy Matching
    "FUZZY_MATCH_CONFIDENCE_THRESHOLD",
    "FUZZY_MATCH_SUBSTRING_WEIGHT",
    "FUZZY_MATCH_WORD_OVERLAP_WEIGHT",
    "FUZZY_MATCH_CHAR_SIMILARITY_WEIGHT",
    "FUZZY_MATCH_SECONDARY_WEIGHT",
    # Database Query Limits
    "DB_DEFAULT_LIMIT",
    "DB_RECENT_TASKS_LIMIT",
    "DB_CONVERSATION_HISTORY_LIMIT",
    # Timeout Values
    "LLM_REQUEST_TIMEOUT",
    "DB_QUERY_TIMEOUT",
    "TOOL_EXECUTION_TIMEOUT",
    # Error Handling
    "MAX_RETRY_ATTEMPTS",
    "RETRY_BACKOFF_MULTIPLIER",
    "RETRY_INITIAL_DELAY",
    # Response Limits
    "VALIDATOR_RESPONSE_WORD_LIMIT",
    # Agent Profile Types (generic)
    "AgentLLMConfig",
    "ThresholdProfile",
    "AgentProfile",
    "get_agent_profile",
    "get_llm_profile",
    "get_thresholds",
    "get_latency_budget",
    # Config Registry
    "ConfigRegistry",
    "ConfigKeys",
]
