//! Rust-defined workflows with explicit stage actions and routing.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::llm::LlmAction;
use crate::tools::{DenialBehavior, ToolDefinition};
use crate::types::{Error, Result, RunView, StageRecord};

/// Total run bounds. Every dimension is checked before consuming more budget.
#[derive(Debug, Clone, Copy)]
pub struct RunLimits {
    pub max_stage_executions: u32,
    pub max_llm_calls: u32,
    pub max_tool_calls: u32,
    pub deadline: Option<Duration>,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_stage_executions: 100,
            max_llm_calls: 100,
            max_tool_calls: 100,
            deadline: None,
        }
    }
}

impl RunLimits {
    fn validate(self) -> Result<Self> {
        if self.max_stage_executions == 0 {
            return Err(Error::configuration(
                "max_stage_executions must be positive",
            ));
        }
        Ok(self)
    }
}

/// Which action failures a workflow-level retry policy handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOn {
    Retryable,
    AnyFailure,
}

/// Explicit retry policy for one stage.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub backoff_multiplier: f64,
    pub max_backoff: Duration,
    pub retry_on: RetryOn,
}

impl RetryPolicy {
    pub fn exponential(max_attempts: u32, initial_backoff: Duration) -> Self {
        Self {
            max_attempts,
            initial_backoff,
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(30),
            retry_on: RetryOn::Retryable,
        }
    }

    pub fn retry_on_any_failure(mut self) -> Self {
        self.retry_on = RetryOn::AnyFailure;
        self
    }

    pub fn with_max_backoff(mut self, max_backoff: Duration) -> Self {
        self.max_backoff = max_backoff;
        self
    }

    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    pub fn backoff_for_retry(&self, completed_attempts: u32) -> Duration {
        if self.initial_backoff.is_zero() || self.max_backoff.is_zero() {
            return Duration::ZERO;
        }
        if self.initial_backoff >= self.max_backoff {
            return self.max_backoff;
        }

        let exponent = f64::from(completed_attempts.saturating_sub(1));
        let seconds = self.initial_backoff.as_secs_f64() * self.backoff_multiplier.powf(exponent);
        if !seconds.is_finite() || seconds >= self.max_backoff.as_secs_f64() {
            self.max_backoff
        } else {
            Duration::from_secs_f64(seconds)
        }
    }

    fn validate(self) -> Result<Self> {
        if self.max_attempts == 0 {
            return Err(Error::configuration("retry max_attempts must be positive"));
        }
        if !self.backoff_multiplier.is_finite() || self.backoff_multiplier < 1.0 {
            return Err(Error::configuration(
                "retry backoff multiplier must be finite and at least 1",
            ));
        }
        Ok(self)
    }
}

/// Consumer-defined deterministic stage action.
#[async_trait]
pub trait DeterministicAction: Send + Sync {
    async fn execute(&self, run: &RunView<'_>) -> Result<Value>;
}

/// Adapter for synchronous deterministic closures.
pub struct DeterministicFn<F>(F);

impl<F> DeterministicFn<F> {
    pub fn new(function: F) -> Self {
        Self(function)
    }
}

impl<F> fmt::Debug for DeterministicFn<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeterministicFn(..)")
    }
}

