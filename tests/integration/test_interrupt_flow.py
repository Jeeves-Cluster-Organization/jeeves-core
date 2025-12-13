"""Integration tests for the unified flow interrupt system.

Tests:
- Create interrupt -> respond -> kernel resume -> verify stage transition
- Rate limit trigger -> RESOURCE_EXHAUSTED interrupt created
- Concurrent interrupt creation (race condition test)
- Expiration job runs correctly
"""

import asyncio
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List, Optional

import pytest

from jeeves_control_tower.services.interrupt_service import (
    FlowInterrupt,
    InterruptKind,
    InterruptResponse,
    InterruptService,
    InterruptStatus,
)


# =============================================================================
# TEST FIXTURES
# =============================================================================


@pytest.fixture
def interrupt_service():
    """Create a fresh InterruptService with in-memory storage."""
    return InterruptService()


@pytest.fixture
def sample_interrupt_data():
    """Sample data for creating interrupts."""
    return {
        "request_id": "req-123",
        "user_id": "user-abc",
        "session_id": "sess-xyz",
        "envelope_id": "env-456",
    }


# =============================================================================
# CREATE -> RESPOND -> RESUME FLOW TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_clarification_flow_end_to_end(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test complete clarification flow: create -> respond -> resume."""
    # Step 1: Create clarification interrupt
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Which file should I analyze?",
        **sample_interrupt_data,
    )

    assert interrupt is not None
    assert interrupt.kind == InterruptKind.CLARIFICATION
    assert interrupt.status == InterruptStatus.PENDING
    assert interrupt.question == "Which file should I analyze?"

    # Step 2: Verify interrupt is retrievable
    fetched = await interrupt_service.get_interrupt(interrupt.id)
    assert fetched is not None
    assert fetched.id == interrupt.id

    # Step 3: Verify it's in pending for session
    pending = await interrupt_service.get_pending_for_session(
        sample_interrupt_data["session_id"]
    )
    assert len(pending) == 1
    assert pending[0].id == interrupt.id

    # Step 4: Respond to interrupt
    resolved = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="The main.py file"),
        user_id=sample_interrupt_data["user_id"],
    )

    assert resolved is not None
    assert resolved.status == InterruptStatus.RESOLVED
    assert resolved.response is not None
    assert resolved.response.text == "The main.py file"
    assert resolved.resolved_at is not None

    # Step 5: Verify no more pending interrupts
    pending_after = await interrupt_service.get_pending_for_session(
        sample_interrupt_data["session_id"]
    )
    assert len(pending_after) == 0


