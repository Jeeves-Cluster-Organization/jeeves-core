use async_trait::async_trait;
use std::sync::Arc;

use crate::llm::ChatMessage;
use crate::tools::{ToolCall, ToolOutput};

/// Decision returned by a `BeforeToolCall` hook.
#[derive(Debug)]
pub enum HookDecision {
    /// Proceed with the tool call as-is.
    Continue,
    /// Skip the tool call; return the given output to the model as the result.
    Replace(ToolOutput),
    /// Abort the turn with an error-style tool result delivered to the model.
    Block { reason: String },
}

#[async_trait]
pub trait BeforeToolCall: Send + Sync + std::fmt::Debug {
    async fn call(&self, call: &ToolCall) -> HookDecision;
}

#[async_trait]
pub trait AfterToolCall: Send + Sync + std::fmt::Debug {
    async fn call(&self, call: &ToolCall, output: &mut ToolOutput);
}

/// Mutates the message window before it's sent to the LLM
/// (e.g. sliding-window compaction, summarization).
pub trait TransformContext: Send + Sync + std::fmt::Debug {
    fn transform(&self, messages: &mut Vec<ChatMessage>);
}

pub type DynBefore = Arc<dyn BeforeToolCall>;
pub type DynAfter = Arc<dyn AfterToolCall>;
pub type DynTransform = Arc<dyn TransformContext>;
