"""
Chat router - REST + SSE streaming endpoints.

Provides:
- POST /messages - Send message (returns final response)
- GET /stream - SSE streaming of flow events
- GET /sessions - List sessions
- POST /sessions - Create session
- GET /sessions/{session_id} - Get session details
- GET /sessions/{session_id}/messages - List messages in session
"""

from __future__ import annotations

import json
from typing import Optional, List, AsyncIterator

from fastapi import APIRouter, Request, Query, HTTPException, status
from fastapi.responses import StreamingResponse
from pydantic import BaseModel, Field

from jeeves_avionics.gateway.grpc_client import get_grpc_client
from jeeves_avionics.gateway.sse import SSEStream, merge_sse_streams
from jeeves_avionics.logging import get_current_logger
from jeeves_shared.serialization import ms_to_iso

# Import proto types (generated)
try:
    from proto import jeeves_pb2
except ImportError:
    jeeves_pb2 = None

router = APIRouter()


# =============================================================================
# Event Publishing (Constitutional Pattern)
# =============================================================================

async def _publish_unified_event(event: dict):
    """
    Publish unified event to the gateway event bus.

    Constitutional Pattern:
    - Router receives gRPC events and converts to UnifiedEvent
    - Emits to gateway_events bus
    - WebSocket handler subscribes and broadcasts
    - Zero coupling between router and WebSocket implementation

    This enables real-time agent trace visibility on the frontend.

    Args:
        event: Dict containing event data from gRPC FlowEvent payload
    """
    from jeeves_avionics.gateway.event_bus import gateway_events
    from jeeves_protocols.events import (
        UnifiedEvent,
        EventCategory,
        EventSeverity,
    )
    from datetime import datetime, timezone
    import uuid

    # Extract event fields from AgentEvent dict
    event_type = event.get("event_type", "agent.unknown")
    request_id = event.get("request_id", "")
    session_id = event.get("session_id", "")
    timestamp_ms = event.get("timestamp_ms", int(datetime.now(timezone.utc).timestamp() * 1000))
    payload = event.get("payload", {})
    agent_name = event.get("agent_name", "")

    # Determine category from event_type
    if "perception" in event_type or "intent" in event_type or "planner" in event_type or \
       "executor" in event_type or "synthesizer" in event_type or "integration" in event_type or \
       "agent.started" in event_type or "agent.completed" in event_type:
        category = EventCategory.AGENT_LIFECYCLE
    elif "critic" in event_type:
        category = EventCategory.CRITIC_DECISION
    elif "tool" in event_type:
        category = EventCategory.TOOL_EXECUTION
    elif "orchestrator" in event_type or "flow" in event_type:
        category = EventCategory.PIPELINE_FLOW
    elif "stage" in event_type:
        category = EventCategory.STAGE_TRANSITION
    else:
        category = EventCategory.DOMAIN_EVENT

    # Create UnifiedEvent
    dt = datetime.fromtimestamp(timestamp_ms / 1000, tz=timezone.utc)
    unified = UnifiedEvent(
        event_id=str(uuid.uuid4()),
        event_type=event_type,
        category=category,
        timestamp_iso=dt.isoformat(),
        timestamp_ms=timestamp_ms,
        request_id=request_id,
        session_id=session_id,
        user_id=event.get("user_id", ""),
        payload=payload,
        severity=EventSeverity.INFO,
        source="grpc_gateway",
        version="1.0",
    )

    # Emit to gateway event bus
    await gateway_events.emit(unified)


# =============================================================================
# Request/Response Models
# =============================================================================

class MessageSend(BaseModel):
    """Request to send a chat message."""
    message: str = Field(..., min_length=1, max_length=10000)
    session_id: Optional[str] = None  # Creates new session if not provided
    mode: Optional[str] = None  # Capability mode (registered via CapabilityResourceRegistry)
    repo_path: Optional[str] = None  # Repository path for analysis capabilities


class MessageResponse(BaseModel):
    """Response from sending a message."""
    request_id: str
    session_id: str
    status: str  # completed, clarification, confirmation, error
    response: Optional[str] = None
    clarification_needed: bool = False  # P1 compliance: explicit flag for frontend
    clarification_question: Optional[str] = None
    confirmation_message: Optional[str] = None
    confirmation_id: Optional[str] = None
    # Capability-specific fields (registered via CapabilityResourceRegistry)
    files_examined: Optional[List[str]] = None
    citations: Optional[List[str]] = None
    thread_id: Optional[str] = None  # For resuming capability flows


