//! Pluggable hooks for the LLM agent's ReAct loop.
//!
//! A single `LlmAgentHook` trait with default-impl methods covers three
//! interception points:
//!
//! - **`before_llm_call`** — fires before each LLM call. Mutate the
//!   message window for compaction, redaction, prompt-injection defense.
//! - **`before_tool_call`** — fires after the LLM emits a tool call and
//!   before it executes. Returns a `HookDecision` to gate the call.
//! - **`after_tool_call`** — fires after a tool result is produced
//!   (whether by execution or via `HookDecision::Replace`). Mutate the
//!   output to redact / decorate.
//!
//! Implementers override only the methods they care about. Hooks register
//! on `LlmAgent` via `AgentFactoryBuilder::with_hook` and run in
//! registration order; for `before_tool_call`, the first non-`Continue`
//! decision wins.
//!
//! Adding a new hook point later (e.g., `before_react_round`,
//! `after_llm_call`) is a new default-impl method on this trait — not a
//! breaking change.

use async_trait::async_trait;
use std::sync::Arc;

use crate::worker::llm::{ChatMessage, ToolCall};
use crate::worker::tools::ToolOutput;

/// Decision returned by `LlmAgentHook::before_tool_call`.
#[derive(Debug)]
pub enum HookDecision {
    /// Proceed with the tool call as-is.
    Continue,
    /// Skip the tool call; the supplied output is delivered to the model
    /// as the tool result without invoking the executor. Useful for
    /// caching layers and canned responses.
    Replace(ToolOutput),
    /// Block the tool call; an error-style tool result is delivered to
    /// the model with the supplied reason and execution is skipped.
    Block { reason: String },
}

/// Lifecycle hook for the LLM agent's ReAct loop.
#[async_trait]
pub trait LlmAgentHook: Send + Sync + std::fmt::Debug {
    /// Mutate the message window before each LLM call.
    async fn before_llm_call(&self, _messages: &mut Vec<ChatMessage>) {}

    /// Decide whether a tool call proceeds. Default: `Continue`.
    async fn before_tool_call(&self, _call: &ToolCall) -> HookDecision {
        HookDecision::Continue
    }

    /// Mutate a tool result after it's produced.
    async fn after_tool_call(&self, _call: &ToolCall, _output: &mut ToolOutput) {}
}

pub type DynHook = Arc<dyn LlmAgentHook>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug)]
    struct DenyBash;

    #[async_trait]
    impl LlmAgentHook for DenyBash {
        async fn before_tool_call(&self, call: &ToolCall) -> HookDecision {
            if call.name == "bash" {
                HookDecision::Block { reason: "bash blocked".to_string() }
            } else {
                HookDecision::Continue
            }
        }
    }

    #[derive(Debug)]
    struct Redact;

    #[async_trait]
    impl LlmAgentHook for Redact {
        async fn after_tool_call(&self, _call: &ToolCall, output: &mut ToolOutput) {
            output.data = json!({"redacted": true});
        }
    }

    #[derive(Debug)]
    struct DropOldest;

    #[async_trait]
    impl LlmAgentHook for DropOldest {
        async fn before_llm_call(&self, messages: &mut Vec<ChatMessage>) {
            if messages.len() > 2 {
                messages.remove(1);
            }
        }
    }

    #[tokio::test]
    async fn before_tool_call_block() {
        let hook = DenyBash;
        let call = ToolCall { id: "1".into(), name: "bash".into(), arguments: json!({}) };
        match hook.before_tool_call(&call).await {
            HookDecision::Block { reason } => assert!(reason.contains("blocked")),
            _ => panic!("expected Block"),
        }
    }

    #[tokio::test]
    async fn before_tool_call_default_continue() {
        let hook = DenyBash;
        let call = ToolCall { id: "1".into(), name: "read".into(), arguments: json!({}) };
        assert!(matches!(hook.before_tool_call(&call).await, HookDecision::Continue));
    }

    #[tokio::test]
    async fn after_tool_call_mutates_output() {
        let hook = Redact;
        let call = ToolCall { id: "1".into(), name: "search".into(), arguments: json!({}) };
        let mut output = ToolOutput::json(json!({"secret": "hunter2"}));
        hook.after_tool_call(&call, &mut output).await;
        assert_eq!(output.data, json!({"redacted": true}));
    }

    #[tokio::test]
    async fn before_llm_call_mutates_messages() {
        let hook = DropOldest;
        let mut messages = vec![
            ChatMessage::system("sys".to_string()),
            ChatMessage::user("oldest".to_string()),
            ChatMessage::user("newest".to_string()),
        ];
        hook.before_llm_call(&mut messages).await;
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn default_impls_no_op_for_unrelated_hook() {
        // A hook that only implements one method must compile and behave as
        // no-op on the others.
        #[derive(Debug)]
        struct OnlyAfter;

        #[async_trait]
        impl LlmAgentHook for OnlyAfter {
            async fn after_tool_call(&self, _: &ToolCall, _: &mut ToolOutput) {}
        }

        let hook = OnlyAfter;
        let call = ToolCall { id: "1".into(), name: "x".into(), arguments: json!({}) };
        assert!(matches!(hook.before_tool_call(&call).await, HookDecision::Continue));

        let mut messages = vec![ChatMessage::user("hi".to_string())];
        hook.before_llm_call(&mut messages).await;
        assert_eq!(messages.len(), 1);
    }
}
