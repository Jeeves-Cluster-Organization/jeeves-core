//! PipelineBuilder — fluent API for constructing PipelineConfig.
//!
//! Reduces pipeline authoring from 30+ lines of raw JSON to ~5 lines:
//! ```ignore
//! PipelineBuilder::new("chat")
//!     .stage("understand", "understand").has_llm(false).default_next("respond").done()
//!     .stage("respond", "respond").done()
//!     .bounds(10, 5, 5)
//!     .build()?
//! ```

use crate::types::Result;

use super::orchestrator_types::{
    EdgeLimit, JoinStrategy, MergeStrategy, NodeKind, PipelineConfig, PipelineStage, RouterTarget,
    StateField,
};
use super::routing::{FieldRef, RoutingExpr, RoutingRule};

/// Fluent builder for PipelineConfig.
#[derive(Debug)]
pub struct PipelineBuilder {
    name: String,
    stages: Vec<PipelineStage>,
    max_iterations: i32,
    max_llm_calls: i32,
    max_agent_hops: i32,
    edge_limits: Vec<EdgeLimit>,
    step_limit: Option<i32>,
    state_schema: Vec<StateField>,
}

impl PipelineBuilder {
    /// Create a new pipeline builder with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stages: Vec::new(),
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: Vec::new(),
            step_limit: None,
            state_schema: Vec::new(),
        }
    }

    /// Add a stage and return a StageHandle for fluent configuration.
    pub fn stage(mut self, name: &str, agent: &str) -> StageHandle {
        let stage = PipelineStage {
            name: name.to_string(),
            agent: agent.to_string(),
            ..PipelineStage::default()
        };
        self.stages.push(stage);
        let idx = self.stages.len() - 1;
        StageHandle {
            builder: self,
            stage_idx: idx,
        }
    }

    /// Set pipeline-level bounds (max_iterations, max_llm_calls, max_agent_hops).
    pub fn bounds(mut self, iterations: i32, llm_calls: i32, hops: i32) -> Self {
        self.max_iterations = iterations;
        self.max_llm_calls = llm_calls;
        self.max_agent_hops = hops;
        self
    }

    /// Add an edge limit.
    pub fn edge_limit(mut self, from: &str, to: &str, max: i32) -> Self {
        self.edge_limits.push(EdgeLimit {
            from_stage: from.to_string(),
            to_stage: to.to_string(),
            max_count: max,
        });
        self
    }

    /// Add a state field with a merge strategy.
    pub fn state_field(mut self, key: &str, merge: MergeStrategy) -> Self {
        self.state_schema.push(StateField {
            key: key.to_string(),
            merge,
        });
        self
    }

    /// Set optional step limit.
    pub fn step_limit(mut self, limit: i32) -> Self {
        self.step_limit = Some(limit);
        self
    }

    /// Build the PipelineConfig, running validation.
    pub fn build(self) -> Result<PipelineConfig> {
        let config = PipelineConfig {
            name: self.name,
            stages: self.stages,
            max_iterations: self.max_iterations,
            max_llm_calls: self.max_llm_calls,
            max_agent_hops: self.max_agent_hops,
            edge_limits: self.edge_limits,
            step_limit: self.step_limit,
            state_schema: self.state_schema,
            subscriptions: vec![],
            publishes: vec![],
        };
        config.validate()?;
        Ok(config)
    }
}

/// Handle for configuring a stage in the builder chain.
/// Call `.done()` to return to the pipeline level.
#[derive(Debug)]
pub struct StageHandle {
    builder: PipelineBuilder,
    stage_idx: usize,
}

impl StageHandle {
    fn stage_mut(&mut self) -> &mut PipelineStage {
        &mut self.builder.stages[self.stage_idx]
    }

    /// Set default_next for this stage.
    pub fn default_next(mut self, target: &str) -> Self {
        self.stage_mut().default_next = Some(target.to_string());
        self
    }

    /// Set error_next for this stage.
    pub fn error_next(mut self, target: &str) -> Self {
        self.stage_mut().error_next = Some(target.to_string());
        self
    }

    /// Add a routing rule to this stage.
    pub fn routing(mut self, expr: RoutingExpr, target: &str) -> Self {
        self.stage_mut().routing.push(RoutingRule {
            expr,
            target: target.to_string(),
        });
        self
    }

