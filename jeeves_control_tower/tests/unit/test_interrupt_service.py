"""Unit tests for InterruptService.

Tests the unified interrupt handling service for all 7 interrupt kinds:
- clarification
- confirmation
- critic_review
- checkpoint
- resource_exhausted
- timeout
- system_error
"""

import pytest
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock, MagicMock

from jeeves_control_tower.services.interrupt_service import (
    InterruptService,
    InterruptKind,
    InterruptStatus,
    InterruptResponse,
    FlowInterrupt,
    InterruptConfig,
    DEFAULT_INTERRUPT_CONFIGS,
)


@pytest.fixture
def mock_logger():
    """Create a mock logger."""
    logger = MagicMock()
    logger.info = MagicMock()
    logger.warning = MagicMock()
    logger.error = MagicMock()
    return logger


@pytest.fixture
def mock_db():
    """Create a mock database client."""
    db = AsyncMock()
    db.insert = AsyncMock(return_value="test-id")
    db.update = AsyncMock(return_value=1)
    db.fetch_one = AsyncMock(return_value=None)
    db.fetch_all = AsyncMock(return_value=[])
    return db


@pytest.fixture
def mock_webhook_service():
    """Create a mock webhook service."""
    webhook = AsyncMock()
    webhook.emit_event = AsyncMock(return_value=1)
    return webhook


@pytest.fixture
def mock_otel_adapter():
    """Create a mock OpenTelemetry adapter."""
    otel = MagicMock()
    otel.record_event = MagicMock()
    otel.get_trace_context = MagicMock(return_value=None)
    return otel


@pytest.fixture
def service(mock_logger):
    """Create InterruptService with in-memory storage."""
    return InterruptService(
        db=None,  # Use in-memory storage
        logger=mock_logger,
        webhook_service=None,
        otel_adapter=None,
    )


@pytest.fixture
def service_with_db(mock_db, mock_logger, mock_webhook_service, mock_otel_adapter):
    """Create InterruptService with mocked database."""
    return InterruptService(
        db=mock_db,
        logger=mock_logger,
        webhook_service=mock_webhook_service,
        otel_adapter=mock_otel_adapter,
    )


class TestInterruptKindEnum:
    """Test InterruptKind enum has all 7 kinds."""

    def test_all_7_kinds_present(self):
        """Verify all 7 interrupt kinds are defined."""
        assert InterruptKind.CLARIFICATION.value == "clarification"
        assert InterruptKind.CONFIRMATION.value == "confirmation"
        assert InterruptKind.CRITIC_REVIEW.value == "critic_review"
        assert InterruptKind.CHECKPOINT.value == "checkpoint"
        assert InterruptKind.RESOURCE_EXHAUSTED.value == "resource_exhausted"
        assert InterruptKind.TIMEOUT.value == "timeout"
        assert InterruptKind.SYSTEM_ERROR.value == "system_error"

    def test_enum_count(self):
        """Verify exactly 7 interrupt kinds exist."""
        assert len(InterruptKind) == 7


class TestInterruptResponse:
    """Test InterruptResponse dataclass."""

    def test_to_dict_with_text(self):
        """Test to_dict for clarification response."""
        response = InterruptResponse(text="The main.py file")
        result = response.to_dict()

        assert result["text"] == "The main.py file"
        assert "received_at" in result
        assert "approved" not in result

    def test_to_dict_with_approved(self):
        """Test to_dict for confirmation response."""
        response = InterruptResponse(approved=True)
        result = response.to_dict()

        assert result["approved"] is True
        assert "received_at" in result

    def test_to_dict_with_decision(self):
        """Test to_dict for critic review response."""
        response = InterruptResponse(decision="approve")
        result = response.to_dict()

        assert result["decision"] == "approve"
        assert "received_at" in result

    def test_from_dict_roundtrip(self):
        """Test from_dict creates correct object."""
        original = {"text": "answer", "approved": True, "decision": "modify"}
        response = InterruptResponse.from_dict(original)

        assert response.text == "answer"
        assert response.approved is True
        assert response.decision == "modify"


