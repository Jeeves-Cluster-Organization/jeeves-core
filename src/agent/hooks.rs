//! Pluggable hooks. Two trait surfaces, one per observation scope:
//!
//! - **[`AgentHook`]** — fires once per `Agent::process()` invocation at the
//!   kernel runner boundary. Works for every `Agent` impl (LlmAgent,
//!   ToolDelegatingAgent, DeterministicAgent, custom). Sees the
//!   [`AgentContext`] input and the final [`AgentOutput`]. Right scope for
//!   telemetry, audit, output redaction.
//!
//! - **[`LlmAgentHook`]** — fires inside the ReAct loop of `LlmAgent` only.
//!   Three points: `before_llm_call`, `after_llm_call`, plus per-tool
//!   `before_tool_call` / `after_tool_call`. Right scope for message-window
//!   compaction, response inspection, per-tool gating/redaction.
//!
//! Both register on `AgentFactoryBuilder` (`with_agent_hook` /
//! `with_hook`) and run in registration order; for `before_tool_call`, the
//! first non-`Continue` decision wins.

use async_trait::async_trait;
use std::sync::Arc;

use crate::agent::llm::{ChatMessage, ChatResponse, ToolCall};
use crate::agent::{AgentContext, AgentOutput};
use crate::tools::ToolOutput;

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

/// Lifecycle hook for the LLM agent's ReAct loop. Fires only for `LlmAgent`.
#[async_trait]
pub trait LlmAgentHook: Send + Sync + std::fmt::Debug {
    /// Mutate the message window before each LLM call.
    async fn before_llm_call(&self, _messages: &mut Vec<ChatMessage>) {}

    /// Inspect / mutate the response after each LLM call (content,
    /// tool_calls, usage). Fires before tools dispatched in this round.
    async fn after_llm_call(&self, _response: &mut ChatResponse) {}

    /// Decide whether a tool call proceeds. Default: `Continue`.
    async fn before_tool_call(&self, _call: &ToolCall) -> HookDecision {
        HookDecision::Continue
    }

    /// Mutate a tool result after it's produced.
    async fn after_tool_call(&self, _call: &ToolCall, _output: &mut ToolOutput) {}
}

pub type DynHook = Arc<dyn LlmAgentHook>;

/// Wraps every `Agent::process()` call. Works for any `Agent` impl. Fires
/// outside the retry envelope (once per logical execution, not per attempt).
#[async_trait]
pub trait AgentHook: Send + Sync + std::fmt::Debug {
    /// Fires before agent execution begins. Observer-only.
    async fn before_agent(&self, _ctx: &AgentContext) {}

    /// Fires after agent execution returns (success, failure, or interrupt
    /// request). May mutate the output — redaction, decoration, audit
    /// tagging. Consumers branch on `output.interrupt_request.is_some()` if
    /// they need to distinguish suspend from terminal completion.
    async fn after_agent(&self, _ctx: &AgentContext, _output: &mut AgentOutput) {}
}

pub type DynAgentHook = Arc<dyn AgentHook>;

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

    #[tokio::test]
    async fn after_llm_call_default_impl_is_no_op() {
        #[derive(Debug)]
        struct NoOp;
        #[async_trait]
        impl LlmAgentHook for NoOp {}

        use crate::agent::llm::TokenUsage;
        let mut resp = ChatResponse {
            content: Some("hi".to_string()),
            tool_calls: vec![],
            usage: TokenUsage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
            model: "mock".into(),
        };
        NoOp.after_llm_call(&mut resp).await;
        assert_eq!(resp.content.as_deref(), Some("hi"));
    }

    #[tokio::test]
    async fn after_llm_call_can_mutate_response() {
        #[derive(Debug)]
        struct Censor;
        #[async_trait]
        impl LlmAgentHook for Censor {
            async fn after_llm_call(&self, response: &mut ChatResponse) {
                response.content = Some("[redacted]".to_string());
            }
        }

        use crate::agent::llm::TokenUsage;
        let mut resp = ChatResponse {
            content: Some("the password is hunter2".to_string()),
            tool_calls: vec![],
            usage: TokenUsage { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
            model: "mock".into(),
        };
        Censor.after_llm_call(&mut resp).await;
        assert_eq!(resp.content.as_deref(), Some("[redacted]"));
    }

    #[tokio::test]
    async fn agent_hook_default_impls_no_op() {
        #[derive(Debug)]
        struct NoOp;
        #[async_trait]
        impl AgentHook for NoOp {}

        use crate::agent::AgentContext;
        use crate::agent::metrics::AgentExecutionMetrics;
        use std::collections::HashMap;
        use std::sync::Arc;

        let ctx = AgentContext {
            raw_input: "hi".into(),
            outputs: HashMap::new(),
            state: HashMap::new(),
            metadata: HashMap::new(),
            event_tx: None,
            stage_name: None,
            workflow_name: Arc::from("test"),
            max_context_tokens: None,
            context_overflow: None,
            interrupt_response: None,
            response_format: None,
        };
        let mut output = AgentOutput {
            output: json!({"k": "v"}),
            metrics: AgentExecutionMetrics::default(),
            success: true,
            error_message: String::new(),
            interrupt_request: None,
        };

        NoOp.before_agent(&ctx).await;
        NoOp.after_agent(&ctx, &mut output).await;
        assert_eq!(output.output, json!({"k": "v"}));
    }

    #[tokio::test]
    async fn agent_hook_after_can_mutate_output() {
        #[derive(Debug)]
        struct Tag;
        #[async_trait]
        impl AgentHook for Tag {
            async fn after_agent(&self, _ctx: &AgentContext, output: &mut AgentOutput) {
                if let serde_json::Value::Object(ref mut map) = output.output {
                    map.insert("audited".to_string(), json!(true));
                }
            }
        }

        use crate::agent::AgentContext;
        use crate::agent::metrics::AgentExecutionMetrics;
        use std::collections::HashMap;
        use std::sync::Arc;

        let ctx = AgentContext {
            raw_input: "x".into(),
            outputs: HashMap::new(),
            state: HashMap::new(),
            metadata: HashMap::new(),
            event_tx: None,
            stage_name: None,
            workflow_name: Arc::from("t"),
            max_context_tokens: None,
            context_overflow: None,
            interrupt_response: None,
            response_format: None,
        };
        let mut output = AgentOutput {
            output: json!({"response": "ok"}),
            metrics: AgentExecutionMetrics::default(),
            success: true,
            error_message: String::new(),
            interrupt_request: None,
        };

        Tag.after_agent(&ctx, &mut output).await;
        assert_eq!(output.output["audited"], json!(true));
        assert_eq!(output.output["response"], json!("ok"));
    }
}
