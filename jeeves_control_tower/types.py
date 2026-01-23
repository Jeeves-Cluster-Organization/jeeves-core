"""Control Tower types - OS-level abstractions.

These types mirror OS kernel concepts:
- ProcessState: Process lifecycle states
- ResourceQuota: cgroups-style resource limits
- DispatchTarget: Routing information for service dispatch

Layering: This module ONLY imports from jeeves_protocols (syscall interface).
"""

from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

from jeeves_protocols import RateLimitConfig, InterruptKind
from jeeves_shared.serialization import utc_now


# =============================================================================
# PROCESS STATES (mirrors OS process states)
# =============================================================================

class ProcessState(str, Enum):
    """Process states - mirrors OS process lifecycle.

    State transitions:
        NEW -> READY -> RUNNING -> (WAITING | BLOCKED | TERMINATED)
        WAITING -> READY (on event)
        BLOCKED -> READY (on resource available)
    """
    NEW = "new"                    # Just created, not yet scheduled
    READY = "ready"                # Ready to run, waiting for CPU
    RUNNING = "running"            # Currently executing
    WAITING = "waiting"            # Waiting for I/O or event (clarification, confirmation)
    BLOCKED = "blocked"            # Blocked on resource (quota exceeded)
    TERMINATED = "terminated"      # Finished execution
    ZOMBIE = "zombie"              # Terminated but not yet cleaned up


class SchedulingPriority(str, Enum):
    """Scheduling priority levels."""
    REALTIME = "realtime"          # Highest priority (system critical)
    HIGH = "high"                  # User-interactive
    NORMAL = "normal"              # Default
    LOW = "low"                    # Background tasks
    IDLE = "idle"                  # Only when nothing else to do


# =============================================================================
# RESOURCE QUOTAS (mirrors cgroups)
# =============================================================================

@dataclass
class ResourceQuota:
    """Resource quota - cgroups-style limits.

    Enforces resource constraints at the kernel level:
    - Token limits (memory equivalent)
    - Call limits (CPU time equivalent)
    - Time limits (wall clock)
    - Rate limits (requests over time)
    """
    # Token limits (like memory limits)
    max_input_tokens: int = 4096
    max_output_tokens: int = 2048
    max_context_tokens: int = 16384

    # Call limits (like CPU time)
    max_llm_calls: int = 10
    max_tool_calls: int = 50
    max_agent_hops: int = 21
    max_iterations: int = 3

    # Time limits
    timeout_seconds: int = 300
    soft_timeout_seconds: int = 240  # Warn before hard timeout

    # Rate limits (per-user)
    rate_limit: Optional[RateLimitConfig] = None

    def is_within_bounds(
        self,
        llm_calls: int,
        tool_calls: int,
        agent_hops: int,
        iterations: int,
    ) -> bool:
        """Check if usage is within quota."""
        return (
            llm_calls <= self.max_llm_calls
            and tool_calls <= self.max_tool_calls
            and agent_hops <= self.max_agent_hops
            and iterations <= self.max_iterations
        )


@dataclass
class ResourceUsage:
    """Current resource usage for a process."""
    llm_calls: int = 0
    tool_calls: int = 0
    agent_hops: int = 0
    iterations: int = 0
    tokens_in: int = 0
    tokens_out: int = 0
    elapsed_seconds: float = 0.0

    def exceeds_quota(self, quota: ResourceQuota) -> Optional[str]:
        """Check if usage exceeds quota. Returns reason or None."""
        if self.llm_calls > quota.max_llm_calls:
            return "max_llm_calls_exceeded"
        if self.tool_calls > quota.max_tool_calls:
            return "max_tool_calls_exceeded"
        if self.agent_hops > quota.max_agent_hops:
            return "max_agent_hops_exceeded"
        if self.iterations > quota.max_iterations:
            return "max_iterations_exceeded"
        if self.elapsed_seconds > quota.timeout_seconds:
            return "timeout_exceeded"
        return None


# =============================================================================
# PROCESS CONTROL BLOCK (PCB)
# =============================================================================