class TestFlowInterrupt:
    """Test FlowInterrupt dataclass."""

    def test_to_dict_includes_all_fields(self):
        """Test to_dict includes all required fields."""
        interrupt = FlowInterrupt(
            id="int-123",
            kind=InterruptKind.CLARIFICATION,
            request_id="req-456",
            user_id="user-789",
            session_id="sess-abc",
            question="What file?",
        )
        result = interrupt.to_dict()

        assert result["id"] == "int-123"
        assert result["kind"] == "clarification"
        assert result["request_id"] == "req-456"
        assert result["user_id"] == "user-789"
        assert result["session_id"] == "sess-abc"
        assert result["question"] == "What file?"
        assert result["status"] == "pending"

    def test_from_db_row_creates_correct_object(self):
        """Test from_db_row parses database row correctly."""
        row = {
            "id": "int-123",
            "kind": "confirmation",
            "request_id": "req-456",
            "user_id": "user-789",
            "session_id": "sess-abc",
            "message": "Execute this?",
            "status": "pending",
            "created_at": datetime.now(timezone.utc),
        }
        interrupt = FlowInterrupt.from_db_row(row)

        assert interrupt.id == "int-123"
        assert interrupt.kind == InterruptKind.CONFIRMATION
        assert interrupt.message == "Execute this?"

    def test_to_db_row_from_db_row_roundtrip(self):
        """Test roundtrip through database format."""
        original = FlowInterrupt(
            id="int-123",
            kind=InterruptKind.CRITIC_REVIEW,
            request_id="req-456",
            user_id="user-789",
            session_id="sess-abc",
            message="Review this plan",
            data={"plan_id": "plan-123"},
        )

        db_row = original.to_db_row()
        restored = FlowInterrupt.from_db_row(db_row)

        assert restored.id == original.id
        assert restored.kind == original.kind
        assert restored.message == original.message
        assert restored.data == original.data


