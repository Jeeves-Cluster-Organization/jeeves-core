//! Gateway request/response types.

use serde::{Deserialize, Serialize};

use crate::kernel::orchestrator_types::PipelineConfig;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub pipeline_config: PipelineConfig,
    pub input: String,
    #[serde(default = "default_user_id")]
    pub user_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub process_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

fn default_user_id() -> String {
    "anonymous".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponse {
    pub process_id: String,
    pub terminated: bool,
    pub terminal_reason: Option<String>,
    pub outputs: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub processes_total: usize,
    pub active_sessions: usize,
    pub services_healthy: usize,
}

#[derive(Debug, Deserialize)]
pub struct ResolveInterruptRequest {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub approved: Option<bool>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub data: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Agent card for discovery (A2A compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub tools: Vec<ToolCardEntry>,
}

/// Tool entry in an agent card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCardEntry {
    pub name: String,
    pub description: String,
}

/// Task status for A2A task tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    pub task_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<serde_json::Value>,
}