#[async_trait]
impl<F> DeterministicAction for DeterministicFn<F>
where
    F: Fn(&RunView<'_>) -> Result<Value> + Send + Sync,
{
    async fn execute(&self, run: &RunView<'_>) -> Result<Value> {
        (self.0)(run)
    }
}

/// Result of routing after a successful stage.
#[derive(Debug, Clone)]
pub enum Route {
    Next(Arc<str>),
    Complete,
}

impl Route {
    pub fn next(stage: impl Into<Arc<str>>) -> Self {
        Self::Next(stage.into())
    }
}

pub trait Router: Send + Sync {
    fn route(&self, run: &RunView<'_>) -> Result<Route>;
}

impl<F> Router for F
where
    F: Fn(&RunView<'_>) -> Result<Route> + Send + Sync,
{
    fn route(&self, run: &RunView<'_>) -> Result<Route> {
        self(run)
    }
}

/// Optional consumer state reducer invoked after every stage attempt.
pub trait StateReducer: Send + Sync {
    fn reduce(&self, state: &mut Value, record: &StageRecord) -> Result<()>;
}

impl<F> StateReducer for F
where
    F: Fn(&mut Value, &StageRecord) -> Result<()> + Send + Sync,
{
    fn reduce(&self, state: &mut Value, record: &StageRecord) -> Result<()> {
        self(state, record)
    }
}

/// Direct tool-stage configuration.
pub type ToolArguments = Arc<dyn Fn(&RunView<'_>) -> Result<Value> + Send + Sync>;

#[derive(Clone)]
pub struct ToolAction {
    pub tool: Arc<ToolDefinition>,
    pub on_denied: DenialBehavior,
    arguments: ToolArguments,
}

impl ToolAction {
    pub fn new(tool: Arc<ToolDefinition>) -> Self {
        Self {
            tool,
            on_denied: DenialBehavior::FailStage,
            arguments: Arc::new(|run| Ok(run.input.clone())),
        }
    }

    pub fn on_denied(mut self, behavior: DenialBehavior) -> Self {
        self.on_denied = behavior;
        self
    }

    /// Build invocation arguments from the current read-only run view.
    pub fn arguments(
        mut self,
        build: impl Fn(&RunView<'_>) -> Result<Value> + Send + Sync + 'static,
    ) -> Self {
        self.arguments = Arc::new(build);
        self
    }

    pub(crate) fn build_arguments(&self, run: &RunView<'_>) -> Result<Value> {
        (self.arguments)(run)
    }
}

impl fmt::Debug for ToolAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolAction")
            .field("tool", &self.tool)
            .field("on_denied", &self.on_denied)
            .field("arguments", &"ToolArguments(..)")
            .finish()
    }
}

#[derive(Clone)]
pub enum StageAction {
    Deterministic(Arc<dyn DeterministicAction>),
    Llm(LlmAction),
    Tool(ToolAction),
    RouteOnly,
}

impl fmt::Debug for StageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Deterministic(_) => f.write_str("StageAction::Deterministic(..)"),
            Self::Llm(action) => f.debug_tuple("StageAction::Llm").field(action).finish(),
            Self::Tool(action) => f.debug_tuple("StageAction::Tool").field(action).finish(),
            Self::RouteOnly => f.write_str("StageAction::RouteOnly"),
        }
    }
}

#[derive(Clone)]
pub(crate) enum StageRouting {
    Complete,
    Next(Arc<str>),
    Dynamic(Arc<dyn Router>),
}

impl fmt::Debug for StageRouting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => f.write_str("Complete"),
            Self::Next(stage) => f.debug_tuple("Next").field(stage).finish(),
            Self::Dynamic(_) => f.write_str("Dynamic(..)"),
        }
    }
}

/// One explicit workflow stage.
#[derive(Debug, Clone)]
pub struct Stage {
    pub(crate) name: Arc<str>,
    pub(crate) action: StageAction,
    pub(crate) routing: StageRouting,
    pub(crate) on_error: Option<Arc<str>>,
    pub(crate) retry: Option<RetryPolicy>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_visits: Option<u32>,
}

impl Stage {
    fn new(name: impl Into<Arc<str>>, action: StageAction) -> Self {
        Self {
            name: name.into(),
            action,
            routing: StageRouting::Complete,
            on_error: None,
            retry: None,
            timeout: None,
            max_visits: None,
        }
    }

    pub fn deterministic(name: impl Into<Arc<str>>, action: Arc<dyn DeterministicAction>) -> Self {
        Self::new(name, StageAction::Deterministic(action))
    }

