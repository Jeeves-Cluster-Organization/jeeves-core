//! HTTP gateway — axum server.
//!
//! Routes:
//! - POST /api/v1/chat/messages     — run a pipeline with input
//! - POST /api/v1/pipelines/run     — run a pipeline (alternative)
//! - GET  /api/v1/sessions/:id      — get session state
//! - GET  /health                    — health check
//! - GET  /ready                     — readiness check
//! - GET  /api/v1/status             — system status

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

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
        .route("/api/v1/pipelines/run", post(run_pipeline))
        // Session query
        .route("/api/v1/sessions/{id}", get(get_session))
        // System
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/api/v1/status", get(system_status))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// =============================================================================
// Request/Response types
// =============================================================================

#[derive(Debug, Deserialize)]
struct ChatRequest {
    /// Pipeline configuration (JSON).
    pipeline_config: PipelineConfig,
    /// Raw user input.
    input: String,
    /// User identifier.
    #[serde(default = "default_user_id")]
    user_id: String,
    /// Session identifier (optional — generated if absent).
    #[serde(default)]
    session_id: Option<String>,
    /// Process identifier (optional — generated if absent).
    #[serde(default)]
    process_id: Option<String>,
    /// Optional metadata.
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

async fn run_pipeline_inner(
    state: AppState,
    req: ChatRequest,
) -> std::result::Result<Json<ChatResponse>, (StatusCode, Json<ErrorResponse>)> {
    let process_id = req
        .process_id
        .unwrap_or_else(|| format!("proc_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]));
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("sess_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]));

    let pid = ProcessId::from_string(process_id).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "INVALID_PROCESS_ID".to_string(),
            }),
        )
    })?;

    // Build envelope with optional metadata
    let envelope = Envelope::new_minimal(&req.user_id, &session_id, &req.input, req.metadata);

    // Initialize session
    let _session = state
        .handle
        .initialize_session(pid.clone(), req.pipeline_config, envelope, false)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                    code: "INIT_FAILED".to_string(),
                }),
            )
        })?;

    // Run pipeline loop
    match crate::worker::run_pipeline_loop(&state.handle, &pid, &state.agents).await {
        Ok(result) => Ok(Json(ChatResponse {
            process_id: result.process_id.as_str().to_string(),
            terminated: result.terminated,
            terminal_reason: result.terminal_reason.map(|r| format!("{:?}", r)),
            outputs: serde_json::to_value(result.outputs).unwrap_or_default(),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "PIPELINE_FAILED".to_string(),
            }),
        )),
    }
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
    // Verify kernel actor is responsive
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
