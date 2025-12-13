"""HTTP CommBus Adapter - Bridge to Go CommBus via HTTP.

This adapter enables CommBusCoordinator to dispatch requests to
services running in the Go CommBus ecosystem via HTTP REST API.

Architecture:
    Python CommBusCoordinator
           ↓ (commbus_adapter.dispatch)
    HttpCommBusAdapter
           ↓ (HTTP POST)
    Go CommBus HTTP Server
           ↓ (internal dispatch)
    Go Service Handler

Note: This is a stub implementation. The actual Go CommBus HTTP server
must be implemented separately. This adapter defines the expected interface.
"""

import asyncio
from typing import Any, Dict, Optional, Protocol

from jeeves_protocols import GenericEnvelope, LoggerProtocol


class CommBusAdapterProtocol(Protocol):
    """Protocol for CommBus adapters.

    Any adapter used with CommBusCoordinator must implement this interface.
    """

    async def dispatch(
        self,
        service_name: str,
        method: str,
        envelope: GenericEnvelope,
        timeout: float,
    ) -> GenericEnvelope:
        """Dispatch request to a service.

        Args:
            service_name: Target service name
            method: Method to invoke on the service
            envelope: Request envelope
            timeout: Timeout in seconds

        Returns:
            Result envelope from the service
        """
        ...

    async def broadcast(
        self,
        event_type: str,
        payload: Dict[str, Any],
    ) -> None:
        """Broadcast an event.

        Args:
            event_type: Type of event (e.g., "process.created")
            payload: Event payload data
        """
        ...

    async def request(
        self,
        service_name: str,
        query_type: str,
        payload: Dict[str, Any],
        timeout: float,
    ) -> Dict[str, Any]:
        """Send a query request.

        Args:
            service_name: Target service name
            query_type: Type of query
            payload: Query payload
            timeout: Timeout in seconds

        Returns:
            Query response data
        """
        ...


class HttpCommBusAdapter:
    """HTTP-based CommBus adapter.

    Bridges Python CommBusCoordinator to Go CommBus via HTTP REST API.

    Expected Go CommBus HTTP endpoints:
        POST /api/dispatch/{service_name}/{method}
        POST /api/publish/{event_type}
        POST /api/query/{service_name}/{query_type}

    Usage:
        adapter = HttpCommBusAdapter(
            host="http://localhost:8090",
            logger=logger,
        )

        coordinator = CommBusCoordinator(
            logger=logger,
            commbus_adapter=adapter,
        )
    """

    def __init__(
        self,
        host: str,
        logger: LoggerProtocol,
        timeout_seconds: float = 30.0,
    ) -> None:
        """Initialize HTTP CommBus adapter.

        Args:
            host: Base URL of Go CommBus HTTP server (e.g., "http://localhost:8090")
            logger: Logger instance
            timeout_seconds: Default request timeout
        """
        self._host = host.rstrip("/")
        self._logger = logger.bind(component="http_commbus_adapter")
        self._timeout = timeout_seconds
        self._client: Optional[Any] = None  # httpx.AsyncClient

    async def _get_client(self) -> Any:
        """Get or create HTTP client."""
        if self._client is None:
            import httpx
            self._client = httpx.AsyncClient(
                timeout=self._timeout,
                headers={"Content-Type": "application/json"},
            )
        return self._client

    async def close(self) -> None:
        """Close the HTTP client."""
        if self._client is not None:
            await self._client.aclose()
            self._client = None

    async def dispatch(
        self,
        service_name: str,
        method: str,
        envelope: GenericEnvelope,
        timeout: float,
    ) -> GenericEnvelope:
        """Dispatch request to service via HTTP.

        Args:
            service_name: Target service name
            method: Method to invoke
            envelope: Request envelope
            timeout: Timeout in seconds

        Returns:
            Result envelope from service

        Raises:
            TimeoutError: If request times out
            ConnectionError: If unable to connect to CommBus
        """
        self._logger.debug(
            "http_dispatch",
            service_name=service_name,
            method=method,
            envelope_id=envelope.envelope_id,
        )

        client = await self._get_client()
        url = f"{self._host}/api/dispatch/{service_name}/{method}"

        try:
            response = await asyncio.wait_for(
                client.post(
                    url,
                    json=envelope.model_dump(mode="json"),
                ),
                timeout=timeout,
            )

            if response.status_code != 200:
                self._logger.error(
                    "http_dispatch_error",
                    status_code=response.status_code,
                    service_name=service_name,
                )
                # Return envelope with error
                envelope.terminated = True
                envelope.termination_reason = f"HTTP dispatch failed: {response.status_code}"
                return envelope

            # Parse response as GenericEnvelope
            data = response.json()
            return GenericEnvelope.model_validate(data)

        except asyncio.TimeoutError:
            self._logger.error(
                "http_dispatch_timeout",
                service_name=service_name,
                timeout=timeout,
            )
            envelope.terminated = True
            envelope.termination_reason = "Dispatch timeout"
            return envelope

        except Exception as e:
            self._logger.error(
                "http_dispatch_exception",
                service_name=service_name,
                error=str(e),
            )
            envelope.terminated = True
            envelope.termination_reason = f"Dispatch error: {str(e)}"
            return envelope

    async def broadcast(
        self,
        event_type: str,
        payload: Dict[str, Any],
    ) -> None:
        """Broadcast event via HTTP.

        Args:
            event_type: Event type
            payload: Event payload
        """
        self._logger.debug(
            "http_broadcast",
            event_type=event_type,
        )

        client = await self._get_client()
        url = f"{self._host}/api/publish/{event_type}"

        try:
            await client.post(url, json=payload)
        except Exception as e:
            self._logger.error(
                "http_broadcast_error",
                event_type=event_type,
                error=str(e),
            )

    async def request(
        self,
        service_name: str,
        query_type: str,
        payload: Dict[str, Any],
        timeout: float,
    ) -> Dict[str, Any]:
        """Send query request via HTTP.

        Args:
            service_name: Target service
            query_type: Query type
            payload: Query payload
            timeout: Timeout in seconds

        Returns:
            Query response data
        """
        self._logger.debug(
            "http_request",
            service_name=service_name,
            query_type=query_type,
        )

        client = await self._get_client()
        url = f"{self._host}/api/query/{service_name}/{query_type}"

        try:
            response = await asyncio.wait_for(
                client.post(url, json=payload),
                timeout=timeout,
            )

            if response.status_code != 200:
                self._logger.error(
                    "http_request_error",
                    status_code=response.status_code,
                )
                return {"error": f"HTTP error: {response.status_code}"}

            return response.json()

        except asyncio.TimeoutError:
            self._logger.error("http_request_timeout", timeout=timeout)
            return {"error": "Request timeout"}

        except Exception as e:
            self._logger.error("http_request_exception", error=str(e))
            return {"error": str(e)}


__all__ = ["HttpCommBusAdapter", "CommBusAdapterProtocol"]