    pub fn deterministic_fn<F>(name: impl Into<Arc<str>>, function: F) -> Self
    where
        F: Fn(&RunView<'_>) -> Result<Value> + Send + Sync + 'static,
    {
        Self::deterministic(name, Arc::new(DeterministicFn::new(function)))
    }

    pub fn llm(name: impl Into<Arc<str>>, action: LlmAction) -> Self {
        Self::new(name, StageAction::Llm(action))
    }

    pub fn tool(name: impl Into<Arc<str>>, action: ToolAction) -> Self {
        Self::new(name, StageAction::Tool(action))
    }

    pub fn route_only(name: impl Into<Arc<str>>) -> Self {
        Self::new(name, StageAction::RouteOnly)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn next(mut self, stage: impl Into<Arc<str>>) -> Self {
        self.routing = StageRouting::Next(stage.into());
        self
    }

    pub fn route(mut self, router: impl Router + 'static) -> Self {
        self.routing = StageRouting::Dynamic(Arc::new(router));
        self
    }

    pub fn on_error(mut self, stage: impl Into<Arc<str>>) -> Self {
        self.on_error = Some(stage.into());
        self
    }

    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = Some(policy);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn max_visits(mut self, max_visits: u32) -> Self {
        self.max_visits = Some(max_visits);
        self
    }
}

/// Validated immutable workflow.
#[derive(Clone)]
pub struct Workflow {
    pub(crate) name: Arc<str>,
    pub(crate) stages: Arc<[Stage]>,
    pub(crate) limits: RunLimits,
    pub(crate) initial_state: Value,
    pub(crate) reducer: Option<Arc<dyn StateReducer>>,
}

impl Workflow {
    pub fn builder(name: impl Into<Arc<str>>) -> WorkflowBuilder {
        WorkflowBuilder::new(name)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    pub(crate) fn stage(&self, name: &str) -> Option<&Stage> {
        self.stages.iter().find(|stage| stage.name.as_ref() == name)
    }
}

impl fmt::Debug for Workflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Workflow")
            .field("name", &self.name)
            .field("stages", &self.stages)
            .field("limits", &self.limits)
            .field("initial_state", &self.initial_state)
            .field("has_reducer", &self.reducer.is_some())
            .finish()
    }
}

/// Workflow construction and validation.
pub struct WorkflowBuilder {
    name: Arc<str>,
    stages: Vec<Stage>,
    limits: RunLimits,
    initial_state: Value,
    reducer: Option<Arc<dyn StateReducer>>,
}

impl WorkflowBuilder {
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self {
            name: name.into(),
            stages: Vec::new(),
            limits: RunLimits::default(),
            initial_state: Value::Null,
            reducer: None,
        }
    }

    pub fn stage(mut self, stage: Stage) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn limits(mut self, limits: RunLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn state(mut self, initial_state: Value, reducer: impl StateReducer + 'static) -> Self {
        self.initial_state = initial_state;
        self.reducer = Some(Arc::new(reducer));
        self
    }

    pub fn build(self) -> Result<Workflow> {
        if self.name.is_empty() {
            return Err(Error::configuration("workflow name cannot be empty"));
        }
        if self.stages.is_empty() {
            return Err(Error::configuration(
                "workflow must contain at least one stage",
            ));
        }
        let mut names = HashSet::new();
        for stage in &self.stages {
            if stage.name.is_empty() {
                return Err(Error::configuration("stage name cannot be empty"));
            }
            if !names.insert(stage.name.clone()) {
                return Err(Error::configuration(format!(
                    "duplicate stage '{}'",
                    stage.name
                )));
            }
            if stage.max_visits == Some(0) {
                return Err(Error::configuration(format!(
                    "stage '{}' max_visits must be positive",
                    stage.name
                )));
            }
            if stage.timeout == Some(Duration::ZERO) {
                return Err(Error::configuration(format!(
                    "stage '{}' timeout must be positive",
                    stage.name
                )));
            }
            if let Some(retry) = stage.retry {
                retry.validate()?;
            }
            if let StageAction::Llm(action) = &stage.action {
                action.validate()?;
            }
        }

        for stage in &self.stages {
            let static_target = match &stage.routing {
                StageRouting::Next(target) => Some(target),
                StageRouting::Complete | StageRouting::Dynamic(_) => None,
            };
            for target in static_target.into_iter().chain(stage.on_error.iter()) {
                if !names.contains(target) {
                    return Err(Error::configuration(format!(
                        "stage '{}' targets missing stage '{}'",
                        stage.name, target
                    )));
                }
            }
        }

        Ok(Workflow {
            name: self.name,
            stages: self.stages.into(),
            limits: self.limits.validate()?,
            initial_state: self.initial_state,
            reducer: self.reducer,
        })
    }
}

impl fmt::Debug for WorkflowBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkflowBuilder")
            .field("name", &self.name)
            .field("stages", &self.stages)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
