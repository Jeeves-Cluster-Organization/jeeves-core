//! Routing is code, not data: consumers register named functions on the
//! kernel; stages reference them by name. Static `default_next` / `error_next`
//! remain declarative on `Stage`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Read-only snapshot passed to a [`RoutingFn`].
#[derive(Debug)]
pub struct RoutingContext<'a> {
    pub current_stage: &'a str,
    pub agent_name: &'a str,
    pub agent_failed: bool,
    pub outputs: &'a HashMap<String, HashMap<String, serde_json::Value>>,
    pub metadata: &'a HashMap<String, serde_json::Value>,
    pub interrupt_response: Option<&'a serde_json::Value>,
    pub state: &'a HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum RoutingResult {
    Next(String),
    Terminate,
}

pub trait RoutingFn: Send + Sync {
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingResult;
}

impl<F> RoutingFn for F
where
    F: Fn(&RoutingContext<'_>) -> RoutingResult + Send + Sync,
{
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingResult {
        self(ctx)
    }
}

#[derive(Default)]
pub struct RoutingRegistry {
    fns: HashMap<String, Arc<dyn RoutingFn>>,
}

impl RoutingRegistry {
    pub fn new() -> Self {
        Self { fns: HashMap::new() }
    }

    pub fn register(&mut self, name: impl Into<String>, f: Arc<dyn RoutingFn>) {
        self.fns.insert(name.into(), f);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn RoutingFn>> {
        self.fns.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }
}

impl std::fmt::Debug for RoutingRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutingRegistry")
            .field("registered", &self.fns.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Routing decision with rationale, emitted into the audit trail and
/// [`RunEvent::RoutingDecision`].
///
/// [`RunEvent::RoutingDecision`]: crate::agent::llm::RunEvent::RoutingDecision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub from_stage: String,
    /// `None` means the pipeline terminated.
    pub target: Option<String>,
    pub reason: RoutingReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RoutingReason {
    ErrorRoute,
    RoutingFn { name: String },
    DefaultRoute,
    /// No routing fn, no `default_next` — pipeline terminates `Completed`.
    NoMatch,
}

pub fn evaluate_routing_with_reason(
    stage: &crate::workflow::Stage,
    registry: &RoutingRegistry,
    ctx: &RoutingContext<'_>,
    from_stage: &str,
) -> RoutingDecision {
    if ctx.agent_failed {
        if let Some(ref error_next) = stage.error_next {
            return RoutingDecision {
                from_stage: from_stage.to_string(),
                target: Some(error_next.clone()),
                reason: RoutingReason::ErrorRoute,
            };
        }
    }

    if let Some(ref fn_name) = stage.routing_fn {
        if let Some(routing_fn) = registry.get(fn_name) {
            let result = routing_fn.route(ctx);
            tracing::debug!(
                stage = %from_stage,
                routing_fn = %fn_name,
                ?result,
                "routing_fn_evaluated"
            );
            let target = match result {
                RoutingResult::Next(t) => Some(t),
                RoutingResult::Terminate => None,
            };
            return RoutingDecision {
                from_stage: from_stage.to_string(),
                target,
                reason: RoutingReason::RoutingFn { name: fn_name.clone() },
            };
        } else {
            tracing::warn!(
                stage = %from_stage,
                routing_fn = %fn_name,
                "routing_fn_not_found_in_registry"
            );
        }
    }

    if let Some(ref default_next) = stage.default_next {
        tracing::debug!(stage = %from_stage, target = %default_next, "default_route_taken");
        return RoutingDecision {
            from_stage: from_stage.to_string(),
            target: Some(default_next.clone()),
            reason: RoutingReason::DefaultRoute,
        };
    }

    tracing::debug!(stage = %from_stage, "no_routing_match");
    RoutingDecision {
        from_stage: from_stage.to_string(),
        target: None,
        reason: RoutingReason::NoMatch,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Stage;

    fn test_registry() -> RoutingRegistry {
        let mut reg = RoutingRegistry::new();
        reg.register("always_s2", Arc::new(|_ctx: &RoutingContext| -> RoutingResult {
            RoutingResult::Next("s2".to_string())
        }));
        reg.register("terminate", Arc::new(|_ctx: &RoutingContext| -> RoutingResult {
            RoutingResult::Terminate
        }));
        reg
    }

    fn empty_ctx() -> (HashMap<String, HashMap<String, serde_json::Value>>, HashMap<String, serde_json::Value>) {
        (HashMap::new(), HashMap::new())
    }

    fn make_ctx<'a>(
        outputs: &'a HashMap<String, HashMap<String, serde_json::Value>>,
        metadata: &'a HashMap<String, serde_json::Value>,
        state: &'a HashMap<String, serde_json::Value>,
    ) -> RoutingContext<'a> {
        RoutingContext {
            current_stage: "s1",
            agent_name: "a1",
            agent_failed: false,
            outputs,
            metadata,
            interrupt_response: None,
            state,
        }
    }

    #[test]
    fn test_routing_fn_called() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("always_s2".to_string()),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, Some("s2".to_string()));
        assert!(matches!(decision.reason, RoutingReason::RoutingFn { ref name } if name == "always_s2"));
    }

    #[test]
    fn test_error_next_takes_priority() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let mut ctx = make_ctx(&outputs, &metadata, &state);
        ctx.agent_failed = true;

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("always_s2".to_string()),
            error_next: Some("s_err".to_string()),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, Some("s_err".to_string()));
        assert!(matches!(decision.reason, RoutingReason::ErrorRoute));
    }

    #[test]
    fn test_default_next_fallback() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            default_next: Some("s2".to_string()),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, Some("s2".to_string()));
        assert!(matches!(decision.reason, RoutingReason::DefaultRoute));
    }

    #[test]
    fn test_no_routing_no_default_terminates() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, None);
        assert!(matches!(decision.reason, RoutingReason::NoMatch));
    }

    #[test]
    fn test_terminate_routing_fn() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("terminate".to_string()),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, None);
        assert!(matches!(decision.reason, RoutingReason::RoutingFn { .. }));
    }

    #[test]
    fn test_missing_routing_fn_falls_through() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = Stage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("nonexistent".to_string()),
            default_next: Some("s2".to_string()),
            ..Stage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, Some("s2".to_string()));
        assert!(matches!(decision.reason, RoutingReason::DefaultRoute));
    }

    #[test]
    fn test_registry_operations() {
        let mut reg = RoutingRegistry::new();
        assert!(!reg.contains("test"));

        reg.register("test", Arc::new(|_ctx: &RoutingContext| RoutingResult::Terminate));
        assert!(reg.contains("test"));
        assert!(reg.get("test").is_some());
        assert!(reg.get("other").is_none());
    }
}
