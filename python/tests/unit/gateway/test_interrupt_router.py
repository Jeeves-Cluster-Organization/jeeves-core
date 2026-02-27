"""Tests for the interrupts router."""

import pytest
from unittest.mock import AsyncMock, MagicMock

from fastapi import FastAPI
from fastapi.testclient import TestClient

from jeeves_airframe.gateway.routers.interrupts import router, InterruptKind
from jeeves_airframe.protocols import InterruptStatus


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_interrupt(
    interrupt_id: str = "int-1",
    kind: str = "clarification",
    user_id: str = "user-1",
    status: str = "pending",
):
    """Build a mock interrupt object."""
    interrupt = MagicMock()
    interrupt.id = interrupt_id
    interrupt.kind = InterruptKind(kind)
    interrupt.user_id = user_id
    interrupt.status = InterruptStatus(status)
    interrupt.request_id = "req-1"
    interrupt.session_id = "sess-1"
    interrupt.to_dict.return_value = {
        "id": interrupt_id,
        "kind": kind,
        "request_id": "req-1",
        "user_id": user_id,
        "session_id": "sess-1",
        "status": status,
        "created_at": "2026-01-01T00:00:00Z",
    }
    return interrupt


def _build_app(interrupt_service=None) -> FastAPI:
    """Build a test FastAPI app with the interrupts router."""
    app = FastAPI()
    app.include_router(router, prefix="/api/v1")
    if interrupt_service:
        app.state.interrupt_service = interrupt_service
    return app


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestGetInterrupt:
    def test_not_found_returns_404(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=None)
        app = _build_app(service)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/bad-id?user_id=user-1")
        assert resp.status_code == 404

    def test_wrong_user_returns_403(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=_make_interrupt(user_id="user-1"))
        app = _build_app(service)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/int-1?user_id=other-user")
        assert resp.status_code == 403

    def test_happy_path(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=_make_interrupt())
        app = _build_app(service)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/int-1?user_id=user-1")
        assert resp.status_code == 200
        data = resp.json()
        assert data["id"] == "int-1"
        assert data["kind"] == "clarification"


class TestRespondToInterrupt:
    def test_not_found_returns_404(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=None)
        app = _build_app(service)
        client = TestClient(app)

        resp = client.post(
            "/api/v1/interrupts/bad-id/respond?user_id=user-1",
            json={"text": "answer"},
        )
        assert resp.status_code == 404

    def test_clarification_missing_text_returns_400(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=_make_interrupt(kind="clarification"))
        app = _build_app(service)
        client = TestClient(app)

        resp = client.post(
            "/api/v1/interrupts/int-1/respond?user_id=user-1",
            json={"approved": True},  # wrong field for clarification
        )
        assert resp.status_code == 400

    def test_confirmation_missing_approved_returns_400(self):
        service = AsyncMock()
        service.get_interrupt = AsyncMock(return_value=_make_interrupt(kind="confirmation"))
        app = _build_app(service)
        client = TestClient(app)

        resp = client.post(
            "/api/v1/interrupts/int-1/respond?user_id=user-1",
            json={"text": "yes"},  # wrong field for confirmation
        )
        assert resp.status_code == 400


class TestNoService:
    def test_no_interrupt_service_returns_503(self):
        app = _build_app(interrupt_service=None)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/int-1?user_id=user-1")
        assert resp.status_code == 503


class TestListInterrupts:
    def test_empty_list(self):
        service = AsyncMock()
        service.get_pending_for_session = AsyncMock(return_value=[])
        app = _build_app(service)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/?user_id=user-1&session_id=sess-1")
        assert resp.status_code == 200
        assert resp.json()["total"] == 0

    def test_invalid_kind_returns_400(self):
        service = AsyncMock()
        app = _build_app(service)
        client = TestClient(app)

        resp = client.get("/api/v1/interrupts/?user_id=user-1&session_id=sess-1&kind=bogus")
        assert resp.status_code == 400
