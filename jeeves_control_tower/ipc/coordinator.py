"""CommBus Coordinator - IPC manager equivalent.

This implements the kernel's IPC management:
- Service registration (like init daemon registration)
- Message routing (like kernel message passing)
- Request dispatch (like syscall dispatch)

The actual CommBus is in Go. This coordinator provides:
1. Service registry (in-memory, kernel-side)
2. Dispatch logic (route requests to services)
3. Abstraction over CommBus protocol

Layering: ONLY imports from jeeves_protocols (syscall interface).
The actual CommBus communication happens through injected adapters.
"""

import asyncio
import threading
from datetime import datetime
from typing import Any, Callable, Dict, List, Optional

from jeeves_protocols import GenericEnvelope, LoggerProtocol

from jeeves_control_tower.protocols import CommBusCoordinatorProtocol
from jeeves_control_tower.types import DispatchTarget, ServiceDescriptor


# Type alias for dispatch handlers
DispatchHandler = Callable[[GenericEnvelope], "asyncio.Future[GenericEnvelope]"]


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
        commbus_adapter: Optional[Any] = None,  # CommBus adapter if using Go bus
    ) -> None:
        """Initialize CommBus coordinator.

        Args:
            logger: Logger instance
            commbus_adapter: Optional adapter for external CommBus
        """
        self._logger = logger.bind(component="commbus_coordinator")
        self._commbus_adapter = commbus_adapter

        # Service registry
        self._services: Dict[str, ServiceDescriptor] = {}

        # Dispatch handlers (for local services)
        self._handlers: Dict[str, DispatchHandler] = {}

        # Lock for thread safety
        self._lock = threading.RLock()

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
        envelope: GenericEnvelope,
    ) -> GenericEnvelope:
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
        """Broadcast an event via CommBus adapter."""
        self._logger.debug(
            "broadcast_event",
            event_type=event_type,
        )

        # Forward to CommBus adapter if available
        if self._commbus_adapter:
            try:
                await self._commbus_adapter.broadcast(event_type, payload)
            except Exception as e:
                self._logger.error(
                    "broadcast_adapter_error",
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
        """Send a request to a service and wait for response."""
        # Check if service exists
        service = self.get_service(service_name)
        if not service:
            return {"error": f"Unknown service: {service_name}"}

        # Use CommBus adapter if available
        if self._commbus_adapter:
            try:
                return await asyncio.wait_for(
                    self._commbus_adapter.request(
                        service_name=service_name,
                        query_type=query_type,
                        payload=payload,
                    ),
                    timeout=timeout_seconds,
                )
            except asyncio.TimeoutError:
                return {"error": "Request timeout"}
            except Exception as e:
                return {"error": str(e)}

        return {"error": "No CommBus adapter configured"}
