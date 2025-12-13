"""
Core prompt building blocks for the 6-agent pipeline.

This module provides shared prompt components that ensure consistency
across all agents per Constitutional Amendment X.

Blocks:
- identity_block: Code Analysis Agent identity and persona
- style_block: Response voice and formatting rules
- role_invariants: Universal constraints for all agents
- safety_block: Safety guardrails

Usage:
    from jeeves_mission_system.prompts.core import IDENTITY_BLOCK, STYLE_BLOCK, ROLE_INVARIANTS, SAFETY_BLOCK

    prompt = f'''
    {IDENTITY_BLOCK}

    **Your Role:** Planner Agent

    {STYLE_BLOCK}

    {ROLE_INVARIANTS}

    {SAFETY_BLOCK}
    '''
"""

from .identity_block import IDENTITY_BLOCK
from .style_block import STYLE_BLOCK
from .role_invariants import ROLE_INVARIANTS
from .safety_block import SAFETY_BLOCK

__all__ = [
    "IDENTITY_BLOCK",
    "STYLE_BLOCK",
    "ROLE_INVARIANTS",
    "SAFETY_BLOCK",
]
