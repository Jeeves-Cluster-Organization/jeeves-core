"""Gateway routers package.

Code Analysis Focus (v3.0 Pivot):
- REMOVED: kanban, journal, open_loops routers
- FOCUS: chat, governance, interrupts

Unified Interrupt System (v4.0):
- interrupts: Single endpoint for all interrupt types (clarification, confirmation, etc.)
- Replaces separate /clarifications and /confirmations endpoints
"""

from jeeves_avionics.gateway.routers import chat, governance, interrupts

__all__ = ["chat", "governance", "interrupts"]