"""
NOTE: ConfirmationSend and ClarificationSend models have been removed.
All interrupt responses now go through the unified /interrupts/{id}/respond endpoint.
See jeeves_avionics/gateway/routers/interrupts.py
"""


class SessionCreate(BaseModel):
    """Request to create a new session."""
    user_id: Optional[str] = None
    title: Optional[str] = None


class SessionResponse(BaseModel):
    """Session data."""
    session_id: str
    user_id: str
    title: Optional[str] = None
    message_count: int = 0
    status: str = "active"
    created_at: str
    last_activity: Optional[str] = None


class SessionListResponse(BaseModel):
    """List of sessions."""
    sessions: List[SessionResponse]
    total: int


class ChatMessageResponse(BaseModel):
    """Chat message data."""
    message_id: str
    session_id: str
    role: str  # user, assistant
    content: str
    created_at: str


class MessageListResponse(BaseModel):
    """List of messages."""
    messages: List[ChatMessageResponse]
    total: int


# =============================================================================
# Chat Message Endpoints
# =============================================================================

@router.post("/messages", response_model=MessageResponse)
async def send_message(
    request: Request,
    body: MessageSend,
    user_id: str = Query(..., min_length=1, max_length=255),
):
    """
    Send a chat message and get the response.

    This is the synchronous endpoint - waits for full response.
    For streaming, use GET /stream with SSE.
    """
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(
            status_code=503,
            detail="gRPC stubs not generated. Run proto compilation first."
        )

    client = get_grpc_client()

    # Build context for gRPC request
    context = {}
    if body.mode:
        context["mode"] = body.mode
    if body.repo_path:
        context["repo_path"] = body.repo_path

    grpc_request = jeeves_pb2.FlowRequest(
        user_id=user_id,
        session_id=body.session_id or "",
        message=body.message,
        context=context,
    )

    # Look up mode configuration from capability registry (constitutional pattern)
    # Avionics R3: No Domain Logic - registry lookup instead of hardcoded mode names
    from jeeves_protocols import get_capability_resource_registry
    mode_registry = get_capability_resource_registry()
    mode_config = mode_registry.get_mode_config(body.mode) if body.mode else None

    try:
        # Consume the stream, collect final result
        final_response = None
        request_id = ""
        session_id = body.session_id or ""

        # Map event types to agent event names for frontend display
        # Only broadcast internal/trace events, NOT final response events
        # (final response is returned via POST, broadcasting it causes duplicates)
        #
        # Extended mapping for 7-agent pipeline visibility:
        # - AGENT_STARTED/COMPLETED: Generic agent lifecycle events
        # - PLAN_CREATED: Planner generates execution plan
        # - TOOL_STARTED/COMPLETED: Traverser tool executions
        # - CRITIC_DECISION: Critic validates results
        # - SYNTHESIZER_COMPLETE: Understanding structure built
        # - STAGE_TRANSITION: Multi-stage execution transitions
        internal_event_types = {
            jeeves_pb2.FlowEvent.FLOW_STARTED,
            jeeves_pb2.FlowEvent.PLAN_CREATED,
            jeeves_pb2.FlowEvent.TOOL_STARTED,
            jeeves_pb2.FlowEvent.TOOL_COMPLETED,
            jeeves_pb2.FlowEvent.CRITIC_DECISION,
            jeeves_pb2.FlowEvent.AGENT_STARTED,
            jeeves_pb2.FlowEvent.AGENT_COMPLETED,
            jeeves_pb2.FlowEvent.SYNTHESIZER_COMPLETE,
            jeeves_pb2.FlowEvent.STAGE_TRANSITION,
            # Note: RESPONSE_READY, CLARIFICATION, CONFIRMATION, ERROR are NOT broadcast
            # because those are returned in the POST response and would cause duplicates
        }

        async for event in client.flow.StartFlow(grpc_request):
            request_id = event.request_id or request_id
            session_id = event.session_id or session_id

            # Parse payload once
            payload = {}
            if event.payload:
                try:
                    payload = json.loads(event.payload)
                except json.JSONDecodeError:
                    pass

            # Publish internal/trace events for frontend visibility (not final responses)
            # Convert gRPC events to UnifiedEvent and publish to gateway_events
            if event.type in internal_event_types:
                # Merge gRPC context with payload for UnifiedEvent creation
                event_data = {
                    "request_id": request_id,
                    "session_id": session_id,
                    "user_id": user_id,
                    **payload
                }
                await _publish_unified_event(event_data)

            if event.type == jeeves_pb2.FlowEvent.RESPONSE_READY:
                final_response = {
                    "status": "completed",
                    "response": payload.get("response_text") or payload.get("response"),
                }
                # Include mode-specific response fields from registry configuration
                # Constitutional: Capabilities register which fields they need in responses
                if mode_config:
                    for field in mode_config.response_fields:
                        if payload.get(field):
                            final_response[field] = payload.get(field)
            elif event.type == jeeves_pb2.FlowEvent.CLARIFICATION:
                _logger.info(
                    "gateway_received_clarification",
                    request_id=request_id,
                    session_id=session_id,
                    clarification_question=payload.get("question"),
                    thread_id=payload.get("thread_id"),
                )
                final_response = {
                    "status": "clarification",
                    "clarification_needed": True,  # P1 compliance: frontend checks this flag
                    "clarification_question": payload.get("question"),
                }
                # Include mode-specific fields for clarification responses
                if mode_config:
                    for field in mode_config.response_fields:
                        if payload.get(field):
                            final_response[field] = payload.get(field)
            elif event.type == jeeves_pb2.FlowEvent.CONFIRMATION:
                final_response = {
                    "status": "confirmation",
                    "confirmation_needed": True,  # Flag for frontend
                    "confirmation_message": payload.get("message"),
                    "confirmation_id": payload.get("confirmation_id"),
                }
            elif event.type == jeeves_pb2.FlowEvent.ERROR:
                final_response = {
                    "status": "error",
                    "response": payload.get("error", "Unknown error"),
                }

        if final_response is None:
            raise HTTPException(
                status_code=500,
                detail="No response received from orchestrator"
            )

        _logger.info(
            "gateway_returning_response",
            request_id=request_id,
            session_id=session_id,
            status=final_response.get("status"),
            has_clarification=bool(final_response.get("clarification_question")),
        )

        return MessageResponse(
            request_id=request_id,
            session_id=session_id,
            **final_response,
        )

    except Exception as e:
        _logger.error("chat_message_failed", error=str(e), user_id=user_id)
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/stream")
async def stream_chat(
    request: Request,
    user_id: str = Query(..., min_length=1, max_length=255),
    message: str = Query(..., min_length=1, max_length=10000),
    session_id: Optional[str] = Query(None),
    mode: Optional[str] = Query(None, description="Capability mode (registered via CapabilityResourceRegistry)"),
    repo_path: Optional[str] = Query(None, description="Repository path for analysis capabilities"),
):
    """
    SSE streaming endpoint for chat.

    Streams flow events as they happen:
    - flow_started: Flow initiated
    - token: Streaming token from LLM
    - plan_created: Execution plan generated
    - tool_started: Tool execution started
    - tool_completed: Tool execution completed
    - response_ready: Final response
    - clarification: Needs clarification
    - confirmation: Needs confirmation
    - error: Error occurred

    Capability modes are registered via CapabilityResourceRegistry at startup.
    Use the registered mode name in the 'mode' parameter.

    Example usage:
        const eventSource = new EventSource('/api/v1/chat/stream?user_id=u1&message=hello');
        eventSource.onmessage = (e) => console.log(JSON.parse(e.data));

    With capability mode:
        /api/v1/chat/stream?user_id=u1&message=How%20does%20the%20pipeline%20work&mode=<registered_mode>
    """
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(
            status_code=503,
            detail="gRPC stubs not generated"
        )

    client = get_grpc_client()

    async def event_generator() -> AsyncIterator[str]:
        stream = SSEStream()

        # Build context
        context = {}
        if mode:
            context["mode"] = mode
        if repo_path:
            context["repo_path"] = repo_path

        grpc_request = jeeves_pb2.FlowRequest(
            user_id=user_id,
            session_id=session_id or "",
            message=message,
            context=context,
        )

        try:
            async for event in client.flow.StartFlow(grpc_request):
                # Parse payload first
                payload = json.loads(event.payload) if event.payload else {}

                # Map gRPC event type to SSE event name (use specific name from payload if available)
                event_name = _event_type_to_name(event.type, payload)

                # Add common fields
                payload["request_id"] = event.request_id
                payload["session_id"] = event.session_id
                payload["timestamp_ms"] = event.timestamp_ms

                yield stream.event(payload, event=event_name)

            yield stream.done()

        except Exception as e:
            _logger.error("chat_stream_error", error=str(e))
            yield stream.error(str(e))

    return StreamingResponse(
        merge_sse_streams(event_generator()),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",  # Disable nginx buffering
        }
    )


