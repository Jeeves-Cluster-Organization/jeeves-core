//! HTTP gateway — axum server.
//!
//! Routes:
//! - POST /api/v1/chat/messages     — run a pipeline with input (buffered)
//! - POST /api/v1/chat/stream       — run a pipeline with SSE streaming
//! - POST /api/v1/pipelines/run     — run a pipeline (alternative, buffered)
//! - GET  /api/v1/sessions/:id      — get session state
//! - GET  /api/v1/agents            — list registered agents
//! - GET  /api/v1/agents/:name/card — get agent card
//! - GET  /api/v1/tasks/:id         — get task status
//! - GET  /api/v1/tools             — list registered tools
//! - GET  /health                   — health check
//! - GET  /ready                    — readiness check
//! - GET  /api/v1/status            — system status

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::instrument;

use crate::envelope::Envelope;
use crate::types::ProcessId;
use crate::worker::agent::AgentRegistry;
use crate::worker::handle::KernelHandle;
use crate::worker::tools::ToolRegistry;

use super::gateway_errors::{bad_request, map_init_error, map_pipeline_error, GatewayError};
use super::gateway_types::*;

/// Shared application state.
#[derive(Clone, Debug)]
pub struct AppState {
    pub handle: KernelHandle,
    pub agents: Arc<AgentRegistry>,
    pub tools: Arc<ToolRegistry>,
}

/// Build the axum router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Chat/pipeline endpoints
        .route("/api/v1/chat/messages", post(chat_messages))
        .route("/api/v1/chat/stream", post(chat_stream))
        .route("/api/v1/pipelines/run", post(run_pipeline))
        // Session query
        .route("/api/v1/sessions/{id}", get(get_session))
        // Discovery & A2A
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents/{name}/card", get(get_agent_card))
        .route("/api/v1/tasks/{process_id}", get(get_task_status))
        .route("/api/v1/tools", get(list_tools))
        // Interrupt resolution
        .route("/api/v1/interrupts/{process_id}/{interrupt_id}/resolve", post(resolve_interrupt))
        // System
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/status", get(system_status))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024)) // 2MB
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// =============================================================================
// Request parsing
// =============================================================================

/// Parse process_id, session_id, and envelope from a ChatRequest.
fn parse_request(
    req: &mut ChatRequest,
) -> std::result::Result<(ProcessId, Envelope), GatewayError> {
    let process_id = req
        .process_id
        .take()
        .unwrap_or_else(|| format!("proc_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]));
    let session_id = req
        .session_id
        .take()
        .unwrap_or_else(|| format!("sess_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]));

    let pid = ProcessId::from_string(process_id)
        .map_err(|e| bad_request(e, "INVALID_PROCESS_ID"))?;
    let envelope =
        Envelope::new_minimal(&req.user_id, &session_id, &req.input, req.metadata.take());

    Ok((pid, envelope))
}

// =============================================================================
// Handlers
// =============================================================================

async fn chat_messages(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    run_pipeline_inner(state, req).await
}

async fn run_pipeline(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    run_pipeline_inner(state, req).await
}

#[instrument(skip_all)]
async fn run_pipeline_inner(
    state: AppState,
    mut req: ChatRequest,
) -> std::result::Result<Json<ChatResponse>, GatewayError> {
    let (pid, envelope) = parse_request(&mut req)?;

    state
        .handle
        .initialize_session(pid.clone(), req.pipeline_config, envelope, false)
        .await
        .map_err(map_init_error)?;

    let result = crate::worker::run_pipeline_loop(&state.handle, &pid, &state.agents, None)
        .await
        .map_err(map_pipeline_error)?;

    Ok(Json(ChatResponse {
        process_id: result.process_id.as_str().to_string(),
        terminated: result.terminated,
        terminal_reason: result.terminal_reason.map(|r| format!("{:?}", r)),
        outputs: serde_json::to_value(result.outputs).unwrap_or_default(),
    }))
}

/// SSE streaming endpoint — runs a pipeline and streams PipelineEvents.
#[instrument(skip_all)]
async fn chat_stream(
    State(state): State<AppState>,
    Json(mut req): Json<ChatRequest>,
) -> std::result::Result<
    Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>,
    GatewayError,
> {
    let (pid, envelope) = parse_request(&mut req)?;

    let (_jh, rx) = crate::worker::run_pipeline_streaming(
        state.handle.clone(),
        pid,
        req.pipeline_config,
        envelope,
        state.agents.clone(),
    )
    .await
    .map_err(map_init_error)?;

    let stream = futures::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|event| {
            let sse_event = Event::default()
                .event(event.event_type())
                .data(serde_json::to_string(&event).unwrap_or_default());
            (Ok(sse_event), rx)
        })
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<serde_json::Value>, GatewayError> {
    let pid = ProcessId::from_string(id).map_err(|e| bad_request(e, "INVALID_PROCESS_ID"))?;

    let session_state = state.handle.get_session_state(&pid).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "NOT_FOUND".to_string(),
                error_id: None,
                details: None,
            }),
        )
    })?;

    Ok(Json(serde_json::to_value(session_state).unwrap_or_default()))
}

