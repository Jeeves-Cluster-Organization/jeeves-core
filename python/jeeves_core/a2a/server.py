"""A2A Server — Expose jeeves pipelines as A2A-compatible agents.

Implements the Google A2A protocol:
- GET /.well-known/agent.json — Agent Card
- POST /a2a/tasks/send — Create and execute a task (pipeline)
- GET /a2a/tasks/{id} — Get task status
- POST /a2a/tasks/{id}/cancel — Cancel a task

Reuses: PipelineWorker.execute(), KernelClient.get_orchestration_session_state(),
        KernelClient.terminate_process(), KernelClient.register_service(),
        EventBridge, SSEStream.
"""

from __future__ import annotations

import json
import logging
import uuid
from typing import Any, Dict, Optional

from fastapi import APIRouter, Request, HTTPException
from fastapi.responses import JSONResponse

logger = logging.getLogger(__name__)

a2a_router = APIRouter()


# =============================================================================
# Agent Card
# =============================================================================

@a2a_router.get("/.well-known/agent.json")
async def agent_card(request: Request) -> JSONResponse:
    """Return A2A Agent Card describing this agent's capabilities."""
    from jeeves_core.protocols import get_capability_resource_registry

    registry = get_capability_resource_registry()
    service_name = registry.get_default_service() or "jeeves"

    # Build skills from registered pipelines
    skills = []
    ctx = getattr(request.app.state, "context", None)
    if ctx and hasattr(ctx, "pipeline_registry"):
        for pipeline_name in ctx.pipeline_registry.list_pipelines():
            skills.append({
                "id": pipeline_name,
                "name": pipeline_name.replace("_", " ").title(),
                "description": f"Pipeline: {pipeline_name}",
            })

    card = {
        "name": service_name,
        "description": f"{service_name} agent powered by jeeves-core",
        "url": str(request.base_url).rstrip("/"),
        "version": "0.0.1",
        "capabilities": {
            "streaming": True,
            "pushNotifications": False,
            "stateTransitionHistory": True,
        },
        "skills": skills,
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain", "application/json"],
    }

    return JSONResponse(card)


# =============================================================================
# Task Send
# =============================================================================

@a2a_router.post("/tasks/send")
async def task_send(request: Request) -> JSONResponse:
    """Create and execute a task (A2A tasks/send).

    Maps to PipelineWorker.execute() with a new process.
    """
    try:
        body = await request.json()
    except Exception:
        raise HTTPException(status_code=400, detail="Invalid JSON body")

    task_id = body.get("id", str(uuid.uuid4()))
    message = body.get("message", {})
    input_text = ""

    # Extract text from A2A message parts
    parts = message.get("parts", [])
    for part in parts:
        if part.get("type") == "text":
            input_text += part.get("text", "")

    if not input_text:
        input_text = message.get("text", "")

    if not input_text:
        raise HTTPException(status_code=400, detail="No input text in message")

    # Get pipeline config and worker from app state
    ctx = getattr(request.app.state, "context", None)
    if not ctx:
        raise HTTPException(status_code=503, detail="Service not initialized")

    pipeline_worker = getattr(ctx, "pipeline_worker", None)
    if not pipeline_worker:
        raise HTTPException(status_code=503, detail="Pipeline worker not available")

    # Determine which pipeline to use
    pipeline_name = body.get("skill", None)
    pipeline_config = None
    if pipeline_name and hasattr(ctx, "pipeline_configs"):
        pipeline_config = ctx.pipeline_configs.get(pipeline_name)

    if not pipeline_config and hasattr(ctx, "default_pipeline_config"):
        pipeline_config = ctx.default_pipeline_config

    if not pipeline_config:
        raise HTTPException(status_code=400, detail="No pipeline configured")

    # Build envelope
    from jeeves_core.runtime.agents import create_envelope
    from jeeves_core.protocols import RequestContext

    request_context = RequestContext(
        request_id=task_id,
        capability="a2a",
        session_id=body.get("sessionId", str(uuid.uuid4())),
    )

    envelope = create_envelope(
        raw_input=input_text,
        request_context=request_context,
        metadata={"a2a_task_id": task_id, "pipeline": pipeline_name or "default"},
    )

    # Execute pipeline
    try:
        worker_result = await pipeline_worker.execute(
            process_id=task_id,
            pipeline_config=pipeline_config if isinstance(pipeline_config, dict) else pipeline_config.to_kernel_dict(),
            envelope=envelope,
        )

        # Map result to A2A task response
        artifacts = []
        for output_key, output_value in worker_result.envelope.outputs.items():
            if isinstance(output_value, dict):
                response_text = output_value.get("response", output_value.get("final_response", ""))
                if response_text:
                    artifacts.append({
                        "parts": [{"type": "text", "text": str(response_text)}],
                        "name": output_key,
                    })

        status = "completed" if worker_result.terminated else "working"
        if worker_result.terminal_reason and "EXCEEDED" in (worker_result.terminal_reason or ""):
            status = "failed"

        return JSONResponse({
            "id": task_id,
            "status": {"state": status},
            "artifacts": artifacts,
        })

    except Exception as e:
        logger.error("a2a_task_execution_error", extra={"task_id": task_id, "error": str(e)})
        return JSONResponse({
            "id": task_id,
            "status": {"state": "failed", "message": str(e)},
            "artifacts": [],
        }, status_code=500)