@dataclass
class ProcessControlBlock:
    """Process Control Block - kernel's view of a request.

    This is the kernel's metadata about a running "process" (request).
    The actual request state is in Envelope; this tracks:
    - Scheduling state
    - Resource accounting
    - Interrupt status
    """
    # Identity
    pid: str                       # Process ID (envelope_id)
    request_id: str
    user_id: str
    session_id: str

    # State
    state: ProcessState = ProcessState.NEW
    priority: SchedulingPriority = SchedulingPriority.NORMAL

    # Resource tracking
    quota: ResourceQuota = field(default_factory=ResourceQuota)
    usage: ResourceUsage = field(default_factory=ResourceUsage)

    # Scheduling
    created_at: datetime = field(default_factory=datetime.utcnow)
    started_at: Optional[datetime] = None
    completed_at: Optional[datetime] = None
    last_scheduled_at: Optional[datetime] = None

    # Current execution
    current_stage: str = ""
    current_service: str = ""      # Which service is executing

    # Interrupt handling
    pending_interrupt: Optional[InterruptKind] = None
    interrupt_data: Dict[str, Any] = field(default_factory=dict)

    # Parent/child relationships (for sub-requests)
    parent_pid: Optional[str] = None
    child_pids: List[str] = field(default_factory=list)

    def can_schedule(self) -> bool:
        """Check if process can be scheduled."""
        return self.state in (ProcessState.NEW, ProcessState.READY)

    def is_runnable(self) -> bool:
        """Check if process is runnable."""
        return self.state == ProcessState.READY

    def is_terminated(self) -> bool:
        """Check if process has terminated."""
        return self.state in (ProcessState.TERMINATED, ProcessState.ZOMBIE)


# =============================================================================
# DISPATCH TARGETS
# =============================================================================

@dataclass
class ServiceDescriptor:
    """Describes a registered service (Mission System service).

    Services are like OS daemons that Control Tower dispatches to:
    - FlowService: Pipeline execution
    - WorkerCoordinator: Distributed execution
    - VerticalRegistry: Capability discovery
    """
    name: str
    service_type: str              # "flow", "worker", "vertical"
    version: str = "1.0.0"
    capabilities: List[str] = field(default_factory=list)
    max_concurrent: int = 10
    current_load: int = 0
    healthy: bool = True
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass
class DispatchTarget:
    """Target for request dispatch."""
    service_name: str
    method: str
    priority: SchedulingPriority = SchedulingPriority.NORMAL
    timeout_seconds: int = 300
    retry_count: int = 0
    max_retries: int = 3


# =============================================================================
# CONTROL TOWER EVENTS (kernel events)
# =============================================================================

@dataclass
class KernelEvent:
    """Event emitted by the kernel.

    These are OS-level events, not application events.
    Used for monitoring and inter-service coordination.
    """
    event_type: str
    timestamp: datetime
    pid: Optional[str] = None
    data: Dict[str, Any] = field(default_factory=dict)

    @classmethod
    def process_created(cls, pid: str, request_id: str) -> "KernelEvent":
        return cls(
            event_type="process.created",
            timestamp=utc_now(),
            pid=pid,
            data={"request_id": request_id},
        )

    @classmethod
    def process_state_changed(
        cls,
        pid: str,
        old_state: ProcessState,
        new_state: ProcessState,
    ) -> "KernelEvent":
        return cls(
            event_type="process.state_changed",
            timestamp=utc_now(),
            pid=pid,
            data={"old_state": old_state.value, "new_state": new_state.value},
        )

    @classmethod
    def interrupt_raised(
        cls,
        pid: str,
        interrupt_type: InterruptKind,
        data: Dict[str, Any],
    ) -> "KernelEvent":
        return cls(
            event_type="interrupt.raised",
            timestamp=utc_now(),
            pid=pid,
            data={"interrupt_type": interrupt_type.value, **data},
        )

    @classmethod
    def resource_exhausted(
        cls,
        pid: str,
        resource: str,
        usage: int,
        quota: int,
    ) -> "KernelEvent":
        return cls(
            event_type="resource.exhausted",
            timestamp=utc_now(),
            pid=pid,
            data={"resource": resource, "usage": usage, "quota": quota},
        )