    /// Set this stage as a Gate (routing-only, no agent execution).
    pub fn gate(mut self) -> Self {
        self.stage_mut().node_kind = NodeKind::Gate;
        self
    }

    /// Set this stage as a Fork (parallel fan-out).
    pub fn fork(mut self) -> Self {
        self.stage_mut().node_kind = NodeKind::Fork;
        self
    }

    /// Set this stage as a Router (agent picks target from declared set).
    pub fn router(mut self) -> Self {
        self.stage_mut().node_kind = NodeKind::Router;
        self
    }

    /// Add a router target (unconditional).
    pub fn router_target(mut self, target: &str, description: &str) -> Self {
        self.stage_mut().router_targets.push(RouterTarget {
            target: target.to_string(),
            description: description.to_string(),
            when: None,
        });
        self
    }

    /// Add a router target with a guard condition.
    pub fn router_target_when(mut self, target: &str, description: &str, when: RoutingExpr) -> Self {
        self.stage_mut().router_targets.push(RouterTarget {
            target: target.to_string(),
            description: description.to_string(),
            when: Some(when),
        });
        self
    }

    /// Set whether this agent makes LLM calls.
    pub fn has_llm(mut self, v: bool) -> Self {
        self.stage_mut().agent_config.has_llm = v;
        self
    }

    /// Set max_visits for this stage.
    pub fn max_visits(mut self, n: i32) -> Self {
        self.stage_mut().max_visits = Some(n);
        self
    }

    /// Set allowed_tools for this stage.
    pub fn allowed_tools(mut self, tools: Vec<&str>) -> Self {
        self.stage_mut().allowed_tools = Some(tools.into_iter().map(|s| s.to_string()).collect());
        self
    }

    /// Set LLM temperature for this stage.
    pub fn temperature(mut self, t: f64) -> Self {
        self.stage_mut().agent_config.temperature = Some(t);
        self
    }

    /// Set LLM max_tokens for this stage.
    pub fn max_tokens(mut self, n: i32) -> Self {
        self.stage_mut().agent_config.max_tokens = Some(n);
        self
    }

    /// Set model_role for this stage.
    pub fn model_role(mut self, role: &str) -> Self {
        self.stage_mut().agent_config.model_role = Some(role.to_string());
        self
    }

    /// Set prompt_key for this stage.
    pub fn prompt_key(mut self, key: &str) -> Self {
        self.stage_mut().agent_config.prompt_key = Some(key.to_string());
        self
    }

    /// Set output_key for this stage.
    pub fn output_key(mut self, key: &str) -> Self {
        self.stage_mut().output_key = Some(key.to_string());
        self
    }

    /// Set join_strategy for this stage.
    pub fn join_strategy(mut self, strategy: JoinStrategy) -> Self {
        self.stage_mut().join_strategy = strategy;
        self
    }

    /// Set output_schema for this stage.
    pub fn output_schema(mut self, schema: serde_json::Value) -> Self {
        self.stage_mut().output_schema = Some(schema);
        self
    }

    /// Set child_pipeline for this stage (composition).
    pub fn child_pipeline(mut self, name: &str) -> Self {
        self.stage_mut().agent_config.child_pipeline = Some(name.to_string());
        self.stage_mut().agent_config.has_llm = false; // mutually exclusive
        self
    }

    /// Return to the pipeline builder.
    pub fn done(self) -> PipelineBuilder {
        self.builder
    }
}

// =============================================================================
// Default impl for PipelineStage
// =============================================================================

// PipelineStage Default is derived — all fields have matching defaults.
// AgentConfig::default() sets has_llm=false, matching serde `default_has_llm()`.

// =============================================================================
// RoutingExpr convenience constructors
// =============================================================================

/// Always-true routing expression.
pub fn always() -> RoutingExpr {
    RoutingExpr::Always
}

/// Equality routing expression.
pub fn eq(field: FieldRef, value: impl Into<serde_json::Value>) -> RoutingExpr {
    RoutingExpr::Eq { field, value: value.into() }
}