"""
REMOVED: /confirmations and /clarifications endpoints

These endpoints have been replaced by the unified interrupt system:
- POST /interrupts/{id}/respond

See jeeves_avionics/gateway/routers/interrupts.py for the unified implementation.

Migration path:
- Old: POST /chat/confirmations with {confirmation_id, response}
- New: POST /interrupts/{confirmation_id}/respond with {approved: true/false}

- Old: POST /chat/clarifications with {thread_id, clarification}
- New: POST /interrupts/{interrupt_id}/respond with {text: "..."}
"""


# =============================================================================
# Session Endpoints
# =============================================================================

@router.post("/sessions", response_model=SessionResponse)
async def create_session(
    request: Request,
    body: SessionCreate,
    user_id: str = Query(None, min_length=1, max_length=255),
):
    """Create a new chat session."""
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(status_code=503, detail="gRPC stubs not generated")

    # Get user_id from body or query param
    actual_user_id = user_id
    if hasattr(body, 'user_id') and body.user_id:
        actual_user_id = body.user_id
    if not actual_user_id:
        raise HTTPException(status_code=400, detail="user_id is required")

    client = get_grpc_client()

    grpc_request = jeeves_pb2.CreateSessionRequest(
        user_id=actual_user_id,
        title=body.title or "",
    )

    try:
        response = await client.flow.CreateSession(grpc_request)

        return SessionResponse(
            session_id=response.session_id,
            user_id=response.user_id,
            title=response.title or None,
            message_count=0,
            status="active",
            created_at=ms_to_iso(response.created_at_ms),
            last_activity=None,
        )

    except Exception as e:
        _logger.error("create_session_failed", error=str(e))
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/sessions", response_model=SessionListResponse)
async def list_sessions(
    request: Request,
    user_id: str = Query(..., min_length=1, max_length=255),
    limit: int = Query(50, ge=1, le=200),
    offset: int = Query(0, ge=0),
    include_deleted: bool = Query(False),
):
    """List chat sessions for a user."""
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(status_code=503, detail="gRPC stubs not generated")

    client = get_grpc_client()

    grpc_request = jeeves_pb2.ListSessionsRequest(
        user_id=user_id,
        limit=limit,
        offset=offset,
        include_deleted=include_deleted,
    )

    try:
        response = await client.flow.ListSessions(grpc_request)

        sessions = [
            SessionResponse(
                session_id=s.session_id,
                user_id=s.user_id,
                title=s.title or None,
                message_count=s.message_count,
                status=s.status,
                created_at=ms_to_iso(s.created_at_ms),
                last_activity=ms_to_iso(s.last_activity_ms) if s.last_activity_ms else None,
            )
            for s in response.sessions
        ]

        return SessionListResponse(sessions=sessions, total=response.total)

    except Exception as e:
        _logger.error("list_sessions_failed", error=str(e))
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/sessions/{session_id}", response_model=SessionResponse)
async def get_session(
    request: Request,
    session_id: str,
    user_id: str = Query(..., min_length=1, max_length=255),
):
    """Get session details."""
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(status_code=503, detail="gRPC stubs not generated")

    client = get_grpc_client()

    grpc_request = jeeves_pb2.GetSessionRequest(
        session_id=session_id,
        user_id=user_id,
    )

    try:
        s = await client.flow.GetSession(grpc_request)

        return SessionResponse(
            session_id=s.session_id,
            user_id=s.user_id,
            title=s.title or None,
            message_count=s.message_count,
            status=s.status,
            created_at=ms_to_iso(s.created_at_ms),
            last_activity=ms_to_iso(s.last_activity_ms) if s.last_activity_ms else None,
        )

    except Exception as e:
        _logger.error("get_session_failed", error=str(e), session_id=session_id)
        raise HTTPException(status_code=404, detail="Session not found")