class TestInterruptServiceCreateInterrupt:
    """Test InterruptService.create_interrupt()."""

    @pytest.mark.asyncio
    async def test_create_clarification_interrupt(self, service):
        """Test creating a clarification interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            question="What do you mean?",
        )

        assert interrupt.kind == InterruptKind.CLARIFICATION
        assert interrupt.question == "What do you mean?"
        assert interrupt.status == InterruptStatus.PENDING
        assert interrupt.id is not None

    @pytest.mark.asyncio
    async def test_create_confirmation_interrupt(self, service):
        """Test creating a confirmation interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Delete this file?",
        )

        assert interrupt.kind == InterruptKind.CONFIRMATION
        assert interrupt.message == "Delete this file?"
        assert interrupt.status == InterruptStatus.PENDING

    @pytest.mark.asyncio
    async def test_create_critic_review_interrupt(self, service):
        """Test creating a critic review interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CRITIC_REVIEW,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Review this execution plan",
            data={"plan_summary": "Execute 5 tool calls"},
        )

        assert interrupt.kind == InterruptKind.CRITIC_REVIEW
        assert interrupt.message == "Review this execution plan"
        assert interrupt.data["plan_summary"] == "Execute 5 tool calls"

    @pytest.mark.asyncio
    async def test_create_checkpoint_interrupt(self, service):
        """Test creating a checkpoint interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CHECKPOINT,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            data={"checkpoint_name": "before_execution"},
        )

        assert interrupt.kind == InterruptKind.CHECKPOINT
        assert interrupt.data["checkpoint_name"] == "before_execution"

    @pytest.mark.asyncio
    async def test_create_resource_exhausted_interrupt(self, service):
        """Test creating a resource exhausted interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.RESOURCE_EXHAUSTED,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Rate limit exceeded",
            data={"retry_after_seconds": 60},
        )

        assert interrupt.kind == InterruptKind.RESOURCE_EXHAUSTED
        assert interrupt.data["retry_after_seconds"] == 60

    @pytest.mark.asyncio
    async def test_create_timeout_interrupt(self, service):
        """Test creating a timeout interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.TIMEOUT,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Operation timed out after 300s",
        )

        assert interrupt.kind == InterruptKind.TIMEOUT
        assert interrupt.message == "Operation timed out after 300s"

    @pytest.mark.asyncio
    async def test_create_system_error_interrupt(self, service):
        """Test creating a system error interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.SYSTEM_ERROR,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Database connection failed",
            data={"error_code": "DB_CONN_FAILED"},
        )

        assert interrupt.kind == InterruptKind.SYSTEM_ERROR
        assert interrupt.data["error_code"] == "DB_CONN_FAILED"

    @pytest.mark.asyncio
    async def test_create_all_7_kinds(self, service):
        """Test creating interrupts for all 7 kinds."""
        kinds_created = []

        for kind in InterruptKind:
            interrupt = await service.create_interrupt(
                kind=kind,
                request_id=f"req-{kind.value}",
                user_id="user-test",
                session_id="sess-test",
            )
            kinds_created.append(interrupt.kind)

        assert len(kinds_created) == 7
        assert set(kinds_created) == set(InterruptKind)

    @pytest.mark.asyncio
    async def test_default_ttl_applied(self, service):
        """Test that default TTL from config is applied."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        # Clarification has 24 hour default TTL
        assert interrupt.expires_at is not None
        expected_expiry = interrupt.created_at + timedelta(hours=24)
        # Allow 1 second tolerance
        assert abs((interrupt.expires_at - expected_expiry).total_seconds()) < 1

    @pytest.mark.asyncio
    async def test_custom_ttl_overrides_default(self, service):
        """Test that custom TTL overrides default."""
        custom_ttl = timedelta(minutes=10)
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            ttl=custom_ttl,
        )

        expected_expiry = interrupt.created_at + custom_ttl
        assert abs((interrupt.expires_at - expected_expiry).total_seconds()) < 1


class TestInterruptServiceRespond:
    """Test InterruptService.respond()."""

    @pytest.mark.asyncio
    async def test_respond_to_clarification(self, service):
        """Test responding to a clarification interrupt."""
        # Create interrupt
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            question="Which file?",
        )

        # Respond
        response = InterruptResponse(text="The main.py file")
        resolved = await service.respond(
            interrupt_id=interrupt.id,
            response=response,
            user_id="user-456",
        )

        assert resolved is not None
        assert resolved.status == InterruptStatus.RESOLVED
        assert resolved.response.text == "The main.py file"

    @pytest.mark.asyncio
    async def test_respond_to_confirmation(self, service):
        """Test responding to a confirmation interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Delete this?",
        )

        response = InterruptResponse(approved=True)
        resolved = await service.respond(
            interrupt_id=interrupt.id,
            response=response,
            user_id="user-456",
        )

        assert resolved.status == InterruptStatus.RESOLVED
        assert resolved.response.approved is True

    @pytest.mark.asyncio
    async def test_respond_to_critic_review(self, service):
        """Test responding to a critic review interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CRITIC_REVIEW,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Review plan",
        )

        response = InterruptResponse(decision="approve")
        resolved = await service.respond(
            interrupt_id=interrupt.id,
            response=response,
            user_id="user-456",
        )

        assert resolved.status == InterruptStatus.RESOLVED
        assert resolved.response.decision == "approve"

    @pytest.mark.asyncio
    async def test_respond_validates_user_id(self, service):
        """Test that respond rejects mismatched user_id."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        response = InterruptResponse(text="answer")
        resolved = await service.respond(
            interrupt_id=interrupt.id,
            response=response,
            user_id="wrong-user",  # Different user
        )

        # Should return None for unauthorized access
        assert resolved is None

    @pytest.mark.asyncio
    async def test_respond_rejects_non_pending(self, service):
        """Test that respond rejects already resolved interrupts."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        # First response
        response1 = InterruptResponse(text="first answer")
        await service.respond(interrupt.id, response1, "user-456")

        # Try second response
        response2 = InterruptResponse(text="second answer")
        result = await service.respond(interrupt.id, response2, "user-456")

        # Should be rejected
        assert result is None

    @pytest.mark.asyncio
    async def test_respond_not_found(self, service):
        """Test that respond returns None for unknown interrupt."""
        response = InterruptResponse(text="answer")
        result = await service.respond(
            interrupt_id="nonexistent-id",
            response=response,
            user_id="user-456",
        )

        assert result is None