@pytest.mark.asyncio
async def test_confirmation_flow_approved(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test confirmation flow with approval."""
    # Create confirmation interrupt
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CONFIRMATION,
        message="Delete this file?",
        **sample_interrupt_data,
    )

    assert interrupt.kind == InterruptKind.CONFIRMATION
    assert interrupt.message == "Delete this file?"

    # Approve the action
    resolved = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(approved=True),
        user_id=sample_interrupt_data["user_id"],
    )

    assert resolved.status == InterruptStatus.RESOLVED
    assert resolved.response.approved is True


@pytest.mark.asyncio
async def test_confirmation_flow_denied(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test confirmation flow with denial."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CONFIRMATION,
        message="Execute this risky operation?",
        **sample_interrupt_data,
    )

    resolved = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(approved=False),
        user_id=sample_interrupt_data["user_id"],
    )

    assert resolved.status == InterruptStatus.RESOLVED
    assert resolved.response.approved is False


@pytest.mark.asyncio
async def test_critic_review_flow(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test critic review interrupt flow."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CRITIC_REVIEW,
        message="Review this execution plan",
        data={"plan_id": "plan-123", "steps": ["step1", "step2"]},
        **sample_interrupt_data,
    )

    assert interrupt.kind == InterruptKind.CRITIC_REVIEW
    assert interrupt.data["plan_id"] == "plan-123"

    # Approve the plan
    resolved = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(decision="approve"),
        user_id=sample_interrupt_data["user_id"],
    )

    assert resolved.response.decision == "approve"


# =============================================================================
# RESOURCE EXHAUSTED (RATE LIMIT) TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_resource_exhausted_interrupt(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test RESOURCE_EXHAUSTED interrupt creation for rate limits."""
    interrupt = await interrupt_service.create_resource_exhausted(
        resource_type="llm_api_rate_limit",
        retry_after_seconds=60.0,
        **sample_interrupt_data,
    )

    assert interrupt.kind == InterruptKind.RESOURCE_EXHAUSTED
    assert interrupt.data["resource_type"] == "llm_api_rate_limit"
    assert interrupt.data["retry_after_seconds"] == 60.0
    assert "Resource exhausted" in interrupt.message


@pytest.mark.asyncio
async def test_multiple_resource_exhausted_types(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test different resource exhaustion types."""
    # Rate limit
    rate_limit = await interrupt_service.create_resource_exhausted(
        resource_type="rate_limit",
        retry_after_seconds=30.0,
        **sample_interrupt_data,
    )

    # Different request for token limit
    token_data = {**sample_interrupt_data, "request_id": "req-456"}
    token_limit = await interrupt_service.create_resource_exhausted(
        resource_type="token_limit",
        retry_after_seconds=0,
        **token_data,
    )

    assert rate_limit.data["resource_type"] == "rate_limit"
    assert token_limit.data["resource_type"] == "token_limit"


# =============================================================================
# CONCURRENT ACCESS TESTS (RACE CONDITIONS)
# =============================================================================


@pytest.mark.asyncio
async def test_concurrent_interrupt_creation(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test concurrent interrupt creation doesn't cause data corruption."""
    num_concurrent = 10

    async def create_interrupt(idx: int):
        data = {**sample_interrupt_data, "request_id": f"req-{idx}"}
        return await interrupt_service.create_interrupt(
            kind=InterruptKind.CLARIFICATION,
            question=f"Question {idx}?",
            **data,
        )

    # Create many interrupts concurrently
    tasks = [create_interrupt(i) for i in range(num_concurrent)]
    results = await asyncio.gather(*tasks)

    # Verify all were created with unique IDs
    ids = {r.id for r in results}
    assert len(ids) == num_concurrent

    # Verify all are retrievable
    for interrupt in results:
        fetched = await interrupt_service.get_interrupt(interrupt.id)
        assert fetched is not None
        assert fetched.id == interrupt.id


@pytest.mark.asyncio
async def test_concurrent_respond_to_same_interrupt(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that only one response succeeds for concurrent responses."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Test question",
        **sample_interrupt_data,
    )

    async def respond(response_text: str):
        return await interrupt_service.respond(
            interrupt_id=interrupt.id,
            response=InterruptResponse(text=response_text),
            user_id=sample_interrupt_data["user_id"],
        )

    # Try to respond concurrently
    tasks = [respond(f"Response {i}") for i in range(5)]
    results = await asyncio.gather(*tasks)

    # Only one should succeed (others return None because status changed)
    successful = [r for r in results if r is not None]
    assert len(successful) == 1
    assert successful[0].status == InterruptStatus.RESOLVED


@pytest.mark.asyncio
async def test_concurrent_read_write(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test concurrent reads and writes don't deadlock or corrupt data."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Test",
        **sample_interrupt_data,
    )

    async def read_interrupt():
        for _ in range(10):
            await interrupt_service.get_interrupt(interrupt.id)
            await asyncio.sleep(0.001)

    async def create_new():
        for i in range(5):
            data = {**sample_interrupt_data, "request_id": f"new-req-{i}"}
            await interrupt_service.create_interrupt(
                kind=InterruptKind.CHECKPOINT,
                data={"iteration": i},
                **data,
            )
            await asyncio.sleep(0.001)

    # Run reads and writes concurrently
    await asyncio.gather(
        read_interrupt(),
        read_interrupt(),
        create_new(),
    )

    # Verify original interrupt is intact
    fetched = await interrupt_service.get_interrupt(interrupt.id)
    assert fetched is not None
    assert fetched.question == "Test"


# =============================================================================
# EXPIRATION TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_expiration_job_expires_old_interrupts(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that expiration job correctly expires old interrupts."""
    # Create interrupt with very short TTL
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="This will expire",
        ttl=timedelta(milliseconds=1),  # Expires almost immediately
        **sample_interrupt_data,
    )

    assert interrupt.expires_at is not None

    # Wait a tiny bit for expiration time to pass
    await asyncio.sleep(0.01)

    # Run expiration job
    expired_count = await interrupt_service.expire_pending()

    assert expired_count == 1

    # Verify interrupt is now expired
    fetched = await interrupt_service.get_interrupt(interrupt.id)
    assert fetched.status == InterruptStatus.EXPIRED


@pytest.mark.asyncio
async def test_expiration_job_ignores_resolved_interrupts(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that expiration job doesn't affect resolved interrupts."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Will be resolved",
        ttl=timedelta(milliseconds=1),
        **sample_interrupt_data,
    )

    # Resolve before expiration check
    await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="Response"),
        user_id=sample_interrupt_data["user_id"],
    )

    await asyncio.sleep(0.01)

    # Run expiration job
    expired_count = await interrupt_service.expire_pending()

    assert expired_count == 0  # Already resolved, not expired


