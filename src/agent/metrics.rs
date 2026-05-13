//! Metrics produced by agents during execution. The kernel records these
//! against the run; consumers see them in `RunEvent::StageCompleted.metrics`
//! and `WorkerResult.aggregate_metrics`.

use serde::{Deserialize, Serialize};

/// Per-tool-call result reported by the agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub name: String,
    pub success: bool,
    pub latency_ms: u64,
    pub error_type: Option<String>,
}

/// Aggregate metrics from one agent's execution (one Instruction round).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentExecutionMetrics {
    pub llm_calls: i32,
    pub tool_calls: i32,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub duration_ms: i64,
    #[serde(default)]
    pub tool_results: Vec<ToolCallResult>,
}
