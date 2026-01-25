"""CommBus Coordinator - IPC manager equivalent.

This implements the kernel's IPC management:
- Service registration (like init daemon registration)
- Message routing (like kernel message passing)
- Request dispatch (like syscall dispatch)
- Event publish/subscribe via InMemoryCommBus

Constitutional Reference:
- Control Tower CONSTITUTION: CommBus communication for service dispatch
- Memory Module CONSTITUTION P4: Memory operations publish events via CommBus

Layering: ONLY imports from protocols (syscall interface).
The actual CommBus communication happens through the InMemoryCommBus.
"""

import asyncio
import threading
from datetime import datetime
from typing import Any, Callable, Dict, List, Optional, TYPE_CHECKING, Union

from protocols import Envelope, LoggerProtocol, RequestContext

from control_tower.protocols import CommBusCoordinatorProtocol
from control_tower.types import DispatchTarget, ServiceDescriptor

if TYPE_CHECKING:
    from control_tower.ipc.commbus import InMemoryCommBus


# Type alias for dispatch handlers
DispatchHandler = Callable[[Envelope], "asyncio.Future[Envelope]"]


class CommBusCoordinator(CommBusCoordinatorProtocol):
    """CommBus coordinator - kernel IPC manager.

    Manages service registration and request dispatch.
    Acts as the kernel's view of the IPC fabric.

    Design:
    - Services register with the kernel via register_service()
    - Kernel dispatches requests to services via dispatch()
    - Services can be local (in-process) or remote (via adapter)

    Usage:
        coordinator = CommBusCoordinator(logger)

        # Register a service
        coordinator.register_service(ServiceDescriptor(
            name="flow_service",
            service_type="flow",
            capabilities=["execute_pipeline"],
        ))

        # Dispatch a request
        result = await coordinator.dispatch(
            target=DispatchTarget(service_name="flow_service", method="run"),
            envelope=envelope,
        )
    """

    def __init__(
        self,
        logger: LoggerProtocol,
        commbus: Optional["InMemoryCommBus"] = None,
    ) -> None:
        """Initialize CommBus coordinator.

        Args:
            logger: Logger instance
            commbus: Optional InMemoryCommBus for event pub/sub and queries.
                     If not provided, get_commbus() is used lazily.
        """
        self._logger = logger.bind(component="commbus_coordinator")
        self._commbus = commbus

        # Service registry
        self._services: Dict[str, ServiceDescriptor] = {}

        # Dispatch handlers (for local services)
        self._handlers: Dict[str, DispatchHandler] = {}

        # Lock for thread safety
        self._lock = threading.RLock()

    @property
    def commbus(self) -> "InMemoryCommBus":
        """Get the CommBus instance (lazy initialization)."""
        if self._commbus is None:
            from control_tower.ipc.commbus import get_commbus
            self._commbus = get_commbus(self._logger)
        return self._commbus

    def register_service(
        self,
        descriptor: ServiceDescriptor,
    ) -> bool:
        """Register a service with the kernel."""
        with self._lock:
            if descriptor.name in self._services:
                self._logger.warning(
                    "service_already_registered",
                    service_name=descriptor.name,
                )
                return False

            self._services[descriptor.name] = descriptor

            self._logger.info(
                "service_registered",
                service_name=descriptor.name,
                service_type=descriptor.service_type,
                capabilities=descriptor.capabilities,
            )

            return True

    def unregister_service(self, service_name: str) -> bool:
        """Unregister a service."""
        with self._lock:
            if service_name not in self._services:
                return False

            del self._services[service_name]
            self._handlers.pop(service_name, None)

            self._logger.info(
                "service_unregistered",
                service_name=service_name,
            )

            return True

    def get_service(self, service_name: str) -> Optional[ServiceDescriptor]:
        """Get a service descriptor."""
        with self._lock:
            return self._services.get(service_name)

    def list_services(
        self,
        service_type: Optional[str] = None,
        healthy_only: bool = True,
    ) -> List[ServiceDescriptor]:
        """List registered services."""
        with self._lock:
            result = []
            for svc in self._services.values():
                if service_type and svc.service_type != service_type:
                    continue
                if healthy_only and not svc.healthy:
                    continue
                result.append(svc)
            return result

    def register_handler(
        self,
        service_name: str,
        handler: DispatchHandler,
    ) -> None:
        """Register a local dispatch handler.

        For in-process services, register a handler function that
        will be called directly instead of going through CommBus.

        Args:
            service_name: Name of the service
            handler: Async function that processes envelopes
        """
        with self._lock:
            self._handlers[service_name] = handler

            self._logger.debug(
                "handler_registered",
                service_name=service_name,
            )

    async def dispatch(
        self,
        target: DispatchTarget,
        envelope: Envelope,
    ) -> Envelope:
        """Dispatch a request to a service."""
        service_name = target.service_name

        # Check if service exists
        service = self.get_service(service_name)
        if not service:
            self._logger.error(
                "dispatch_unknown_service",
                service_name=service_name,
            )
            envelope.terminated = True
            envelope.termination_reason = f"Unknown service: {service_name}"
            return envelope

        # Check if service is healthy
        if not service.healthy:
            self._logger.error(
                "dispatch_unhealthy_service",
                service_name=service_name,
            )
            envelope.terminated = True
            envelope.termination_reason = f"Service unhealthy: {service_name}"
            return envelope

        # Update service load
        with self._lock:
            service.current_load += 1

        try:
            # Check for local handler first
            handler = self._handlers.get(service_name)
            if handler:
                self._logger.debug(
                    "dispatch_local",
                    service_name=service_name,
                    method=target.method,
                    envelope_id=envelope.envelope_id,
                )

                # Apply timeout
                try:
                    result = await asyncio.wait_for(
                        handler(envelope),
                        timeout=target.timeout_seconds,
                    )
                    return result
                except asyncio.TimeoutError:
                    self._logger.error(
                        "dispatch_timeout",
                        service_name=service_name,
                        timeout=target.timeout_seconds,
                    )
                    envelope.terminated = True
                    envelope.termination_reason = "Dispatch timeout"
                    return envelope

            # Fall back to CommBus adapter if available
            if self._commbus_adapter:
                self._logger.debug(
                    "dispatch_remote",
                    service_name=service_name,
                    method=target.method,
                )

                # Use adapter (implementation depends on adapter interface)
                result = await self._commbus_adapter.dispatch(
                    service_name=service_name,
                    method=target.method,
                    envelope=envelope,
                    timeout=target.timeout_seconds,
                )
                return result

            # No handler or adapter
            self._logger.error(
                "dispatch_no_handler",
                service_name=service_name,
            )
            envelope.terminated = True
            envelope.termination_reason = f"No handler for service: {service_name}"
            return envelope

        except Exception as e:
            self._logger.error(
                "dispatch_error",
                service_name=service_name,
                error=str(e),
            )

            # Retry logic
            if target.retry_count < target.max_retries:
                target.retry_count += 1
                self._logger.info(
                    "dispatch_retry",
                    service_name=service_name,
                    retry=target.retry_count,
                    max_retries=target.max_retries,
                )
                return await self.dispatch(target, envelope)

            envelope.terminated = True
            envelope.termination_reason = f"Dispatch error: {str(e)}"
            return envelope

        finally:
            with self._lock:
                service.current_load = max(0, service.current_load - 1)

    async def broadcast(
        self,
        event_type: str,
        payload: Dict[str, Any],
    ) -> None:
        """Broadcast an event via CommBus.

        Creates a simple event message and publishes to all subscribers.
        For typed events, use publish_event() instead.
        """
        self._logger.debug(
            "broadcast_event",
            event_type=event_type,
        )

        # Create a simple event wrapper
        from dataclasses import dataclass, field

        @dataclass
        class _DynamicEvent:
            category: str = field(default="event", init=False)
            event_type: str = ""
            payload: Dict[str, Any] = field(default_factory=dict)

        event = _DynamicEvent(event_type=event_type, payload=payload)

        try:
            await self.commbus.publish(event)
        except Exception as e:
            self._logger.error(
                "broadcast_error",
                event_type=event_type,
                error=str(e),
            )

    async def publish_event(self, event: Any) -> None:
        """Publish a typed event via CommBus.

        Use this for proper typed events (e.g., MemoryStored, SessionStateChanged).

        Args:
            event: The event message (must have category="event")
        """
        event_type = type(event).__name__
        self._logger.debug("publish_event", event_type=event_type)

        try:
            await self.commbus.publish(event)
        except Exception as e:
            self._logger.error(
                "publish_event_error",
                event_type=event_type,
                error=str(e),
            )

    async def request(
        self,
        service_name: str,
        query_type: str,
        payload: Dict[str, Any],
        timeout_seconds: float = 30.0,
    ) -> Dict[str, Any]:
        """Send a request to a service and wait for response.

        Uses CommBus query for typed queries, falls back to service dispatch.
        """
        # Check if service exists
        service = self.get_service(service_name)
        if not service:
            return {"error": f"Unknown service: {service_name}"}

        # Check if there's a CommBus handler for this query type
        if self.commbus.has_handler(query_type):
            from dataclasses import dataclass, field

            @dataclass
            class _DynamicQuery:
                category: str = field(default="query", init=False)
                query_type: str = ""
                payload: Dict[str, Any] = field(default_factory=dict)

            query = _DynamicQuery(query_type=query_type, payload=payload)
            try:
                result = await self.commbus.query(query, timeout=timeout_seconds)
                return {"result": result}
            except asyncio.TimeoutError:
                return {"error": "Query timeout"}
            except Exception as e:
                return {"error": str(e)}

        # Fall back to service handler
        handler = self._handlers.get(service_name)
        if handler:
            self._logger.debug(
                "request_via_handler",
                service_name=service_name,
                query_type=query_type,
            )
            # Create a minimal envelope for the request
            ctx_data = payload.get("request_context")
            if ctx_data is None:
                raise ValueError("request_context is required in payload for CommBus request")
            if isinstance(ctx_data, RequestContext):
                request_context = ctx_data
            elif isinstance(ctx_data, dict):
                request_context = RequestContext(**ctx_data)
            else:
                raise TypeError("request_context must be a dict or RequestContext")

            envelope = Envelope(request_context=request_context)
            envelope.metadata = {"query_type": query_type, **payload}
            try:
                result = await asyncio.wait_for(
                    handler(envelope),
                    timeout=timeout_seconds,
                )
                return {"result": result}
            except asyncio.TimeoutError:
                return {"error": "Request timeout"}
            except Exception as e:
                return {"error": str(e)}

        return {"error": f"No handler for service: {service_name}"}

    async def send_query(self, query: Any, timeout: Optional[float] = None) -> Any:
        """Send a typed query via CommBus.

        Use this for proper typed queries (e.g., GetSessionState).

        Args:
            query: The query message (must have category="query")
            timeout: Optional timeout in seconds

        Returns:
            Query response

        Raises:
            ValueError: If no handler registered
            asyncio.TimeoutError: If query times out
        """
        query_type = type(query).__name__
        self._logger.debug("send_query", query_type=query_type)
        return await self.commbus.query(query, timeout=timeout)

    def subscribe(self, event_type: str, handler: Any) -> Callable[[], None]:
        """Subscribe to CommBus events.

        Args:
            event_type: Event type name (e.g., "MemoryStored")
            handler: Handler function (sync or async)

        Returns:
            Unsubscribe function
        """
        return self.commbus.subscribe(event_type, handler)

    def register_query_handler(self, query_type: str, handler: Any) -> None:
        """Register a handler for CommBus queries.

        Args:
            query_type: Query type name (e.g., "GetSessionState")
            handler: Handler function (sync or async)
        """
        self.commbus.register_handler(query_type, handler)
