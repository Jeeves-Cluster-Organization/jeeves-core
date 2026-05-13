//! Core enumerations for run and kernel.
//!
//! Canonical enum definitions for the Jeeves kernel.

use serde::{Deserialize, Serialize};

/// Why processing terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum TerminalReason {
    Completed,
    MaxIterationsExceeded,
    MaxLlmCallsExceeded,
    MaxAgentHopsExceeded,
    MaxStageVisitsExceeded,
    UserCancelled,
    ToolFailedFatally,
    LlmFailedFatally,
    PolicyViolation,
    BreakRequested,
}

impl TerminalReason {
    /// Classify the terminal reason into a high-level outcome.
    ///
    /// Callers read this field instead of string-matching on reason variants.
    /// Adding new TerminalReason variants only requires updating this match arm.
    pub fn outcome(&self) -> &'static str {
        match self {
            Self::Completed | Self::BreakRequested => "completed",
            Self::MaxIterationsExceeded
            | Self::MaxLlmCallsExceeded
            | Self::MaxAgentHopsExceeded
            | Self::MaxStageVisitsExceeded => "bounds_exceeded",
            _ => "failed",
        }
    }
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
