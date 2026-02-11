//! Core enumerations for envelope and kernel.
//!
//! These are the source of truth, matching Go's envelope/enums.go.

use serde::{Deserialize, Serialize};

/// Why processing terminated (matches proto TerminalReason exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TerminalReason {
    Completed,
    MaxIterationsExceeded,
    MaxLlmCallsExceeded,
    MaxAgentHopsExceeded,
    UserCancelled,
    ToolFailedFatally,
    LlmFailedFatally,
    PolicyViolation,
}

/// Interrupt type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptKind {
    Clarification,
    Confirmation,
    AgentReview,
    Checkpoint,
    ResourceExhausted,
    Timeout,
    SystemError,
}

/// Risk level for tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    // Semantic
    ReadOnly,
    Write,
    Destructive,
    // Severity
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn requires_confirmation(self) -> bool {
        matches!(
            self,
            RiskLevel::Destructive | RiskLevel::High | RiskLevel::Critical
        )
    }
}

/// Tool category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    // Operation types
    Read,
    Write,
    Execute,
    Network,
    System,
    // Organization
    Unified,
    Composite,
    Resilient,
    Standalone,
    Internal,
}

/// Health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Loop control verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopVerdict {
    Proceed,
    LoopBack,
    Advance,
    Escalate,
}

/// Risk approval status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskApproval {
    Approved,
    Denied,
    Pending,
}

/// Tool access level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccess {
    None,
    Read,
    Write,
    All,
}

/// Operation result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Success,
    Error,
    NotFound,
    Timeout,
    ValidationError,
    Partial,
    InvalidParameters,
}
