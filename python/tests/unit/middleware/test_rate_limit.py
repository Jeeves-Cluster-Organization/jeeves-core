"""Tests for rate limit middleware."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from jeeves_infra.middleware.rate_limit import (
    RateLimitMiddleware,
    RateLimitError,
    SKIP_ENDPOINTS,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_request(path: str = "/api/v1/chat", user_id: str | None = None, header_user: str | None = None, client_host: str | None = None):
    """Build a mock request object."""
    request = MagicMock()
    request.url = MagicMock()
    request.url.path = path

    # State-based user ID
    if user_id:
        request.state = MagicMock()
        request.state.user = MagicMock()
        request.state.user.id = user_id
    else:
        request.state = MagicMock(spec=[])  # no .user attribute

    # Header-based user ID
    if header_user:
        request.headers = {"X-User-ID": header_user}
    else:
        request.headers = {}

    # Client host fallback
    if client_host:
        request.client = MagicMock()
        request.client.host = client_host
    else:
        request.client = None

    return request


async def _passthrough_next(request):
    """Simulates the next middleware/handler returning a 200 response."""
    response = MagicMock()
    response.status_code = 200
    return response


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestRateLimitAllowed:
    async def test_allowed_request_passes_through(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={"exceeded": False})
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(header_user="user-1")

        response = await middleware(request, _passthrough_next)
        assert response.status_code == 200
        kernel.check_rate_limit.assert_awaited_once()


class TestRateLimitExceeded:
    async def test_exceeded_returns_429(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={
            "exceeded": True,
            "retry_after_seconds": 30,
            "limit": 60,
        })
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(header_user="user-1")

        response = await middleware(request, _passthrough_next)
        # Should return a JSONResponse with status 429
        assert response.status_code == 429


class TestRateLimitKernelDown:
    async def test_kernel_unreachable_returns_503(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(side_effect=ConnectionError("refused"))
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(header_user="user-1")

        response = await middleware(request, _passthrough_next)
        assert response.status_code == 503
        assert response.headers["retry-after"] == "5"


class TestSkipEndpoints:
    @pytest.mark.parametrize("path", ["/health", "/ready", "/metrics"])
    async def test_health_endpoints_skip_rate_limit(self, path: str):
        kernel = AsyncMock()
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(path=path)

        response = await middleware(request, _passthrough_next)
        assert response.status_code == 200
        kernel.check_rate_limit.assert_not_awaited()


class TestUserIdExtraction:
    async def test_user_id_from_state(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={"exceeded": False})
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(user_id="state-user")

        await middleware(request, _passthrough_next)
        kernel.check_rate_limit.assert_awaited_once_with("state-user", "/api/v1/chat")

    async def test_user_id_from_header(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={"exceeded": False})
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(header_user="header-user")

        await middleware(request, _passthrough_next)
        kernel.check_rate_limit.assert_awaited_once_with("header-user", "/api/v1/chat")

    async def test_user_id_from_client_host(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={"exceeded": False})
        middleware = RateLimitMiddleware(kernel)
        request = _make_request(client_host="192.168.1.1")

        await middleware(request, _passthrough_next)
        kernel.check_rate_limit.assert_awaited_once_with("192.168.1.1", "/api/v1/chat")

    async def test_anonymous_fallback(self):
        kernel = AsyncMock()
        kernel.check_rate_limit = AsyncMock(return_value={"exceeded": False})
        middleware = RateLimitMiddleware(kernel)
        request = _make_request()  # no user info at all

        await middleware(request, _passthrough_next)
        kernel.check_rate_limit.assert_awaited_once_with("anonymous", "/api/v1/chat")


class TestRateLimitError:
    def test_error_headers(self):
        error = RateLimitError("user-1", "/api", {
            "limit": 60,
            "retry_after_seconds": 15,
        })
        headers = error.to_response_headers()
        assert headers["X-RateLimit-Limit"] == "60"
        assert headers["X-RateLimit-Remaining"] == "0"
        assert headers["Retry-After"] == "15"

    def test_error_message(self):
        error = RateLimitError("user-1", "/api", {
            "retry_after_seconds": 10,
        })
        assert "user-1" in str(error)
        assert "retry in 10s" in str(error)
