"""Lifecycle management - process scheduler equivalent.

This module implements the kernel's process lifecycle management:
- Process creation and termination
- State machine transitions
- Scheduling (priority queue)
"""

from jeeves_control_tower.lifecycle.manager import LifecycleManager

__all__ = ["LifecycleManager"]
