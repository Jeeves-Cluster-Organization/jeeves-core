"""Control Tower Kernel - the unified kernel interface.

This is the main entry point for the Control Tower.
It composes:
- LifecycleManager (process scheduler)
- ResourceTracker (cgroups)
- CommBusCoordinator (IPC)
- EventAggregator (interrupt handler)
- InterruptService (unified interrupt handling)

Layering: ONLY imports from protocols (syscall interface).
"""

from datetime import datetime
from typing import Any, Dict, Optional

from jeeves_core.types import Envelope, TerminalReason
from protocols import LoggerProtocol
from shared.serialization import utc_now

from control_tower.events import EventAggregator
from control_tower.ipc import CommBusCoordinator
from control_tower.lifecycle import LifecycleManager
from control_tower.protocols import (
    CommBusCoordinatorProtocol,
    ControlTowerProtocol,
    EventAggregatorProtocol,
    LifecycleManagerProtocol,
    ResourceTrackerProtocol,
)
from control_tower.resources import ResourceTracker
from control_tower.services.interrupt_service import (
    InterruptService,
    InterruptKind,
    InterruptResponse,
)
from control_tower.types import (
    DispatchTarget,
    KernelEvent,
    ProcessState,
    ResourceQuota,
    SchedulingPriority,
    ServiceDescriptor,
)