/// Inequality routing expression.
pub fn neq(field: FieldRef, value: impl Into<serde_json::Value>) -> RoutingExpr {
    RoutingExpr::Neq { field, value: value.into() }
}

/// Greater-than routing expression.
pub fn gt(field: FieldRef, value: impl Into<serde_json::Value>) -> RoutingExpr {
    RoutingExpr::Gt { field, value: value.into() }
}

/// Less-than routing expression.
pub fn lt(field: FieldRef, value: impl Into<serde_json::Value>) -> RoutingExpr {
    RoutingExpr::Lt { field, value: value.into() }
}

/// Field-exists routing expression.
pub fn exists(field: FieldRef) -> RoutingExpr {
    RoutingExpr::Exists { field }
}

/// Field-not-exists routing expression.
pub fn not_exists(field: FieldRef) -> RoutingExpr {
    RoutingExpr::NotExists { field }
}

/// Contains routing expression.
pub fn contains(field: FieldRef, value: impl Into<serde_json::Value>) -> RoutingExpr {
    RoutingExpr::Contains { field, value: value.into() }
}

/// FieldRef for current agent output.
pub fn current(key: &str) -> FieldRef {
    FieldRef::Current { key: key.to_string() }
}

/// FieldRef for merged state.
pub fn state(key: &str) -> FieldRef {
    FieldRef::State { key: key.to_string() }
}

/// FieldRef for cross-agent output.
pub fn agent_ref(agent: &str, key: &str) -> FieldRef {
    FieldRef::Agent { agent: agent.to_string(), key: key.to_string() }
}

/// FieldRef for metadata with dot-notation.
pub fn meta(path: &str) -> FieldRef {
    FieldRef::Meta { path: path.to_string() }
}

