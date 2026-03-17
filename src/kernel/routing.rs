//! Routing function dispatch for pipeline orchestration.
//!
//! Routing is code, not data. Consumers register named routing functions
//! that the kernel calls to determine the next stage after agent execution.
//! Static wiring (default_next, error_next) remains declarative on PipelineStage.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// =============================================================================
// Routing Context & Result
// =============================================================================

/// Context passed to routing functions — read-only view of pipeline state.
#[derive(Debug)]
pub struct RoutingContext<'a> {
    /// Name of the stage that just completed (or is being evaluated for Gate).
    pub current_stage: &'a str,
    /// Name of the agent that ran (same as stage.agent).
    pub agent_name: &'a str,
    /// Whether the agent execution failed.
    pub agent_failed: bool,
    /// All agent outputs: agent_name → { key → value }.
    pub outputs: &'a HashMap<String, HashMap<String, serde_json::Value>>,
    /// Envelope metadata (session context, user info, etc.).
    pub metadata: &'a HashMap<String, serde_json::Value>,
    /// Response from a resolved interrupt, if any.
    pub interrupt_response: Option<&'a serde_json::Value>,
    /// Accumulated state across loop iterations.
    pub state: &'a HashMap<String, serde_json::Value>,
}

/// Result from a routing function.
#[derive(Debug, Clone)]
pub enum RoutingResult {
    /// Route to a single target stage.
    Next(String),
    /// Fan out to multiple stages in parallel (Fork semantics).
    Fan(Vec<String>),
    /// End the pipeline.
    Terminate,
}

// =============================================================================
// Routing Function Trait & Registry
// =============================================================================

/// Trait for routing functions. Consumers implement this or use the blanket
/// impl for closures: `Fn(&RoutingContext) -> RoutingResult`.
pub trait RoutingFn: Send + Sync {
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingResult;
}

/// Blanket impl: any closure with the right signature is a RoutingFn.
impl<F> RoutingFn for F
where
    F: Fn(&RoutingContext<'_>) -> RoutingResult + Send + Sync,
{
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingResult {
        self(ctx)
    }
}

/// Thread-safe registry of named routing functions.
#[derive(Default)]
pub struct RoutingRegistry {
    fns: HashMap<String, Arc<dyn RoutingFn>>,
}

impl RoutingRegistry {
    pub fn new() -> Self {
        Self { fns: HashMap::new() }
    }

    /// Register a routing function by name.
    pub fn register(&mut self, name: impl Into<String>, f: Arc<dyn RoutingFn>) {
        self.fns.insert(name.into(), f);
    }

    /// Look up a routing function by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn RoutingFn>> {
        self.fns.get(name)
    }

    /// Check if a routing function is registered.
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

// =============================================================================
// Routing Decision Types (audit trail)
// =============================================================================

/// Result of routing evaluation with decision rationale for audit trails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Stage that made this routing decision.
    pub from_stage: String,
    /// Target stage (None = pipeline terminated).
    pub target: Option<String>,
    /// Why this target was chosen.
    pub reason: RoutingReason,
}

/// Why a particular routing decision was made.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingReason {
    /// Agent failed and error_next was set.
    ErrorRoute,
    /// A registered routing function decided the target.
    RoutingFn { name: String },
    /// No routing function; used default_next.
    DefaultRoute,
    /// No routing function, no default_next — pipeline terminates.
    NoMatch,
}

// =============================================================================
// Evaluation
// =============================================================================

