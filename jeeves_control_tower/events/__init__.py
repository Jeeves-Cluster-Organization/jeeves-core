"""Event management - interrupt handler equivalent.

This module implements the kernel's event/interrupt handling:
- Interrupt queue management
- Event emission and subscription
- Event history tracking
"""

from jeeves_control_tower.events.aggregator import EventAggregator

__all__ = ["EventAggregator"]