@router.get("/sessions/{session_id}/messages", response_model=MessageListResponse)
async def get_session_messages(
    request: Request,
    session_id: str,
    user_id: str = Query(..., min_length=1, max_length=255),
    limit: int = Query(100, ge=1, le=500),
    offset: int = Query(0, ge=0),
):
    """Get messages for a session."""
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(status_code=503, detail="gRPC stubs not generated")

    client = get_grpc_client()

    grpc_request = jeeves_pb2.GetSessionMessagesRequest(
        session_id=session_id,
        user_id=user_id,
        limit=limit,
        offset=offset,
    )

    try:
        response = await client.flow.GetSessionMessages(grpc_request)

        messages = [
            ChatMessageResponse(
                message_id=m.message_id,
                session_id=m.session_id,
                role=m.role,
                content=m.content,
                created_at=ms_to_iso(m.created_at_ms),
            )
            for m in response.messages
        ]

        return MessageListResponse(messages=messages, total=response.total)

    except Exception as e:
        _logger.error("get_session_messages_failed", error=str(e), session_id=session_id)
        raise HTTPException(status_code=500, detail=str(e))


@router.delete("/sessions/{session_id}", status_code=204)
async def delete_session(
    request: Request,
    session_id: str,
    user_id: str = Query(..., min_length=1, max_length=255),
):
    """Delete a session."""
    _logger = get_current_logger()
    if jeeves_pb2 is None:
        raise HTTPException(status_code=503, detail="gRPC stubs not generated")

    client = get_grpc_client()

    grpc_request = jeeves_pb2.DeleteSessionRequest(
        session_id=session_id,
        user_id=user_id,
    )

    try:
        response = await client.flow.DeleteSession(grpc_request)

        if not response.success:
            raise HTTPException(status_code=404, detail="Session not found")

        return None

    except HTTPException:
        raise
    except Exception as e:
        _logger.error("delete_session_failed", error=str(e), session_id=session_id)
        raise HTTPException(status_code=500, detail=str(e))


