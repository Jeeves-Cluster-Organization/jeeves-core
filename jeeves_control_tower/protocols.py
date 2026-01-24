"""Control Tower protocols - kernel interface definitions.

These protocols define the "syscall" interface that the Control Tower exposes
to higher layers (Mission System services, Gateway).

Layering rules:
- Control Tower ONLY imports from jeeves_protocols
- Higher layers import these protocols, not concrete implementations
- This is the kernel ABI - changes must be backward compatible
"""

from datetime import datetime
from typing import Any, Dict, List, Optional, Protocol, runtime_checkable

from jeeves_protocols import GenericEnvelope, RequestContext

from jeeves_protocols import InterruptKind
from jeeves_control_tower.types import (
    DispatchTarget,
    KernelEvent,
    ProcessControlBlock,
    ProcessState,
    ResourceQuota,
    ResourceUsage,
    SchedulingPriority,
    ServiceDescriptor,
)


# =============================================================================
# LIFECYCLE MANAGER PROTOCOL (Process Scheduler)
# =============================================================================

@runtime_checkable
class LifecycleManagerProtocol(Protocol):
    """Process lifecycle manager - kernel scheduler equivalent.

    Manages the lifecycle of requests ("processes"):
    - Create new process (fork)
    - Schedule process (schedule)
    - Transition states (signal)
    - Terminate process (kill)

    State machine:
        submit() -> NEW
        schedule() -> NEW -> READY
        run() -> READY -> RUNNING
        wait() -> RUNNING -> WAITING
        block() -> RUNNING -> BLOCKED
        resume() -> WAITING/BLOCKED -> READY
        terminate() -> * -> TERMINATED
        cleanup() -> TERMINATED -> (removed)
    """

    def submit(
        self,
        envelope: GenericEnvelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> ProcessControlBlock:
        """Submit a new request for processing.

        Creates a new PCB in NEW state.
        Does NOT start execution - call schedule() for that.

        Args:
            envelope: The request envelope
            priority: Scheduling priority
            quota: Resource quota (uses default if None)

        Returns:
            ProcessControlBlock for the new process
        """
        ...

    def schedule(self, pid: str) -> bool:
        """Schedule a process for execution.

        Transitions: NEW -> READY

        Args:
            pid: Process ID (envelope_id)

        Returns:
            True if scheduled, False if not schedulable
        """
        ...

    def get_next_runnable(self) -> Optional[ProcessControlBlock]:
        """Get the next process to run.

        Returns highest priority READY process.
        Transitions: READY -> RUNNING

        Returns:
            PCB of next process to run, or None if queue empty
        """
        ...

    def transition_state(
        self,
        pid: str,
        new_state: ProcessState,
        reason: Optional[str] = None,
    ) -> bool:
        """Transition a process to a new state.

        Args:
            pid: Process ID
            new_state: Target state
            reason: Optional reason for transition

        Returns:
            True if transition successful
        """
        ...

    def get_process(self, pid: str) -> Optional[ProcessControlBlock]:
        """Get a process by ID.

        Args:
            pid: Process ID

        Returns:
            PCB or None if not found
        """
        ...

    def list_processes(
        self,
        state: Optional[ProcessState] = None,
        user_id: Optional[str] = None,
    ) -> List[ProcessControlBlock]:
        """List processes matching criteria.

        Args:
            state: Filter by state (None for all)
            user_id: Filter by user (None for all)

        Returns:
            List of matching PCBs
        """
        ...

    def terminate(
        self,
        pid: str,
        reason: str,
        force: bool = False,
    ) -> bool:
        """Terminate a process.

        Transitions: * -> TERMINATED

        Args:
            pid: Process ID
            reason: Termination reason
            force: Force kill even if blocked

        Returns:
            True if terminated
        """
        ...

    def cleanup(self, pid: str) -> bool:
        """Clean up a terminated process.

        Removes the process from tracking.
        Only valid for TERMINATED or ZOMBIE state.

        Args:
            pid: Process ID

        Returns:
            True if cleaned up
        """
        ...


# =============================================================================
# RESOURCE TRACKER PROTOCOL (cgroups)
# =============================================================================

@runtime_checkable
class ResourceTrackerProtocol(Protocol):
    """Resource tracker - cgroups equivalent.

    Tracks and enforces resource usage:
    - Allocate quotas
    - Track usage
    - Enforce limits
    - Report usage
    """

    def allocate(
        self,
        pid: str,
        quota: ResourceQuota,
    ) -> bool:
        """Allocate resources to a process.

        Args:
            pid: Process ID
            quota: Resource quota to allocate

        Returns:
            True if allocated
        """
        ...

    def release(self, pid: str) -> bool:
        """Release resources from a process.

        Called when process terminates.

        Args:
            pid: Process ID

        Returns:
            True if released
        """
        ...

    def record_usage(
        self,
        pid: str,
        llm_calls: int = 0,
        tool_calls: int = 0,
        agent_hops: int = 0,
        tokens_in: int = 0,
        tokens_out: int = 0,
    ) -> ResourceUsage:
        """Record resource usage.

        Increments counters for the process.

        Args:
            pid: Process ID
            llm_calls: LLM calls to add
            tool_calls: Tool calls to add
            agent_hops: Agent hops to add
            tokens_in: Input tokens to add
            tokens_out: Output tokens to add

        Returns:
            Updated ResourceUsage
        """
        ...

    def check_quota(self, pid: str) -> Optional[str]:
        """Check if process is within quota.

        Args:
            pid: Process ID

        Returns:
            None if within quota, or reason string if exceeded
        """
        ...

    def get_usage(self, pid: str) -> Optional[ResourceUsage]:
        """Get current usage for a process.

        Args:
            pid: Process ID

        Returns:
            ResourceUsage or None if not tracked
        """
        ...

    def get_quota(self, pid: str) -> Optional[ResourceQuota]:
        """Get quota for a process.

        Args:
            pid: Process ID

        Returns:
            ResourceQuota or None if not tracked
        """
        ...

    def get_system_usage(self) -> Dict[str, Any]:
        """Get system-wide resource usage.

        Returns:
            Dict with system-wide metrics
        """
        ...


# =============================================================================
# COMMBUS COORDINATOR PROTOCOL (IPC Manager)
# =============================================================================

@runtime_checkable
class CommBusCoordinatorProtocol(Protocol):
    """CommBus coordinator - IPC manager equivalent.

    Manages communication between kernel and services:
    - Service registration
    - Message routing
    - Request dispatch

    This is the kernel's interface to the CommBus (IPC fabric).
    """

    def register_service(
        self,
        descriptor: ServiceDescriptor,
    ) -> bool:
        """Register a service with the kernel.

        Args:
            descriptor: Service descriptor

        Returns:
            True if registered
        """
        ...

    def unregister_service(self, service_name: str) -> bool:
        """Unregister a service.

        Args:
            service_name: Name of service to unregister

        Returns:
            True if unregistered
        """
        ...

    def get_service(self, service_name: str) -> Optional[ServiceDescriptor]:
        """Get a service descriptor.

        Args:
            service_name: Name of service

        Returns:
            ServiceDescriptor or None if not found
        """
        ...

    def list_services(
        self,
        service_type: Optional[str] = None,
        healthy_only: bool = True,
    ) -> List[ServiceDescriptor]:
        """List registered services.

        Args:
            service_type: Filter by type (None for all)
            healthy_only: Only return healthy services

        Returns:
            List of service descriptors
        """
        ...

    async def dispatch(
        self,
        target: DispatchTarget,
        envelope: GenericEnvelope,
    ) -> GenericEnvelope:
        """Dispatch a request to a service.

        This is how the kernel sends work to services.

        Args:
            target: Dispatch target (service + method)
            envelope: Request envelope

        Returns:
            Updated envelope from service
        """
        ...

    async def broadcast(
        self,
        event_type: str,
        payload: Dict[str, Any],
    ) -> None:
        """Broadcast an event to all subscribers.

        Args:
            event_type: Event type
            payload: Event payload
        """
        ...

    async def request(
        self,
        service_name: str,
        query_type: str,
        payload: Dict[str, Any],
        timeout_seconds: float = 30.0,
    ) -> Dict[str, Any]:
        """Send a request to a service and wait for response.

        Args:
            service_name: Target service
            query_type: Query type
            payload: Query payload
            timeout_seconds: Request timeout

        Returns:
            Response payload
        """
        ...


# =============================================================================
# EVENT AGGREGATOR PROTOCOL (Interrupt Handler)
# =============================================================================

@runtime_checkable
class EventAggregatorProtocol(Protocol):
    """Event aggregator - interrupt handler equivalent.

    Collects and routes kernel events:
    - Aggregate events from services
    - Route to appropriate handlers
    - Manage interrupt queue
    """

    def raise_interrupt(
        self,
        pid: str,
        interrupt_type: InterruptKind,
        data: Dict[str, Any],
        request_context: RequestContext,
    ) -> None:
        """Raise an interrupt for a process.

        Args:
            pid: Process ID
            interrupt_type: Type of interrupt
            data: Interrupt data
            request_context: RequestContext for correlation
        """
        ...

    def get_pending_interrupt(
        self,
        pid: str,
    ) -> Optional[tuple[InterruptKind, Dict[str, Any]]]:
        """Get pending interrupt for a process.

        Args:
            pid: Process ID

        Returns:
            Tuple of (interrupt_type, data) or None
        """
        ...

    def clear_interrupt(self, pid: str) -> bool:
        """Clear pending interrupt for a process.

        Args:
            pid: Process ID

        Returns:
            True if cleared
        """
        ...

    def emit_event(self, event: KernelEvent) -> None:
        """Emit a kernel event.

        Args:
            event: Event to emit
        """
        ...

    def subscribe(
        self,
        event_type: str,
        handler: Any,  # Callable[[KernelEvent], None]
    ) -> None:
        """Subscribe to kernel events.

        Args:
            event_type: Event type to subscribe to
            handler: Handler function
        """
        ...

    def unsubscribe(
        self,
        event_type: str,
        handler: Any,
    ) -> None:
        """Unsubscribe from kernel events.

        Args:
            event_type: Event type
            handler: Handler function
        """
        ...

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
            List of events
        """
        ...


# =============================================================================
# CONTROL TOWER PROTOCOL (Kernel Interface)
# =============================================================================

@runtime_checkable
class ControlTowerProtocol(Protocol):
    """Control Tower protocol - the unified kernel interface.

    This is the main interface that higher layers use to interact
    with the Control Tower. It composes the sub-protocols.
    """

    @property
    def lifecycle(self) -> LifecycleManagerProtocol:
        """Get lifecycle manager."""
        ...

    @property
    def resources(self) -> ResourceTrackerProtocol:
        """Get resource tracker."""
        ...

    @property
    def ipc(self) -> CommBusCoordinatorProtocol:
        """Get IPC coordinator."""
        ...

    @property
    def events(self) -> EventAggregatorProtocol:
        """Get event aggregator."""
        ...

    async def submit_request(
        self,
        envelope: GenericEnvelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> GenericEnvelope:
        """Submit a request for processing.

        This is the main entry point for requests.
        Creates process, allocates resources, dispatches to service.

        Args:
            envelope: Request envelope
            priority: Scheduling priority
            quota: Resource quota (uses default if None)

        Returns:
            Completed envelope
        """
        ...

    async def resume_request(
        self,
        pid: str,
        response_data: Dict[str, Any],
    ) -> GenericEnvelope:
        """Resume a waiting request.

        Used to resume after clarification/confirmation.

        Args:
            pid: Process ID
            response_data: Response data from user

        Returns:
            Completed envelope
        """
        ...

    def get_request_status(self, pid: str) -> Optional[Dict[str, Any]]:
        """Get status of a request.

        Args:
            pid: Process ID

        Returns:
            Status dict or None if not found
        """
        ...

    async def cancel_request(self, pid: str, reason: str) -> bool:
        """Cancel a running request.

        Args:
            pid: Process ID
            reason: Cancellation reason

        Returns:
            True if cancelled
        """
        ...
