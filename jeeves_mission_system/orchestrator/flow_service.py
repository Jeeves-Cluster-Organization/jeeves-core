"""
JeevesFlowServicer - gRPC servicer that delegates to Code Analysis pipeline.

This module provides the gRPC interface for the Code Analysis Agent.
All requests are handled by the 6-agent Code Analysis pipeline.

Pipeline: Perception → Intent → Planner → Traverser → Critic → Integration
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any, AsyncIterator, Dict, Optional, TYPE_CHECKING
from uuid import uuid4

from jeeves_protocols import DatabaseClientProtocol, LoggerProtocol
from jeeves_mission_system.adapters import get_logger
from jeeves_shared.serialization import datetime_to_ms

if TYPE_CHECKING:
    pass


# ─────────────────────────────────────────────────────────────────
# gRPC Servicer
# ─────────────────────────────────────────────────────────────────

try:
    import grpc
    from proto import jeeves_pb2, jeeves_pb2_grpc

    _GRPC_AVAILABLE = True
except ImportError:
    jeeves_pb2 = None
    jeeves_pb2_grpc = None
    _GRPC_AVAILABLE = False


if _GRPC_AVAILABLE:

    class JeevesFlowServicer(jeeves_pb2_grpc.JeevesFlowServiceServicer):
        """
        gRPC implementation of JeevesFlowService.

        All requests are delegated to the Code Analysis pipeline.
        This is the only supported mode as of v3.0.

        Services:
        - StartFlow: Process code analysis queries
        - GetSession: Session retrieval
        - ListSessions: Session listing
        - CreateSession: Create new session
        - DeleteSession: Delete session
        - GetSessionMessages: Get messages for session
        """

        def __init__(
            self,
            db: DatabaseClientProtocol,
            code_analysis_servicer: Any,
            logger: Optional[LoggerProtocol] = None,
        ):
            """
            Initialize the servicer.

            Args:
                db: Database client
                code_analysis_servicer: CodeAnalysisServicer instance
                logger: Optional logger instance
            """
            self._logger = logger or get_logger()
            self.db = db
            self.code_analysis_servicer = code_analysis_servicer

        async def StartFlow(
            self,
            request: "jeeves_pb2.FlowRequest",
            context: "grpc.aio.ServicerContext",
        ) -> AsyncIterator["jeeves_pb2.FlowEvent"]:
            """Start a code analysis flow and stream events."""
            user_id = request.user_id
            session_id = request.session_id or None
            message = request.message
            ctx = dict(request.context) if request.context else None

            self._logger.info(
                "code_analysis_request",
                user_id=user_id,
                session_id=session_id,
                message_length=len(message),
            )

            # Delegate to Code Analysis pipeline
            async for event in self.code_analysis_servicer.process_request(
                user_id=user_id,
                session_id=session_id,
                message=message,
                context=ctx,
            ):
                yield event

        async def GetSession(
            self,
            request: "jeeves_pb2.GetSessionRequest",
            context: "grpc.aio.ServicerContext",
        ) -> "jeeves_pb2.Session":
            """Get session details."""
            session_id = request.session_id
            user_id = request.user_id

            try:
                row = await self.db.fetch_one(
                    """
                    SELECT session_id, user_id, title, created_at,
                           (SELECT COUNT(*) FROM messages WHERE messages.session_id = sessions.session_id AND deleted_at IS NULL) as message_count
                    FROM sessions
                    WHERE session_id = :session_id AND user_id = :user_id AND deleted_at IS NULL
                    """,
                    {"session_id": session_id, "user_id": user_id},
                )

                if not row:
                    await context.abort(grpc.StatusCode.NOT_FOUND, "Session not found")
                    return

                return jeeves_pb2.Session(
                    session_id=str(row["session_id"]),
                    user_id=row["user_id"],
                    title=row.get("title") or "",
                    message_count=row.get("message_count", 0),
                    status="active",
                    created_at_ms=datetime_to_ms(row["created_at"]),
                )

            except grpc.RpcError:
                raise
            except Exception as e:
                self._logger.error("get_session_error", error=str(e))
                await context.abort(grpc.StatusCode.INTERNAL, str(e))

        async def ListSessions(
            self,
            request: "jeeves_pb2.ListSessionsRequest",
            context: "grpc.aio.ServicerContext",
        ) -> "jeeves_pb2.ListSessionsResponse":
            """List sessions for a user."""
            user_id = request.user_id
            limit = request.limit or 50
            offset = request.offset or 0

            try:
                deleted_filter = "" if request.include_deleted else "AND deleted_at IS NULL"

                rows = await self.db.fetch_all(
                    f"""
                    SELECT session_id, user_id, title, created_at,
                           (SELECT COUNT(*) FROM messages WHERE messages.session_id = sessions.session_id AND deleted_at IS NULL) as message_count,
                           (SELECT MAX(created_at) FROM messages WHERE messages.session_id = sessions.session_id) as last_activity
                    FROM sessions
                    WHERE user_id = :user_id {deleted_filter}
                    ORDER BY created_at DESC
                    LIMIT :limit OFFSET :offset
                    """,
                    {"user_id": user_id, "limit": limit, "offset": offset},
                )

                count_row = await self.db.fetch_one(
                    f"SELECT COUNT(*) as total FROM sessions WHERE user_id = :user_id {deleted_filter}",
                    {"user_id": user_id},
                )
                total = count_row["total"] if count_row else 0

                sessions = [
                    jeeves_pb2.Session(
                        session_id=str(row["session_id"]),
                        user_id=row["user_id"],
                        title=row.get("title") or "",
                        message_count=row.get("message_count", 0),
                        status="active",
                        created_at_ms=datetime_to_ms(row["created_at"]),
                        last_activity_ms=datetime_to_ms(row["last_activity"])
                        if row.get("last_activity")
                        else 0,
                    )
                    for row in rows
                ]

                return jeeves_pb2.ListSessionsResponse(sessions=sessions, total=total)

            except Exception as e:
                self._logger.error("list_sessions_error", error=str(e))
                await context.abort(grpc.StatusCode.INTERNAL, str(e))

        async def CreateSession(
            self,
            request: "jeeves_pb2.CreateSessionRequest",
            context: "grpc.aio.ServicerContext",
        ) -> "jeeves_pb2.Session":
            """Create a new chat session."""
            user_id = request.user_id
            title = request.title or f"Code Analysis - {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M')}"

            try:
                session_id = str(uuid4())
                now = datetime.now(timezone.utc)

                await self.db.execute(
                    """
                    INSERT INTO sessions (session_id, user_id, title, created_at)
                    VALUES (:session_id, :user_id, :title, :created_at)
                    """,
                    {
                        "session_id": session_id,
                        "user_id": user_id,
                        "title": title,
                        "created_at": now,
                    },
                )

                self._logger.info("session_created", session_id=session_id, user_id=user_id)

                return jeeves_pb2.Session(
                    session_id=session_id,
                    user_id=user_id,
                    title=title,
                    message_count=0,
                    status="active",
                    created_at_ms=datetime_to_ms(now),
                    last_activity_ms=0,
                )

            except Exception as e:
                self._logger.error("create_session_error", error=str(e))
                await context.abort(grpc.StatusCode.INTERNAL, str(e))

        async def DeleteSession(
            self,
            request: "jeeves_pb2.DeleteSessionRequest",
            context: "grpc.aio.ServicerContext",
        ) -> "jeeves_pb2.DeleteSessionResponse":
            """Delete a session (soft delete)."""
            session_id = request.session_id
            user_id = request.user_id

            try:
                result = await self.db.execute(
                    """
                    UPDATE sessions
                    SET deleted_at = :deleted_at
                    WHERE session_id = :session_id AND user_id = :user_id AND deleted_at IS NULL
                    """,
                    {
                        "session_id": session_id,
                        "user_id": user_id,
                        "deleted_at": datetime.now(timezone.utc),
                    },
                )

                success = result.rowcount > 0 if hasattr(result, "rowcount") else True

                self._logger.info(
                    "session_deleted",
                    session_id=session_id,
                    user_id=user_id,
                    success=success,
                )

                return jeeves_pb2.DeleteSessionResponse(success=success)

            except Exception as e:
                self._logger.error("delete_session_error", error=str(e))
                await context.abort(grpc.StatusCode.INTERNAL, str(e))

        async def GetSessionMessages(
            self,
            request: "jeeves_pb2.GetSessionMessagesRequest",
            context: "grpc.aio.ServicerContext",
        ) -> "jeeves_pb2.GetSessionMessagesResponse":
            """Get messages for a session."""
            session_id = request.session_id
            user_id = request.user_id
            limit = request.limit or 100
            offset = request.offset or 0

            try:
                session = await self.db.fetch_one(
                    """
                    SELECT session_id FROM sessions
                    WHERE session_id = :session_id AND user_id = :user_id AND deleted_at IS NULL
                    """,
                    {"session_id": session_id, "user_id": user_id},
                )

                if not session:
                    await context.abort(grpc.StatusCode.NOT_FOUND, "Session not found")
                    return

                rows = await self.db.fetch_all(
                    """
                    SELECT message_id, session_id, role, content, created_at
                    FROM messages
                    WHERE session_id = :session_id AND deleted_at IS NULL
                    ORDER BY created_at ASC
                    LIMIT :limit OFFSET :offset
                    """,
                    {"session_id": session_id, "limit": limit, "offset": offset},
                )

                count_row = await self.db.fetch_one(
                    "SELECT COUNT(*) as total FROM messages WHERE session_id = :session_id AND deleted_at IS NULL",
                    {"session_id": session_id},
                )
                total = count_row["total"] if count_row else 0

                messages = [
                    jeeves_pb2.ChatMessage(
                        message_id=str(row["message_id"]),
                        session_id=str(row["session_id"]),
                        role=row["role"],
                        content=row["content"],
                        created_at_ms=datetime_to_ms(row["created_at"]),
                    )
                    for row in rows
                ]

                return jeeves_pb2.GetSessionMessagesResponse(messages=messages, total=total)

            except grpc.RpcError:
                raise
            except Exception as e:
                self._logger.error("get_session_messages_error", error=str(e))
                await context.abort(grpc.StatusCode.INTERNAL, str(e))

        def _make_event(
            self,
            event_type: int,
            request_id: str,
            session_id: str,
            payload: dict,
        ) -> "jeeves_pb2.FlowEvent":
            """Create a FlowEvent."""
            return jeeves_pb2.FlowEvent(
                type=event_type,
                request_id=request_id,
                session_id=session_id,
                payload=json.dumps(payload).encode(),
                timestamp_ms=int(datetime.now(timezone.utc).timestamp() * 1000),
            )