# =============================================================================
# Task Get
# =============================================================================

@a2a_router.get("/tasks/{task_id}")
async def task_get(request: Request, task_id: str) -> JSONResponse:
    """Get task status (A2A tasks/get).

    Maps to KernelClient.get_orchestration_session_state().
    """
    ctx = getattr(request.app.state, "context", None)
    if not ctx or not ctx.kernel_client:
        raise HTTPException(status_code=503, detail="Kernel not available")

    try:
        state = await ctx.kernel_client.get_orchestration_session_state(task_id)

        # Map kernel state to A2A status
        if state.terminated:
            if state.terminal_reason == "COMPLETED":
                a2a_status = "completed"
            elif "EXCEEDED" in (state.terminal_reason or ""):
                a2a_status = "failed"
            else:
                a2a_status = "canceled"
        else:
            a2a_status = "working"

        # Build artifacts from envelope outputs
        artifacts = []
        if state.envelope and hasattr(state.envelope, "outputs"):
            for key, val in state.envelope.outputs.items():
                if isinstance(val, dict):
                    text = val.get("response", val.get("final_response", ""))
                    if text:
                        artifacts.append({
                            "parts": [{"type": "text", "text": str(text)}],
                            "name": key,
                        })

        return JSONResponse({
            "id": task_id,
            "status": {
                "state": a2a_status,
                "message": state.terminal_reason or "",
            },
            "artifacts": artifacts,
        })

    except Exception as e:
        logger.error("a2a_task_get_error", extra={"task_id": task_id, "error": str(e)})
        raise HTTPException(status_code=404, detail=f"Task not found: {task_id}")


# =============================================================================
# Task Cancel
# =============================================================================

@a2a_router.post("/tasks/{task_id}/cancel")
async def task_cancel(request: Request, task_id: str) -> JSONResponse:
    """Cancel a task (A2A tasks/cancel).

    Maps to KernelClient.terminate_process().
    """
    ctx = getattr(request.app.state, "context", None)
    if not ctx or not ctx.kernel_client:
        raise HTTPException(status_code=503, detail="Kernel not available")

    try:
        await ctx.kernel_client.terminate_process(task_id, reason="a2a_cancel")

        return JSONResponse({
            "id": task_id,
            "status": {"state": "canceled"},
        })

    except Exception as e:
        logger.error("a2a_task_cancel_error", extra={"task_id": task_id, "error": str(e)})
        raise HTTPException(status_code=500, detail=f"Failed to cancel: {e}")