/// FieldRef for interrupt response.
pub fn interrupt(key: &str) -> FieldRef {
    FieldRef::Interrupt { key: key.to_string() }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_simple_two_stage_pipeline() {
        let config = PipelineBuilder::new("chat")
            .stage("understand", "understand")
                .has_llm(false)
                .default_next("respond")
                .done()
            .stage("respond", "respond")
                .done()
            .bounds(10, 5, 5)
            .build()
            .unwrap();

        assert_eq!(config.name, "chat");
        assert_eq!(config.stages.len(), 2);
        assert_eq!(config.stages[0].name, "understand");
        assert!(!config.stages[0].agent_config.has_llm);
        assert_eq!(config.stages[0].default_next.as_deref(), Some("respond"));
        assert_eq!(config.stages[1].name, "respond");
        assert!(!config.stages[1].agent_config.has_llm); // default is false (opt-in)
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.max_llm_calls, 5);
        assert_eq!(config.max_agent_hops, 5);
    }

    #[test]
    fn test_builder_with_routing() {
        let config = PipelineBuilder::new("routed")
            .stage("router", "router")
                .routing(eq(current("intent"), "search"), "search")
                .routing(eq(current("intent"), "chat"), "chat")
                .default_next("fallback")
                .done()
            .stage("search", "search_agent").done()
            .stage("chat", "chat_agent").done()
            .stage("fallback", "fallback_agent").done()
            .bounds(10, 10, 10)
            .build()
            .unwrap();

        assert_eq!(config.stages[0].routing.len(), 2);
        assert_eq!(config.stages[0].routing[0].target, "search");
        assert_eq!(config.stages[0].routing[1].target, "chat");
    }

    #[test]
    fn test_builder_with_gate_and_fork() {
        let config = PipelineBuilder::new("complex")
            .stage("gate", "")
                .gate()
                .routing(always(), "worker")
                .done()
            .stage("fork", "")
                .fork()
                .routing(always(), "a")
                .routing(always(), "b")
                .default_next("join")
                .done()
            .stage("a", "agent_a").done()
            .stage("b", "agent_b").done()
            .stage("worker", "worker_agent").done()
            .stage("join", "join_agent").done()
            .bounds(20, 10, 10)
            .build()
            .unwrap();

        assert_eq!(config.stages[0].node_kind, NodeKind::Gate);
        assert_eq!(config.stages[1].node_kind, NodeKind::Fork);
    }

    #[test]
    fn test_builder_validation_catches_empty_name() {
        let result = PipelineBuilder::new("")
            .stage("a", "a").done()
            .bounds(10, 10, 10)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_validation_catches_duplicate_stages() {
        let result = PipelineBuilder::new("dup")
            .stage("a", "a").done()
            .stage("a", "a").done()
            .bounds(10, 10, 10)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_with_edge_limit_and_state_schema() {
        let config = PipelineBuilder::new("stateful")
            .stage("a", "agent_a").default_next("b").done()
            .stage("b", "agent_b").done()
            .bounds(10, 10, 10)
            .edge_limit("a", "b", 3)
            .state_field("results", MergeStrategy::Append)
            .build()
            .unwrap();

        assert_eq!(config.edge_limits.len(), 1);
        assert_eq!(config.edge_limits[0].max_count, 3);
        assert_eq!(config.state_schema.len(), 1);
        assert_eq!(config.state_schema[0].key, "results");
    }

    #[test]
    fn test_builder_stage_all_options() {
        let config = PipelineBuilder::new("full")
            .stage("s1", "a1")
                .has_llm(true)
                .default_next("s1")
                .error_next("s1")
                .max_visits(3)
                .temperature(0.7)
                .max_tokens(1000)
                .model_role("fast")
                .prompt_key("my_prompt")
                .output_key("my_output")
                .allowed_tools(vec!["search", "calc"])
                .join_strategy(JoinStrategy::WaitFirst)
                .output_schema(serde_json::json!({"type": "object"}))
                .done()
            .bounds(10, 10, 10)
            .build()
            .unwrap();

        let s = &config.stages[0];
        assert!(s.agent_config.has_llm);
        assert_eq!(s.max_visits, Some(3));
        assert_eq!(s.agent_config.temperature, Some(0.7));
        assert_eq!(s.agent_config.max_tokens, Some(1000));
        assert_eq!(s.agent_config.model_role.as_deref(), Some("fast"));
        assert_eq!(s.agent_config.prompt_key.as_deref(), Some("my_prompt"));
        assert_eq!(s.output_key.as_deref(), Some("my_output"));
        assert_eq!(s.allowed_tools.as_ref().unwrap().len(), 2);
        assert_eq!(s.join_strategy, JoinStrategy::WaitFirst);
        assert!(s.output_schema.is_some());
    }

    #[test]
    fn test_pipeline_stage_default_matches_serde() {
        let stage = PipelineStage::default();
        assert!(!stage.agent_config.has_llm);  // matches default_has_llm() = false
        assert_eq!(stage.node_kind, NodeKind::Agent);
        assert_eq!(stage.join_strategy, JoinStrategy::WaitAll);
        assert!(stage.routing.is_empty());
        assert!(stage.default_next.is_none());
        assert!(stage.error_next.is_none());
        assert!(stage.max_visits.is_none());
        assert!(stage.agent_config.temperature.is_none());
        assert!(stage.agent_config.max_tokens.is_none());
        assert!(stage.agent_config.model_role.is_none());
    }

    #[test]
    fn test_routing_convenience_constructors() {
        assert!(matches!(always(), RoutingExpr::Always));
        assert!(matches!(eq(current("k"), "v"), RoutingExpr::Eq { .. }));
        assert!(matches!(neq(current("k"), "v"), RoutingExpr::Neq { .. }));
        assert!(matches!(gt(current("k"), 5), RoutingExpr::Gt { .. }));
        assert!(matches!(lt(current("k"), 5), RoutingExpr::Lt { .. }));
        assert!(matches!(exists(current("k")), RoutingExpr::Exists { .. }));
        assert!(matches!(not_exists(current("k")), RoutingExpr::NotExists { .. }));
        assert!(matches!(contains(current("k"), "v"), RoutingExpr::Contains { .. }));
    }

    #[test]
    fn test_field_ref_constructors() {
        assert!(matches!(current("k"), FieldRef::Current { .. }));
        assert!(matches!(state("k"), FieldRef::State { .. }));
        assert!(matches!(agent_ref("a", "k"), FieldRef::Agent { .. }));
        assert!(matches!(meta("a.b"), FieldRef::Meta { .. }));
        assert!(matches!(interrupt("k"), FieldRef::Interrupt { .. }));
    }
}