// =============================================================================
// Discovery & A2A Handlers
// =============================================================================

async fn list_agents(
    State(state): State<AppState>,
) -> Json<Vec<AgentCard>> {
    let cards: Vec<AgentCard> = state
        .agents
        .list_names()
        .into_iter()
        .map(|name| AgentCard { name, tools: vec![] })
        .collect();

    Json(cards)
}

async fn get_agent_card(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> std::result::Result<Json<AgentCard>, GatewayError> {
    if state.agents.get(&name).is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Agent '{}' not found", name),
                code: "NOT_FOUND".to_string(),
                error_id: None,
                details: None,
            }),
        ));
    }

    Ok(Json(AgentCard { name, tools: vec![] }))
}

async fn list_tools(
    State(state): State<AppState>,
) -> Json<Vec<ToolCardEntry>> {
    let entries: Vec<ToolCardEntry> = state
        .tools
        .list_all_tools()
        .into_iter()
        .map(|t| ToolCardEntry {
            name: t.name,
            description: t.description,
        })
        .collect();

    Json(entries)
}

async fn get_task_status(
    State(state): State<AppState>,
    Path(process_id): Path<String>,
) -> std::result::Result<Json<TaskStatus>, GatewayError> {
    let pid = ProcessId::from_string(process_id.clone())
        .map_err(|e| bad_request(e, "INVALID_PROCESS_ID"))?;

    let session_state = state.handle.get_session_state(&pid).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "NOT_FOUND".to_string(),
                error_id: None,
                details: None,
            }),
        )
    })?;

    let status_str = if session_state.terminated {
        match &session_state.terminal_reason {
            Some(crate::envelope::TerminalReason::Completed) => "completed",
            Some(_) => "failed",
            None => "completed",
        }
    } else {
        "working"
    };

    let stage_count = session_state.stage_order.len() as f64;
    let current_pos = session_state
        .stage_order
        .iter()
        .position(|s| s == &session_state.current_stage)
        .map(|p| p as f64 + 1.0)
        .unwrap_or(0.0);

    let progress = if stage_count > 0.0 {
        Some(current_pos / stage_count)
    } else {
        None
    };

    Ok(Json(TaskStatus {
        task_id: process_id,
        status: status_str.to_string(),
        current_stage: Some(session_state.current_stage),
        progress,
        terminal_reason: session_state.terminal_reason.map(|r| format!("{:?}", r)),
        outputs: Some(session_state.envelope),
    }))
}

// =============================================================================
// System Handlers
// =============================================================================

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.handle.get_system_status().await;
    Json(serde_json::json!({
        "status": "ready",
        "processes": status.processes_total,
        "sessions": status.active_orchestration_sessions,
    }))
}

async fn system_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let status = state.handle.get_system_status().await;
    Json(StatusResponse {
        processes_total: status.processes_total,
        active_sessions: status.active_orchestration_sessions,
        services_healthy: status.services_healthy,
    })
}

// =============================================================================
// Interrupt Resolution
// =============================================================================

async fn resolve_interrupt(
    State(state): State<AppState>,
    Path((pid_str, interrupt_id)): Path<(String, String)>,
    Json(req): Json<ResolveInterruptRequest>,
) -> std::result::Result<Json<serde_json::Value>, GatewayError> {
    let pid = ProcessId::from_string(pid_str).map_err(|e| bad_request(e, "INVALID_PROCESS_ID"))?;
    let response = crate::envelope::InterruptResponse {
        text: req.text,
        approved: req.approved,
        decision: req.decision,
        data: req.data,
        received_at: chrono::Utc::now(),
    };
    state.handle.resolve_interrupt(&pid, &interrupt_id, response).await.map_err(map_pipeline_error)?;
    Ok(Json(serde_json::json!({"status": "resolved"})))
}