@pytest.mark.asyncio
async def test_expiration_job_respects_no_expiry_interrupts(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that interrupts without expiry are not affected."""
    # Checkpoints have no default TTL
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CHECKPOINT,
        data={"stage": "test"},
        **sample_interrupt_data,
    )

    assert interrupt.expires_at is None

    # Run expiration job
    expired_count = await interrupt_service.expire_pending()

    assert expired_count == 0

    # Verify still pending
    fetched = await interrupt_service.get_interrupt(interrupt.id)
    assert fetched.status == InterruptStatus.PENDING


# =============================================================================
# CANCELLATION TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_cancel_pending_interrupt(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test cancelling a pending interrupt."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="To be cancelled",
        **sample_interrupt_data,
    )

    cancelled = await interrupt_service.cancel(
        interrupt_id=interrupt.id, reason="User navigated away"
    )

    assert cancelled is not None
    assert cancelled.status == InterruptStatus.CANCELLED
    assert cancelled.data.get("cancel_reason") == "User navigated away"


@pytest.mark.asyncio
async def test_cannot_cancel_resolved_interrupt(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that resolved interrupts cannot be cancelled."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Will be resolved",
        **sample_interrupt_data,
    )

    await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="Response"),
        user_id=sample_interrupt_data["user_id"],
    )

    cancelled = await interrupt_service.cancel(interrupt_id=interrupt.id)
    assert cancelled is None  # Cannot cancel resolved


# =============================================================================
# ALL INTERRUPT KINDS TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_all_interrupt_kinds_creatable(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that all 7 interrupt kinds can be created and resolved."""
    kinds_and_responses = [
        (InterruptKind.CLARIFICATION, InterruptResponse(text="Answer")),
        (InterruptKind.CONFIRMATION, InterruptResponse(approved=True)),
        (InterruptKind.CRITIC_REVIEW, InterruptResponse(decision="approve")),
        (InterruptKind.CHECKPOINT, InterruptResponse()),
        (InterruptKind.RESOURCE_EXHAUSTED, InterruptResponse()),
        (InterruptKind.TIMEOUT, InterruptResponse()),
        (InterruptKind.SYSTEM_ERROR, InterruptResponse()),
    ]

    for kind, response in kinds_and_responses:
        data = {**sample_interrupt_data, "request_id": f"req-{kind.value}"}
        interrupt = await interrupt_service.create_interrupt(
            kind=kind,
            question="Q" if kind == InterruptKind.CLARIFICATION else None,
            message="M" if kind in (InterruptKind.CONFIRMATION, InterruptKind.CRITIC_REVIEW) else None,
            **data,
        )

        assert interrupt.kind == kind
        assert interrupt.status == InterruptStatus.PENDING

        # Resolve
        resolved = await interrupt_service.respond(
            interrupt_id=interrupt.id,
            response=response,
            user_id=sample_interrupt_data["user_id"],
        )
        assert resolved.status == InterruptStatus.RESOLVED


# =============================================================================
# USER VALIDATION TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_respond_validates_user(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test that respond validates the user ID."""
    interrupt = await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Test",
        **sample_interrupt_data,
    )

    # Try to respond with wrong user
    result = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="Hacked!"),
        user_id="different-user",
    )

    assert result is None  # Should fail validation

    # Original user can still respond
    result = await interrupt_service.respond(
        interrupt_id=interrupt.id,
        response=InterruptResponse(text="Legit response"),
        user_id=sample_interrupt_data["user_id"],
    )

    assert result is not None
    assert result.response.text == "Legit response"


# =============================================================================
# SESSION QUERY TESTS
# =============================================================================


@pytest.mark.asyncio
async def test_get_pending_for_session_filters_by_kind(
    interrupt_service: InterruptService, sample_interrupt_data: Dict[str, Any]
):
    """Test filtering pending interrupts by kind."""
    # Create different kinds
    await interrupt_service.create_interrupt(
        kind=InterruptKind.CLARIFICATION,
        question="Q1",
        request_id="req-1",
        **{k: v for k, v in sample_interrupt_data.items() if k != "request_id"},
    )

    await interrupt_service.create_interrupt(
        kind=InterruptKind.CONFIRMATION,
        message="M1",
        request_id="req-2",
        **{k: v for k, v in sample_interrupt_data.items() if k != "request_id"},
    )

    await interrupt_service.create_interrupt(
        kind=InterruptKind.CHECKPOINT,
        data={},
        request_id="req-3",
        **{k: v for k, v in sample_interrupt_data.items() if k != "request_id"},
    )

    # Get only clarifications
    clarifications = await interrupt_service.get_pending_for_session(
        sample_interrupt_data["session_id"],
        kinds=[InterruptKind.CLARIFICATION],
    )
    assert len(clarifications) == 1
    assert clarifications[0].kind == InterruptKind.CLARIFICATION

    # Get all
    all_pending = await interrupt_service.get_pending_for_session(
        sample_interrupt_data["session_id"]
    )
    assert len(all_pending) == 3