class ControlTower(ControlTowerProtocol):
    """Control Tower - the Jeeves kernel.

    This is the central coordinator that manages:
    - Request lifecycle (process scheduling)
    - Resource allocation (quotas)
    - Service dispatch (IPC)
    - Event streaming (interrupts)

    OS Analogy:
        The Control Tower is like a microkernel. It doesn't execute
        the actual work (that's done by Mission System services),
        but it manages the lifecycle, resources, and communication.

    Usage:
        # Create kernel
        kernel = ControlTower(logger)

        # Register services (at startup)
        kernel.ipc.register_service(flow_service_descriptor)
        kernel.ipc.register_handler("flow_service", flow_handler)

        # Process a request
        envelope = create_envelope(request)
        result = await kernel.submit_request(envelope)

        # Resume after interrupt (clarification)
        result = await kernel.resume_request(pid, {"response": "..."})

    Layering:
        Gateway (HTTP/gRPC)
               |
               v
        +------------------+
        |  CONTROL TOWER   |  <- YOU ARE HERE
        +------------------+
               |
               v
        Mission System Services (FlowService, etc.)
               |
               v
        Avionics (LLM, Database, etc.)
    """

    def __init__(
        self,
        logger: LoggerProtocol,
        default_quota: Optional[ResourceQuota] = None,
        default_service: str = "flow_service",
        db: Optional[Any] = None,
        webhook_service: Optional[Any] = None,
        otel_adapter: Optional[Any] = None,
    ) -> None:
        """Initialize Control Tower.

        Args:
            logger: Logger instance
            default_quota: Default resource quota for requests
            default_service: Default service to dispatch to
            db: Optional database for interrupt persistence
            webhook_service: Optional webhook service for interrupt events
            otel_adapter: Optional OpenTelemetry adapter for tracing
        """
        self._logger = logger.bind(component="control_tower")
        self._default_quota = default_quota or ResourceQuota()
        self._default_service = default_service

        # Initialize subsystems
        self._lifecycle = LifecycleManager(logger, default_quota)
        self._resources = ResourceTracker(logger, default_quota)
        self._ipc = CommBusCoordinator(logger)
        self._events = EventAggregator(logger)

        # Unified interrupt service
        self._interrupts = InterruptService(
            db=db,
            logger=logger,
            webhook_service=webhook_service,
            otel_adapter=otel_adapter,
        )

        self._logger.info(
            "control_tower_initialized",
            default_service=default_service,
            max_llm_calls=self._default_quota.max_llm_calls,
            max_iterations=self._default_quota.max_iterations,
        )

    # =========================================================================
    # Protocol property implementations
    # =========================================================================

    @property
    def lifecycle(self) -> LifecycleManagerProtocol:
        """Get lifecycle manager."""
        return self._lifecycle

    @property
    def resources(self) -> ResourceTrackerProtocol:
        """Get resource tracker."""
        return self._resources

    @property
    def ipc(self) -> CommBusCoordinatorProtocol:
        """Get IPC coordinator."""
        return self._ipc

    @property
    def events(self) -> EventAggregatorProtocol:
        """Get event aggregator."""
        return self._events

    @property
    def interrupts(self) -> InterruptService:
        """Get interrupt service."""
        return self._interrupts

    # =========================================================================
    # Main request handling
    # =========================================================================

    async def submit_request(
        self,
        envelope: Envelope,
        priority: SchedulingPriority = SchedulingPriority.NORMAL,
        quota: Optional[ResourceQuota] = None,
    ) -> Envelope:
        """Submit a request for processing.

        This is the main entry point for requests.

        Flow:
        1. Create PCB (submit)
        2. Allocate resources
        3. Schedule for execution
        4. Dispatch to service
        5. Handle result/interrupt
        6. Return completed envelope
        """
        pid = envelope.envelope_id
        effective_quota = quota or self._default_quota

        self._logger.info(
            "request_submitted",
            pid=pid,
            request_id=envelope.request_id,
            user_id=envelope.user_id,
            priority=priority.value,
        )

        # 1. Create PCB
        pcb = self._lifecycle.submit(envelope, priority, effective_quota)

        # Emit process created event
        self._events.emit_event(
            KernelEvent.process_created(
                pid,
                envelope.request_id,
                request_context=envelope.request_context,
            )
        )

        # 2. Allocate resources
        self._resources.allocate(pid, effective_quota)

        # 3. Schedule for execution
        if not self._lifecycle.schedule(pid):
            self._logger.error("schedule_failed", pid=pid)
            envelope.terminated = True
            envelope.termination_reason = "Failed to schedule"
            return envelope

        # 4. Get next runnable and execute
        return await self._execute_process(pid, envelope)

    async def _execute_process(
        self,
        pid: str,
        envelope: Envelope,
    ) -> Envelope:
        """Execute a process.

        Handles the main execution loop:
        - Get runnable process
        - Dispatch to service
        - Handle result
        - Check for interrupts
        - Repeat or terminate
        """
        # Get the runnable process
        pcb = self._lifecycle.get_next_runnable()
        if not pcb or pcb.pid != pid:
            self._logger.error(
                "process_not_runnable",
                pid=pid,
                got_pid=pcb.pid if pcb else None,
            )
            envelope.terminated = True
            envelope.termination_reason = "Process not runnable"
            return envelope

        # Emit state change event
        self._events.emit_event(
            KernelEvent.process_state_changed(
                pid,
                ProcessState.READY,
                ProcessState.RUNNING,
                request_context=pcb.request_context,
            )
        )

        try:
            # Dispatch to service
            target = DispatchTarget(
                service_name=self._default_service,
                method="run",
                priority=pcb.priority,
                timeout_seconds=pcb.quota.timeout_seconds,
            )

            self._logger.debug(
                "dispatching_request",
                pid=pid,
                service=target.service_name,
            )

            result = await self._ipc.dispatch(target, envelope)

            # Check for resource exhaustion
            quota_exceeded = self._resources.check_quota(pid)
            if quota_exceeded:
                self._logger.warning(
                    "quota_exceeded",
                    pid=pid,
                    reason=quota_exceeded,
                )
                self._events.emit_event(
                    KernelEvent.resource_exhausted(
                        pid,
                        resource=quota_exceeded,
                        usage=0,
                        quota=0,
                        request_context=result.request_context,
                    )
                )

                # Create RESOURCE_EXHAUSTED interrupt via InterruptService
                await self._interrupts.create_resource_exhausted(
                    request_id=result.request_id,
                    user_id=result.user_id,
                    session_id=result.session_id,
                    resource_type=quota_exceeded,
                    retry_after_seconds=60.0,  # Default retry after 1 minute
                    envelope_id=result.envelope_id,
                )

                result.terminated = True
                result.termination_reason = quota_exceeded
                result.terminal_reason = TerminalReason(quota_exceeded)

            # Check for pending interrupt
            interrupt = self._events.get_pending_interrupt(pid)
            if interrupt:
                interrupt_kind, interrupt_data = interrupt
                return await self._handle_interrupt(
                    pid, result, interrupt_kind, interrupt_data
                )

            # Process completed
            if result.terminated:
                self._lifecycle.transition_state(
                    pid,
                    ProcessState.TERMINATED,
                    result.termination_reason,
                )

                self._events.emit_event(
                    KernelEvent.process_state_changed(
                        pid,
                        ProcessState.RUNNING,
                        ProcessState.TERMINATED,
                        request_context=result.request_context,
                    )
                )

            return result

        except Exception as e:
            self._logger.error(
                "execution_error",
                pid=pid,
                error=str(e),
            )

            envelope.terminated = True
            envelope.termination_reason = f"Execution error: {str(e)}"

            self._lifecycle.transition_state(
                pid,
                ProcessState.TERMINATED,
                str(e),
            )

            return envelope

    async def _handle_interrupt(
        self,
        pid: str,
        envelope: Envelope,
        interrupt_kind: InterruptKind,
        interrupt_data: Dict[str, Any],
    ) -> Envelope:
        """Handle a process interrupt.

        Transitions process to WAITING state and creates a unified
        FlowInterrupt via the InterruptService.
        """
        self._logger.info(
            "handling_interrupt",
            pid=pid,
            interrupt_kind=interrupt_kind.value,
        )

        # Transition to WAITING
        self._lifecycle.transition_state(pid, ProcessState.WAITING)

        self._events.emit_event(
            KernelEvent.process_state_changed(
                pid,
                ProcessState.RUNNING,
                ProcessState.WAITING,
                request_context=envelope.request_context,
            )
        )

        # Handle terminal interrupts
        if interrupt_kind == InterruptKind.TIMEOUT:
            envelope.terminated = True
            envelope.termination_reason = "Timeout"
            envelope.terminal_reason = TerminalReason.MAX_ITERATIONS_EXCEEDED
            return envelope

        if interrupt_kind == InterruptKind.RESOURCE_EXHAUSTED:
            envelope.terminated = True
            envelope.termination_reason = interrupt_data.get(
                "reason", "Resource exhausted"
            )
            return envelope

        # Create unified interrupt via service
        flow_interrupt = await self._interrupts.create_interrupt(
            kind=interrupt_kind,
            request_id=envelope.request_id,
            user_id=envelope.user_id,
            session_id=envelope.session_id,
            envelope_id=envelope.envelope_id,
            question=interrupt_data.get("question"),
            message=interrupt_data.get("message"),
            data=interrupt_data,
        )

        # Set envelope interrupt state
        envelope.interrupt_pending = True
        envelope.interrupt = flow_interrupt

        return envelope

    async def resume_request(
        self,
        pid: str,
        interrupt_id: str,
        response: InterruptResponse,
    ) -> Envelope:
        """Resume a waiting request.

        Called when user provides a response to an interrupt.

        Args:
            pid: Process ID (envelope_id)
            interrupt_id: ID of the interrupt being responded to
            response: Response data (text, approved, decision, etc.)

        Returns:
            Updated envelope after resuming execution
        """
        self._logger.info(
            "resuming_request",
            pid=pid,
            interrupt_id=interrupt_id,
        )

        # Get the PCB
        pcb = self._lifecycle.get_process(pid)
        if not pcb:
            self._logger.error("resume_unknown_pid", pid=pid)
            raise ValueError(f"Unknown process ID: {pid}")

        if pcb.state != ProcessState.WAITING:
            self._logger.warning(
                "resume_not_waiting",
                pid=pid,
                state=pcb.state.value,
            )

        # Resolve the interrupt via service
        resolved = await self._interrupts.respond(
            interrupt_id=interrupt_id,
            response=response,
            user_id=pcb.user_id,
        )

        if not resolved:
            self._logger.error(
                "interrupt_resolve_failed",
                interrupt_id=interrupt_id,
            )
            raise ValueError(f"Failed to resolve interrupt: {interrupt_id}")

        # Clear the kernel-level interrupt
        self._events.clear_interrupt(pid)

        # Transition back to READY (PCB should already be in WAITING state)
        self._lifecycle.transition_state(pid, ProcessState.READY)

        self._events.emit_event(
            KernelEvent.process_state_changed(
                pid,
                ProcessState.WAITING,
                ProcessState.READY,
                request_context=pcb.request_context,
            )
        )

        # Create envelope with resolved interrupt
        envelope = Envelope(
            request_context=pcb.request_context,
            envelope_id=pid,
            request_id=pcb.request_id,
            user_id=pcb.user_id,
            session_id=pcb.session_id,
            current_stage=pcb.current_stage,
        )

        # Clear interrupt pending and store resolved interrupt in envelope
        envelope.interrupt_pending = False
        envelope.interrupt = resolved

        # Re-execute
        return await self._execute_process(pid, envelope)

    def get_request_status(self, pid: str) -> Optional[Dict[str, Any]]:
        """Get status of a request."""
        pcb = self._lifecycle.get_process(pid)
        if not pcb:
            return None

        usage = self._resources.get_usage(pid)
        remaining = self._resources.get_remaining_budget(pid)
        interrupt = self._events.get_pending_interrupt(pid)

        return {
            "pid": pid,
            "state": pcb.state.value,
            "priority": pcb.priority.value,
            "current_stage": pcb.current_stage,
            "created_at": pcb.created_at.isoformat() if pcb.created_at else None,
            "started_at": pcb.started_at.isoformat() if pcb.started_at else None,
            "usage": {
                "llm_calls": usage.llm_calls if usage else 0,
                "tool_calls": usage.tool_calls if usage else 0,
                "elapsed_seconds": usage.elapsed_seconds if usage else 0,
            },
            "remaining": remaining,
            "has_interrupt": interrupt is not None,
            "interrupt_type": interrupt[0].value if interrupt else None,
        }

    async def cancel_request(self, pid: str, reason: str) -> bool:
        """Cancel a running request."""
        self._logger.info(
            "cancelling_request",
            pid=pid,
            reason=reason,
        )

        # Terminate the process
        if not self._lifecycle.terminate(pid, reason, force=True):
            return False

        # Release resources
        self._resources.release(pid)

        pcb = self._lifecycle.get_process(pid)
        if not pcb:
            self._logger.error("cancel_request_missing_pcb", pid=pid)
            return False

        # Emit event
        self._events.emit_event(
            KernelEvent(
                event_type="process.cancelled",
                timestamp=utc_now(),
                request_context=pcb.request_context,
                pid=pid,
                data={"reason": reason},
            )
        )

        return True

    # =========================================================================
    # Resource tracking helpers
    # =========================================================================

    def record_llm_call(
        self,
        pid: str,
        tokens_in: int = 0,
        tokens_out: int = 0,
    ) -> Optional[str]:
        """Record an LLM call for a process.

        Returns quota exceeded reason if any, None otherwise.
        """
        self._resources.record_usage(
            pid,
            llm_calls=1,
            tokens_in=tokens_in,
            tokens_out=tokens_out,
        )

        return self._resources.check_quota(pid)

    def record_tool_call(self, pid: str) -> Optional[str]:
        """Record a tool call for a process.

        Returns quota exceeded reason if any, None otherwise.
        """
        self._resources.record_usage(pid, tool_calls=1)
        return self._resources.check_quota(pid)

    def record_agent_hop(self, pid: str) -> Optional[str]:
        """Record an agent hop for a process.

        Returns quota exceeded reason if any, None otherwise.
        """
        self._resources.record_usage(pid, agent_hops=1)
        return self._resources.check_quota(pid)

    # =========================================================================
    # Service registration helpers
    # =========================================================================

    def register_service(
        self,
        name: str,
        service_type: str,
        handler: Any,
        capabilities: Optional[list] = None,
        max_concurrent: int = 10,
    ) -> bool:
        """Register a service with the kernel.

        Convenience method that registers both descriptor and handler.
        """
        descriptor = ServiceDescriptor(
            name=name,
            service_type=service_type,
            capabilities=capabilities or [],
            max_concurrent=max_concurrent,
        )

        if not self._ipc.register_service(descriptor):
            return False

        self._ipc.register_handler(name, handler)
        return True

    # =========================================================================
    # System metrics
    # =========================================================================

    def get_system_status(self) -> Dict[str, Any]:
        """Get overall system status."""
        process_counts = self._lifecycle.get_process_count()
        resource_usage = self._resources.get_system_usage()
        event_counts = self._events.get_event_counts()
        services = self._ipc.list_services()

        return {
            "processes": {
                "total": sum(process_counts.values()),
                "by_state": {
                    state.value: count
                    for state, count in process_counts.items()
                },
                "queue_depth": self._lifecycle.get_queue_depth(),
            },
            "resources": resource_usage,
            "events": {
                "total": sum(event_counts.values()),
                "by_type": event_counts,
                "history_size": self._events.get_history_size(),
            },
            "services": {
                "total": len(services),
                "healthy": len([s for s in services if s.healthy]),
                "names": [s.name for s in services],
            },
        }
