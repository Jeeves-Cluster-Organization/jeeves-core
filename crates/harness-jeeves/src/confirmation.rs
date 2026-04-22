//! Confirmation gate as a `BeforeToolCall` hook.

use agent_core::{BeforeToolCall, HookDecision, ToolCall, ToolOutput};
use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

/// Consumer-provided decider for destructive tool calls.
///
/// Returns `true` to allow the call, `false` to block with a reason.
#[async_trait]
pub trait Decider: Send + Sync + std::fmt::Debug {
    async fn allow(&self, call: &ToolCall) -> Decision;
}

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny {
        reason: String,
    },
    /// Skip the call; deliver this output to the model as the "result".
    Substitute(ToolOutput),
}

/// Deny-by-default decider that only allows specific tool names.
#[derive(Debug)]
pub struct AllowList(pub HashSet<String>);

#[async_trait]
impl Decider for AllowList {
    async fn allow(&self, call: &ToolCall) -> Decision {
        if self.0.contains(&call.name) {
            Decision::Allow
        } else {
            Decision::Deny {
                reason: format!("'{}' is not on the allow-list", call.name),
            }
        }
    }
}

/// Wraps a `Decider` into a `BeforeToolCall` hook, gated by a set of tool names.
///
/// Only calls matching `guarded` consult the decider; other calls pass through.
#[derive(Debug)]
pub struct ConfirmationGate {
    guarded: HashSet<String>,
    decider: Arc<dyn Decider>,
}

impl ConfirmationGate {
    pub fn new(
        guarded: impl IntoIterator<Item = impl Into<String>>,
        decider: Arc<dyn Decider>,
    ) -> Self {
        Self {
            guarded: guarded.into_iter().map(Into::into).collect(),
            decider,
        }
    }
}

#[async_trait]
impl BeforeToolCall for ConfirmationGate {
    async fn call(&self, call: &ToolCall) -> HookDecision {
        if !self.guarded.contains(&call.name) {
            return HookDecision::Continue;
        }
        match self.decider.allow(call).await {
            Decision::Allow => HookDecision::Continue,
            Decision::Deny { reason } => HookDecision::Block { reason },
            Decision::Substitute(out) => HookDecision::Replace(out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Yes;
    #[async_trait]
    impl Decider for Yes {
        async fn allow(&self, _: &ToolCall) -> Decision {
            Decision::Allow
        }
    }

    #[tokio::test]
    async fn allows_non_guarded_without_consulting_decider() {
        let gate = ConfirmationGate::new(["bash"], Arc::new(Yes));
        let call = ToolCall {
            id: "1".into(),
            name: "read".into(),
            arguments: serde_json::json!({}),
        };
        matches!(gate.call(&call).await, HookDecision::Continue);
    }

    #[tokio::test]
    async fn consults_decider_for_guarded() {
        let gate = ConfirmationGate::new(["bash"], Arc::new(Yes));
        let call = ToolCall {
            id: "1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({}),
        };
        matches!(gate.call(&call).await, HookDecision::Continue);
    }
}