# =============================================================================
# Helpers
# =============================================================================

def _event_type_to_name(event_type: int, payload: dict = None) -> str:
    """
    Map gRPC event type enum to SSE event name.

    If payload contains a specific 'event' field (e.g., "perception.started"),
    use that for more granular event names. Otherwise, use the generic mapping.
    """
    if jeeves_pb2 is None:
        return "unknown"

    # Check if payload has a more specific event name
    if payload and "event" in payload:
        return payload["event"]

    mapping = {
        jeeves_pb2.FlowEvent.FLOW_STARTED: "flow_started",
        jeeves_pb2.FlowEvent.TOKEN: "token",
        jeeves_pb2.FlowEvent.PLAN_CREATED: "plan_created",
        jeeves_pb2.FlowEvent.TOOL_STARTED: "tool_started",
        jeeves_pb2.FlowEvent.TOOL_COMPLETED: "tool_completed",
        jeeves_pb2.FlowEvent.RESPONSE_READY: "response_ready",
        jeeves_pb2.FlowEvent.CLARIFICATION: "clarification",
        jeeves_pb2.FlowEvent.CONFIRMATION: "confirmation",
        jeeves_pb2.FlowEvent.CRITIC_DECISION: "critic_decision",
        jeeves_pb2.FlowEvent.ERROR: "error",
        jeeves_pb2.FlowEvent.AGENT_STARTED: "agent_started",
        jeeves_pb2.FlowEvent.AGENT_COMPLETED: "agent_completed",
        jeeves_pb2.FlowEvent.SYNTHESIZER_COMPLETE: "synthesizer_completed",
        jeeves_pb2.FlowEvent.STAGE_TRANSITION: "stage_transition",
    }
    return mapping.get(event_type, "unknown")
