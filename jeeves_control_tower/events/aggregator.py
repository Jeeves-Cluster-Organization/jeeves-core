"""Event Aggregator - interrupt handler equivalent.

This implements the kernel's event/interrupt handling:
- Process interrupts (like software interrupts)
- Kernel events (like hardware interrupts)
- Event history (like dmesg)

Layering: ONLY imports from jeeves_protocols (syscall interface).
"""

import threading
from collections import deque
from datetime import datetime
from typing import Any, Callable, Deque, Dict, List, Optional, Tuple

from jeeves_protocols import LoggerProtocol, InterruptKind, RequestContext
from jeeves_shared.serialization import utc_now

from jeeves_control_tower.protocols import EventAggregatorProtocol
from jeeves_control_tower.types import KernelEvent


# Type alias for event handlers
EventHandler = Callable[[KernelEvent], None]


class EventAggregator(EventAggregatorProtocol):
    """Event aggregator - kernel interrupt handler.

    Manages kernel events and process interrupts:
    - Interrupt queue per process (pending interrupts)
    - Event bus for kernel events
    - Event history (ring buffer)

    Usage:
        aggregator = EventAggregator(logger)

        # Raise an interrupt on a process
        aggregator.raise_interrupt(
            pid="env-123",
            interrupt_type=InterruptKind.CLARIFICATION,
            data={"question": "What do you mean?"},
            request_context=request_context,
        )

        # Check for pending interrupt
        interrupt = aggregator.get_pending_interrupt(pid)
        if interrupt:
            interrupt_type, data = interrupt
            # Handle interrupt...
            aggregator.clear_interrupt(pid)

        # Subscribe to kernel events
        aggregator.subscribe("process.state_changed", my_handler)

        # Emit a kernel event
        aggregator.emit_event(
            KernelEvent.process_created(
                pid,
                request_id,
                request_context=request_context,
            )
        )
    """

    def __init__(
        self,
        logger: LoggerProtocol,
        history_size: int = 10000,
    ) -> None:
        """Initialize event aggregator.

        Args:
            logger: Logger instance
            history_size: Max events to keep in history
        """
        self._logger = logger.bind(component="event_aggregator")
        self._history_size = history_size

        # Pending interrupts per process
        # Only ONE interrupt can be pending at a time (like CPU)
        self._pending_interrupts: Dict[str, Tuple[InterruptKind, Dict[str, Any]]] = {}

        # Event subscribers (event_type -> list of handlers)
        self._subscribers: Dict[str, List[EventHandler]] = {}

        # Wildcard subscribers (receive all events)
        self._wildcard_subscribers: List[EventHandler] = []

        # Event history (ring buffer)
        self._history: Deque[KernelEvent] = deque(maxlen=history_size)

        # Per-process event history (for debugging)
        self._process_history: Dict[str, Deque[KernelEvent]] = {}
        self._process_history_size = 100

        # Lock for thread safety
        self._lock = threading.RLock()

        # Event counters
        self._event_counts: Dict[str, int] = {}

    def raise_interrupt(
        self,
        pid: str,
        interrupt_type: InterruptKind,
        data: Dict[str, Any],
        request_context: RequestContext,
    ) -> None:
        """Raise an interrupt for a process."""
        with self._lock:
            # Check for existing interrupt
            if pid in self._pending_interrupts:
                existing_type, _ = self._pending_interrupts[pid]
                self._logger.warning(
                    "interrupt_override",
                    pid=pid,
                    existing_type=existing_type.value,
                    new_type=interrupt_type.value,
                )

            self._pending_interrupts[pid] = (interrupt_type, data)

            self._logger.info(
                "interrupt_raised",
                pid=pid,
                interrupt_type=interrupt_type.value,
                data_keys=list(data.keys()),
            )

            # Emit kernel event
            self.emit_event(
                KernelEvent.interrupt_raised(
                    pid,
                    interrupt_type,
                    data,
                    request_context=request_context,
                )
            )

    def get_pending_interrupt(
        self,
        pid: str,
    ) -> Optional[Tuple[InterruptKind, Dict[str, Any]]]:
        """Get pending interrupt for a process."""
        with self._lock:
            return self._pending_interrupts.get(pid)

    def clear_interrupt(self, pid: str) -> bool:
        """Clear pending interrupt for a process."""
        with self._lock:
            if pid not in self._pending_interrupts:
                return False

            interrupt_type, _ = self._pending_interrupts[pid]
            del self._pending_interrupts[pid]

            self._logger.debug(
                "interrupt_cleared",
                pid=pid,
                interrupt_type=interrupt_type.value,
            )

            return True

    def emit_event(self, event: KernelEvent) -> None:
        """Emit a kernel event."""
        with self._lock:
            # Add to history
            self._history.append(event)

            # Add to process history if applicable
            if event.pid:
                if event.pid not in self._process_history:
                    self._process_history[event.pid] = deque(
                        maxlen=self._process_history_size
                    )
                self._process_history[event.pid].append(event)

            # Update counters
            self._event_counts[event.event_type] = (
                self._event_counts.get(event.event_type, 0) + 1
            )

            # Get handlers (copy to avoid lock contention)
            handlers = list(self._subscribers.get(event.event_type, []))
            wildcard_handlers = list(self._wildcard_subscribers)

        # Call handlers outside lock
        for handler in handlers:
            try:
                handler(event)
            except Exception as e:
                self._logger.error(
                    "event_handler_error",
                    event_type=event.event_type,
                    error=str(e),
                )

        for handler in wildcard_handlers:
            try:
                handler(event)
            except Exception as e:
                self._logger.error(
                    "wildcard_handler_error",
                    event_type=event.event_type,
                    error=str(e),
                )

        self._logger.debug(
            "event_emitted",
            event_type=event.event_type,
            pid=event.pid,
            handler_count=len(handlers) + len(wildcard_handlers),
        )

    def subscribe(
        self,
        event_type: str,
        handler: EventHandler,
    ) -> None:
        """Subscribe to kernel events.

        Args:
            event_type: Event type to subscribe to, or "*" for all events
            handler: Handler function
        """
        with self._lock:
            if event_type == "*":
                self._wildcard_subscribers.append(handler)
            else:
                if event_type not in self._subscribers:
                    self._subscribers[event_type] = []
                self._subscribers[event_type].append(handler)

    def unsubscribe(
        self,
        event_type: str,
        handler: EventHandler,
    ) -> None:
        """Unsubscribe from kernel events."""
        with self._lock:
            if event_type == "*":
                try:
                    self._wildcard_subscribers.remove(handler)
                except ValueError:
                    pass
            else:
                if event_type in self._subscribers:
                    try:
                        self._subscribers[event_type].remove(handler)
                    except ValueError:
                        pass

    def get_event_history(
        self,
        pid: Optional[str] = None,
        event_type: Optional[str] = None,
        limit: int = 100,
    ) -> List[KernelEvent]:
        """Get event history.

        Args:
            pid: Filter by process (None for all)
            event_type: Filter by event type (None for all)
            limit: Max events to return

        Returns:
            List of events (newest first)
        """
        with self._lock:
            # Get source based on pid filter
            if pid:
                source = list(self._process_history.get(pid, []))
            else:
                source = list(self._history)

            # Filter by event type
            if event_type:
                source = [e for e in source if e.event_type == event_type]

            # Return newest first, limited
            return list(reversed(source))[:limit]

    # =========================================================================
    # Additional utility methods
    # =========================================================================

    def has_pending_interrupt(self, pid: str) -> bool:
        """Check if a process has a pending interrupt."""
        with self._lock:
            return pid in self._pending_interrupts

    def get_all_pending_interrupts(self) -> Dict[str, InterruptKind]:
        """Get all pending interrupts."""
        with self._lock:
            return {
                pid: interrupt_type
                for pid, (interrupt_type, _) in self._pending_interrupts.items()
            }

    def get_event_counts(self) -> Dict[str, int]:
        """Get event counts by type."""
        with self._lock:
            return dict(self._event_counts)

    def cleanup_process(self, pid: str) -> None:
        """Clean up all data for a process.

        Called when a process is cleaned up.
        """
        with self._lock:
            # Clear pending interrupt
            self._pending_interrupts.pop(pid, None)

            # Clear process history
            self._process_history.pop(pid, None)

    def get_recent_events(
        self,
        seconds: float = 60.0,
    ) -> List[KernelEvent]:
        """Get events from the last N seconds."""
        cutoff = utc_now().timestamp() - seconds

        with self._lock:
            return [
                e for e in self._history
                if e.timestamp.timestamp() > cutoff
            ]

    def get_subscriber_count(self, event_type: str) -> int:
        """Get number of subscribers for an event type."""
        with self._lock:
            if event_type == "*":
                return len(self._wildcard_subscribers)
            return len(self._subscribers.get(event_type, []))

    def get_history_size(self) -> int:
        """Get current history size."""
        with self._lock:
            return len(self._history)