class TestInterruptServiceExpirePending:
    """Test InterruptService.expire_pending()."""

    @pytest.mark.asyncio
    async def test_expire_pending_expires_old_interrupts(self, service):
        """Test that old interrupts are expired."""
        # Create an interrupt with very short TTL
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            ttl=timedelta(seconds=-1),  # Already expired
        )

        # Expire pending
        count = await service.expire_pending()

        assert count == 1

        # Verify it's expired
        expired = await service.get_interrupt(interrupt.id)
        assert expired.status == InterruptStatus.EXPIRED

    @pytest.mark.asyncio
    async def test_expire_pending_ignores_future_expiry(self, service):
        """Test that future expiry interrupts are not expired."""
        await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            ttl=timedelta(hours=24),  # Far future
        )

        count = await service.expire_pending()

        assert count == 0

    @pytest.mark.asyncio
    async def test_expire_pending_handles_utc_correctly(self, service):
        """Test that expiration uses UTC comparison."""
        # Create with explicit UTC time
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            ttl=timedelta(seconds=-10),  # 10 seconds ago
        )

        count = await service.expire_pending()

        assert count == 1


class TestInterruptServiceQuery:
    """Test InterruptService query methods."""

    @pytest.mark.asyncio
    async def test_get_interrupt_returns_correct_interrupt(self, service):
        """Test get_interrupt finds the right interrupt."""
        created = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            question="Test question",
        )

        fetched = await service.get_interrupt(created.id)

        assert fetched is not None
        assert fetched.id == created.id
        assert fetched.question == "Test question"

    @pytest.mark.asyncio
    async def test_get_pending_for_request(self, service):
        """Test get_pending_for_request finds pending interrupt."""
        await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        pending = await service.get_pending_for_request("req-123")

        assert pending is not None
        assert pending.request_id == "req-123"
        assert pending.status == InterruptStatus.PENDING

    @pytest.mark.asyncio
    async def test_get_pending_for_session(self, service):
        """Test get_pending_for_session finds all pending."""
        await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-1",
            user_id="user-456",
            session_id="sess-789",
        )
        await service.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id="req-2",
            user_id="user-456",
            session_id="sess-789",
        )

        pending = await service.get_pending_for_session("sess-789")

        assert len(pending) == 2

    @pytest.mark.asyncio
    async def test_get_pending_for_session_with_kind_filter(self, service):
        """Test get_pending_for_session with kind filter."""
        await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-1",
            user_id="user-456",
            session_id="sess-789",
        )
        await service.create_interrupt(
            kind=InterruptKind.CONFIRMATION,
            request_id="req-2",
            user_id="user-456",
            session_id="sess-789",
        )

        pending = await service.get_pending_for_session(
            "sess-789",
            kinds=[InterruptKind.CLARIFICATION],
        )

        assert len(pending) == 1
        assert pending[0].kind == InterruptKind.CLARIFICATION