/// Evaluate routing for a stage using the registry.
///
/// Evaluation order:
/// 1. If agent_failed AND error_next set → route to error_next
/// 2. If routing_fn registered → call it, use result
/// 3. If no routing_fn → fall through to default_next
/// 4. If no default_next → None (caller terminates with COMPLETED)
pub fn evaluate_routing_with_reason(
    stage: &super::orchestrator_types::PipelineStage,
    registry: &RoutingRegistry,
    ctx: &RoutingContext<'_>,
    from_stage: &str,
) -> RoutingDecision {
    // 1. Error path
    if ctx.agent_failed {
        if let Some(ref error_next) = stage.error_next {
            return RoutingDecision {
                from_stage: from_stage.to_string(),
                target: Some(error_next.clone()),
                reason: RoutingReason::ErrorRoute,
            };
        }
    }

    // 2. Routing function
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
                RoutingResult::Fan(targets) => {
                    // Fan is handled by the caller (Fork dispatch).
                    // For non-Fork stages, take the first target.
                    targets.into_iter().next()
                }
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

    // 3. Default fallback
    if let Some(ref default_next) = stage.default_next {
        tracing::debug!(stage = %from_stage, target = %default_next, "default_route_taken");
        return RoutingDecision {
            from_stage: from_stage.to_string(),
            target: Some(default_next.clone()),
            reason: RoutingReason::DefaultRoute,
        };
    }

    // 4. No match — Temporal pattern: kernel terminates
    tracing::debug!(stage = %from_stage, "no_routing_match");
    RoutingDecision {
        from_stage: from_stage.to_string(),
        target: None,
        reason: RoutingReason::NoMatch,
    }
}

/// Evaluate routing for a Fork node — returns all fan-out targets.
///
/// If the routing function returns Fan(targets), returns those targets.
/// If it returns Next(target), returns a single-element vec.
/// If no routing function, falls back to default_next.
pub fn evaluate_fork_routing(
    stage: &super::orchestrator_types::PipelineStage,
    registry: &RoutingRegistry,
    ctx: &RoutingContext<'_>,
) -> Vec<String> {
    if let Some(ref fn_name) = stage.routing_fn {
        if let Some(routing_fn) = registry.get(fn_name) {
            let result = routing_fn.route(ctx);
            tracing::debug!(
                stage = %stage.name,
                routing_fn = %fn_name,
                ?result,
                "fork_routing_fn_evaluated"
            );
            return match result {
                RoutingResult::Fan(targets) => targets,
                RoutingResult::Next(t) => vec![t],
                RoutingResult::Terminate => vec![],
            };
        } else {
            tracing::warn!(
                stage = %stage.name,
                routing_fn = %fn_name,
                "fork_routing_fn_not_found"
            );
        }
    }

    // Fallback: default_next as single target
    stage.default_next.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::orchestrator_types::PipelineStage;

    fn test_registry() -> RoutingRegistry {
        let mut reg = RoutingRegistry::new();
        reg.register("always_s2", Arc::new(|_ctx: &RoutingContext| -> RoutingResult {
            RoutingResult::Next("s2".to_string())
        }));
        reg.register("terminate", Arc::new(|_ctx: &RoutingContext| -> RoutingResult {
            RoutingResult::Terminate
        }));
        reg.register("fan_ab", Arc::new(|_ctx: &RoutingContext| -> RoutingResult {
            RoutingResult::Fan(vec!["a".to_string(), "b".to_string()])
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

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("always_s2".to_string()),
            ..PipelineStage::default()
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

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("always_s2".to_string()),
            error_next: Some("s_err".to_string()),
            ..PipelineStage::default()
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

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            default_next: Some("s2".to_string()),
            ..PipelineStage::default()
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

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            ..PipelineStage::default()
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

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("terminate".to_string()),
            ..PipelineStage::default()
        };

        let decision = evaluate_routing_with_reason(&stage, &reg, &ctx, "s1");
        assert_eq!(decision.target, None);
        assert!(matches!(decision.reason, RoutingReason::RoutingFn { .. }));
    }

    #[test]
    fn test_fork_routing() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = PipelineStage {
            name: "fork1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("fan_ab".to_string()),
            ..PipelineStage::default()
        };

        let targets = evaluate_fork_routing(&stage, &reg, &ctx);
        assert_eq!(targets, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn test_missing_routing_fn_falls_through() {
        let reg = test_registry();
        let (outputs, metadata) = empty_ctx();
        let state = HashMap::new();
        let ctx = make_ctx(&outputs, &metadata, &state);

        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing_fn: Some("nonexistent".to_string()),
            default_next: Some("s2".to_string()),
            ..PipelineStage::default()
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
