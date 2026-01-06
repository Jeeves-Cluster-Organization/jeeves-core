"""Unit tests for gateway/main.py - FastAPI gateway application.

Tests the HTTP gateway that translates REST/SSE to internal gRPC services.
"""

import pytest
from unittest.mock import Mock, patch, AsyncMock


class TestGatewayConfig:
    """Tests for GatewayConfig class."""

    def test_default_config(self):
        """Test default configuration values."""
        from jeeves_avionics.gateway.main import GatewayConfig

        with patch.dict("os.environ", {}, clear=True):
            config = GatewayConfig()

            assert config.orchestrator_host == "localhost"
            assert config.orchestrator_port == 50051
            assert config.api_host == "0.0.0.0"
            assert config.api_port == 8000
            assert config.debug is False

    def test_respects_env_vars(self):
        """Test that environment variables are respected."""
        from jeeves_avionics.gateway.main import GatewayConfig

        env = {
            "ORCHESTRATOR_HOST": "orchestrator.svc",
            "ORCHESTRATOR_PORT": "50052",
            "API_HOST": "127.0.0.1",
            "API_PORT": "9000",
            "CORS_ORIGINS": "http://localhost:3000,https://app.example.com",
            "DEBUG": "true",
        }
        with patch.dict("os.environ", env, clear=True):
            config = GatewayConfig()

            assert config.orchestrator_host == "orchestrator.svc"
            assert config.orchestrator_port == 50052
            assert config.api_host == "127.0.0.1"
            assert config.api_port == 9000
            assert "http://localhost:3000" in config.cors_origins
            assert config.debug is True


class TestServiceIdentity:
    """Tests for _get_service_identity function."""

    def test_returns_default_service(self):
        """Test that service identity is retrieved from registry."""
        from jeeves_avionics.gateway.main import _get_service_identity

        # Should return "jeeves" as default
        identity = _get_service_identity()
        assert identity is not None
        assert len(identity) > 0


class TestAppCreation:
    """Tests for FastAPI app creation."""

    def test_app_exists(self):
        """Test that the FastAPI app is created."""
        from jeeves_avionics.gateway.main import app

        assert app is not None
        assert app.title is not None

    def test_app_has_health_endpoint(self):
        """Test that health endpoint exists."""
        from jeeves_avionics.gateway.main import app

        # Find health route
        routes = [r.path for r in app.routes if hasattr(r, "path")]
        assert "/health" in routes or any("/health" in r for r in routes)

    def test_app_has_routers_included(self):
        """Test that routers are included."""
        from jeeves_avionics.gateway.main import app

        routes = [r.path for r in app.routes if hasattr(r, "path")]
        # Check that API routes are registered
        api_routes = [r for r in routes if "/api/" in r]
        assert len(api_routes) > 0


class TestHealthEndpoint:
    """Tests for health endpoint."""

    def test_health_route_exists(self):
        """Test that health endpoint route exists."""
        from jeeves_avionics.gateway.main import app

        # Find health route in app routes
        routes = [r.path for r in app.routes if hasattr(r, "path")]
        assert "/health" in routes, "Health endpoint should be registered"


class TestCORSConfiguration:
    """Tests for CORS middleware configuration."""

    def test_cors_middleware_added(self):
        """Test that CORS middleware is configured."""
        from jeeves_avionics.gateway.main import app

        # Check that CORSMiddleware is in the middleware stack
        middleware_classes = [m.cls.__name__ for m in app.user_middleware]
        assert "CORSMiddleware" in middleware_classes

