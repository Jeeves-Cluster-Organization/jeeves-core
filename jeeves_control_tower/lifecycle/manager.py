"""Lifecycle Manager - process scheduler equivalent.

This implements the kernel's process scheduling:
- Process creation (submit)
- State transitions (schedule, wait, block, resume, terminate)
- Priority scheduling (get_next_runnable)

Layering: ONLY imports from jeeves_protocols (syscall interface).
"""

import heapq
import threading
from datetime import datetime
from typing import Dict, List, Optional

from jeeves_protocols import GenericEnvelope, LoggerProtocol
from jeeves_shared.serialization import utc_now

from jeeves_control_tower.protocols import LifecycleManagerProtocol
from jeeves_control_tower.types import (
    ProcessControlBlock,
    ProcessState,
    ResourceQuota,
    SchedulingPriority,
)


# Priority values for heap (lower = higher priority)
_PRIORITY_VALUES = {
    SchedulingPriority.REALTIME: 0,
    SchedulingPriority.HIGH: 1,
    SchedulingPriority.NORMAL: 2,
    SchedulingPriority.LOW: 3,
    SchedulingPriority.IDLE: 4,
}


# Valid state transitions
_VALID_TRANSITIONS: Dict[ProcessState, set[ProcessState]] = {
    ProcessState.NEW: {ProcessState.READY, ProcessState.TERMINATED},
    ProcessState.READY: {ProcessState.RUNNING, ProcessState.TERMINATED},
    ProcessState.RUNNING: {
        ProcessState.READY,      # Preempted
        ProcessState.WAITING,    # Waiting for I/O (clarification)
        ProcessState.BLOCKED,    # Resource exhausted
        ProcessState.TERMINATED,
    },
    ProcessState.WAITING: {ProcessState.READY, ProcessState.TERMINATED},
    ProcessState.BLOCKED: {ProcessState.READY, ProcessState.TERMINATED},
    ProcessState.TERMINATED: {ProcessState.ZOMBIE},
    ProcessState.ZOMBIE: set(),  # Terminal state
}