class TestInterruptServiceCancel:
    """Test InterruptService.cancel()."""

    @pytest.mark.asyncio
    async def test_cancel_pending_interrupt(self, service):
        """Test cancelling a pending interrupt."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        cancelled = await service.cancel(interrupt.id, reason="User cancelled")

        assert cancelled is not None
        assert cancelled.status == InterruptStatus.CANCELLED
        assert cancelled.data.get("cancel_reason") == "User cancelled"

    @pytest.mark.asyncio
    async def test_cancel_already_resolved_fails(self, service):
        """Test that cancelling resolved interrupt fails."""
        interrupt = await service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
        )

        # Resolve it first
        await service.respond(
            interrupt.id,
            InterruptResponse(text="answer"),
            "user-456",
        )

        # Try to cancel
        result = await service.cancel(interrupt.id)

        assert result is None


class TestInterruptServiceConvenienceMethods:
    """Test convenience methods for creating specific interrupt types."""

    @pytest.mark.asyncio
    async def test_create_clarification(self, service):
        """Test create_clarification convenience method."""
        interrupt = await service.create_clarification(
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            question="Which function?",
            context={"file": "main.py"},
        )

        assert interrupt.kind == InterruptKind.CLARIFICATION
        assert interrupt.question == "Which function?"
        assert interrupt.data.get("file") == "main.py"

    @pytest.mark.asyncio
    async def test_create_confirmation(self, service):
        """Test create_confirmation convenience method."""
        interrupt = await service.create_confirmation(
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            message="Execute plan?",
            action_data={"tool_count": 5},
        )

        assert interrupt.kind == InterruptKind.CONFIRMATION
        assert interrupt.message == "Execute plan?"
        assert interrupt.data.get("tool_count") == 5

    @pytest.mark.asyncio
    async def test_create_resource_exhausted(self, service):
        """Test create_resource_exhausted convenience method."""
        interrupt = await service.create_resource_exhausted(
            request_id="req-123",
            user_id="user-456",
            session_id="sess-789",
            resource_type="rate_limit",
            retry_after_seconds=60.0,
        )

        assert interrupt.kind == InterruptKind.RESOURCE_EXHAUSTED
        assert interrupt.data.get("resource_type") == "rate_limit"
        assert interrupt.data.get("retry_after_seconds") == 60.0


class TestDefaultInterruptConfigs:
    """Test DEFAULT_INTERRUPT_CONFIGS."""

    def test_all_7_kinds_have_config(self):
        """Test that all 7 interrupt kinds have default config."""
        for kind in InterruptKind:
            assert kind in DEFAULT_INTERRUPT_CONFIGS, f"Missing config for {kind}"

    def test_clarification_config(self):
        """Test clarification default config."""
        config = DEFAULT_INTERRUPT_CONFIGS[InterruptKind.CLARIFICATION]
        assert config.default_ttl == timedelta(hours=24)
        assert config.webhook_event == "interrupt.clarification_needed"

    def test_confirmation_config(self):
        """Test confirmation default config."""
        config = DEFAULT_INTERRUPT_CONFIGS[InterruptKind.CONFIRMATION]
        assert config.default_ttl == timedelta(hours=1)
        assert config.webhook_event == "interrupt.confirmation_needed"

    def test_critic_review_config(self):
        """Test critic review default config."""
        config = DEFAULT_INTERRUPT_CONFIGS[InterruptKind.CRITIC_REVIEW]
        assert config.default_ttl == timedelta(minutes=30)
        assert config.webhook_event == "interrupt.critic_review"

    def test_checkpoint_config(self):
        """Test checkpoint default config."""
        config = DEFAULT_INTERRUPT_CONFIGS[InterruptKind.CHECKPOINT]
        assert config.default_ttl is None  # No expiry
        assert config.auto_expire is False
        assert config.require_response is False

    def test_resource_exhausted_config(self):
        """Test resource exhausted default config."""
        config = DEFAULT_INTERRUPT_CONFIGS[InterruptKind.RESOURCE_EXHAUSTED]
        assert config.require_response is False
