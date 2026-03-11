"""Core mock implementations for jeeves_core protocols.

Centralized Architecture (v4.0):
- Stage order defined in pipeline config, not enum
- Agent context via AgentContext (frozen dataclass)

This module is reserved for core protocol mocks.
Infrastructure mocks (LLM, DB, etc.) live in infra_mocks.py.
"""
