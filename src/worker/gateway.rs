//! HTTP gateway — axum server.
//!
//! Routes:
//! - POST /api/v1/chat/messages     — run a pipeline with input (buffered)
//! - POST /api/v1/chat/stream       — run a pipeline with SSE streaming
//! - POST /api/v1/pipelines/run     — run a pipeline (alternative, buffered)
//! - GET  /api/v1/sessions/:id      — get session state
//! - GET  /health                    — health check
//! - GET  /ready                     — readiness check
//! - GET  /api/v1/status             — system status

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
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::instrument;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::PipelineConfig;
use crate::types::ProcessId;
use crate::worker::agent::AgentRegistry;
use crate::worker::handle::KernelHandle;

/// Shared application state.
#[derive(Clone, Debug)]
pub struct AppState {
    pub handle: KernelHandle,
    pub agents: Arc<AgentRegistry>,
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
// Request/Response types
// =============================================================================

#[derive(Debug, Deserialize)]
struct ChatRequest {
    pipeline_config: PipelineConfig,
    input: String,
    #[serde(default = "default_user_id")]
    user_id: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    process_id: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

fn default_user_id() -> String {
    "anonymous".to_string()
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    process_id: String,
    terminated: bool,
    terminal_reason: Option<String>,
    outputs: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    processes_total: usize,
    active_sessions: usize,
    services_healthy: usize,
}

// =============================================================================
// Error helpers
// =============================================================================

type GatewayError = (StatusCode, Json<ErrorResponse>);

fn bad_request(msg: impl std::fmt::Display, code: &str) -> GatewayError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: msg.to_string(), code: code.to_string() }),
    )
}

fn map_init_error(e: crate::types::Error) -> GatewayError {
    let (status, code) = match &e {
        crate::types::Error::QuotaExceeded(_) => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INIT_FAILED"),
    };
    (status, Json(ErrorResponse { error: e.to_string(), code: code.to_string() }))
}

fn map_pipeline_error(e: crate::types::Error) -> GatewayError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: e.to_string(), code: "PIPELINE_FAILED".to_string() }),
    )
}

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
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let pid = ProcessId::from_string(id).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "INVALID_PROCESS_ID".to_string(),
            }),
        )
    })?;

    let session_state = state.handle.get_session_state(&pid).await.map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "NOT_FOUND".to_string(),
            }),
        )
    })?;

    Ok(Json(serde_json::to_value(session_state).unwrap_or_default()))
}

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