class LifecycleManager(LifecycleManagerProtocol):
    """Process lifecycle manager - kernel scheduler.

    Thread-safe implementation using locks.
    Uses a priority heap for ready queue.

    Usage:
        lifecycle = LifecycleManager(logger)

        # Submit and schedule a request
        pcb = lifecycle.submit(envelope)
        lifecycle.schedule(pcb.pid)

        # Get next process to run
        next_pcb = lifecycle.get_next_runnable()
        if next_pcb:
            # Execute...
            lifecycle.transition_state(next_pcb.pid, ProcessState.TERMINATED)
    """

    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
    ) -> None:
        """Initialize lifecycle manager.

        Args:
            logger: Logger instance
            default_quota: Default resource quota for new processes
        """
        self._logger = logger.bind(component="lifecycle_manager")
        self._default_quota = default_quota or ResourceQuota()

        # Process table (like OS process table)
        self._processes: Dict[str, ProcessControlBlock] = {}

        # Ready queue (priority heap)
        # Entries: (priority_value, submission_time, pid)
        self._ready_queue: List[tuple[int, datetime, str]] = []

        # Lock for thread safety
        self._lock = threading.RLock()

    def submit(
        self,
        envelope: GenericEnvelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> ProcessControlBlock:
        """Submit a new request for processing.

        Creates PCB in NEW state.
        """
        with self._lock:
            pid = envelope.envelope_id

            # Check for duplicate
            if pid in self._processes:
                self._logger.warning(
                    "duplicate_pid",
                    pid=pid,
                    existing_state=self._processes[pid].state.value,
                )
                return self._processes[pid]

            # Create PCB
            pcb = ProcessControlBlock(
                pid=pid,
                request_id=envelope.request_id,
                user_id=envelope.user_id,
                session_id=envelope.session_id,
                state=ProcessState.NEW,
                priority=priority,
                quota=quota or self._default_quota,
                created_at=utc_now(),
                current_stage=envelope.current_stage,
            )

            self._processes[pid] = pcb

            self._logger.info(
                "process_submitted",
                pid=pid,
                priority=priority.value,
                user_id=envelope.user_id,
            )

            return pcb

    def schedule(self, pid: str) -> bool:
        """Schedule a process for execution.

        Transitions: NEW -> READY
        Adds to ready queue.
        """
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                self._logger.warning("schedule_unknown_pid", pid=pid)
                return False

            if pcb.state != ProcessState.NEW:
                self._logger.warning(
                    "schedule_invalid_state",
                    pid=pid,
                    current_state=pcb.state.value,
                )
                return False

            # Transition to READY
            pcb.state = ProcessState.READY

            # Add to ready queue
            priority_value = _PRIORITY_VALUES[pcb.priority]
            heapq.heappush(
                self._ready_queue,
                (priority_value, pcb.created_at, pid),
            )

            self._logger.debug(
                "process_scheduled",
                pid=pid,
                priority=pcb.priority.value,
            )

            return True

    def get_next_runnable(self) -> Optional[ProcessControlBlock]:
        """Get the next process to run.

        Returns highest priority READY process.
        Transitions: READY -> RUNNING
        """
        with self._lock:
            while self._ready_queue:
                _, _, pid = heapq.heappop(self._ready_queue)

                pcb = self._processes.get(pid)
                if not pcb:
                    continue  # Process was removed

                if pcb.state != ProcessState.READY:
                    continue  # State changed since queuing

                # Transition to RUNNING
                pcb.state = ProcessState.RUNNING
                pcb.started_at = pcb.started_at or utc_now()
                pcb.last_scheduled_at = utc_now()

                self._logger.debug(
                    "process_running",
                    pid=pid,
                    priority=pcb.priority.value,
                )

                return pcb

            return None

    def transition_state(
        self,
        pid: str,
        new_state: ProcessState,
        reason: Optional[str] = None,
    ) -> bool:
        """Transition a process to a new state."""
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                self._logger.warning("transition_unknown_pid", pid=pid)
                return False

            old_state = pcb.state

            # Validate transition
            if new_state not in _VALID_TRANSITIONS.get(old_state, set()):
                self._logger.warning(
                    "invalid_state_transition",
                    pid=pid,
                    old_state=old_state.value,
                    new_state=new_state.value,
                )
                return False

            # Perform transition
            pcb.state = new_state

            # Handle specific transitions
            if new_state == ProcessState.READY:
                # Re-add to ready queue
                priority_value = _PRIORITY_VALUES[pcb.priority]
                heapq.heappush(
                    self._ready_queue,
                    (priority_value, utc_now(), pid),
                )

            elif new_state == ProcessState.TERMINATED:
                pcb.completed_at = utc_now()

            self._logger.info(
                "state_transition",
                pid=pid,
                old_state=old_state.value,
                new_state=new_state.value,
                reason=reason,
            )

            return True

    def get_process(self, pid: str) -> Optional[ProcessControlBlock]:
        """Get a process by ID."""
        with self._lock:
            return self._processes.get(pid)

    def list_processes(
        self,
        state: Optional[ProcessState] = None,
        user_id: Optional[str] = None,
    ) -> List[ProcessControlBlock]:
        """List processes matching criteria."""
        with self._lock:
            result = []
            for pcb in self._processes.values():
                if state and pcb.state != state:
                    continue
                if user_id and pcb.user_id != user_id:
                    continue
                result.append(pcb)
            return result

    def terminate(
        self,
        pid: str,
        reason: str,
        force: bool = False,
    ) -> bool:
        """Terminate a process."""
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                return False

            if pcb.is_terminated():
                return True  # Already terminated

            # Check if forcible
            if pcb.state == ProcessState.RUNNING and not force:
                self._logger.warning(
                    "cannot_terminate_running",
                    pid=pid,
                    state=pcb.state.value,
                )
                return False

            # Terminate
            pcb.state = ProcessState.TERMINATED
            pcb.completed_at = utc_now()

            self._logger.info(
                "process_terminated",
                pid=pid,
                reason=reason,
                force=force,
            )

            return True

    def cleanup(self, pid: str) -> bool:
        """Clean up a terminated process."""
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                return False

            if pcb.state not in (ProcessState.TERMINATED, ProcessState.ZOMBIE):
                self._logger.warning(
                    "cannot_cleanup_active",
                    pid=pid,
                    state=pcb.state.value,
                )
                return False

            del self._processes[pid]

            self._logger.debug("process_cleaned_up", pid=pid)

            return True

    # =========================================================================
    # Additional utility methods
    # =========================================================================

    def get_queue_depth(self) -> int:
        """Get number of processes in ready queue."""
        with self._lock:
            return len(self._ready_queue)

    def get_process_count(self) -> Dict[ProcessState, int]:
        """Get count of processes by state."""
        with self._lock:
            counts: Dict[ProcessState, int] = {}
            for pcb in self._processes.values():
                counts[pcb.state] = counts.get(pcb.state, 0) + 1
            return counts

    def update_current_stage(self, pid: str, stage: str) -> bool:
        """Update the current stage of a process."""
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                return False
            pcb.current_stage = stage
            return True

    def set_interrupt(
        self,
        pid: str,
        interrupt_kind: "InterruptKind",
        data: Dict,
    ) -> bool:
        """Set a pending interrupt on a process."""
        from jeeves_protocols import InterruptKind

        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                return False

            pcb.pending_interrupt = interrupt_kind
            pcb.interrupt_data = data

            # Transition to WAITING if currently running
            if pcb.state == ProcessState.RUNNING:
                pcb.state = ProcessState.WAITING

            return True

    def clear_interrupt(self, pid: str) -> bool:
        """Clear pending interrupt on a process."""
        with self._lock:
            pcb = self._processes.get(pid)
            if not pcb:
                return False

            pcb.pending_interrupt = None
            pcb.interrupt_data = {}

            return True
