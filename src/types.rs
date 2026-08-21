//! Shared execution types.

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;
use std::ops::{AddAssign, Sub};
use std::sync::Arc;
use std::time::Duration;

/// Crate result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Stable classification used by retries and terminal outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    Configuration,
    NotFound,
    Transient,
    Timeout,
    InvalidInput,
    Denied,
    Permanent,
    Routing,
    StateReduction,
    CircuitOpen,
    Limit,
    Cancelled,
    Panic,
    Internal,
}

/// Cloneable execution error. Runtime errors are values because they are kept
/// in stage history and surfaced through in-process events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: Arc<str>,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<Arc<str>>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Configuration, message)
    }

    pub fn not_found(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn transient(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Transient, message)
    }

    pub fn timeout(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Timeout, message)
    }

    pub fn invalid_input(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn denied(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Denied, message)
    }

    pub fn permanent(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Permanent, message)
    }

    pub fn routing(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Routing, message)
    }

    pub fn state_reduction(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::StateReduction, message)
    }

    pub fn circuit_open(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::CircuitOpen, message)
    }

    pub fn limit(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Limit, message)
    }

    pub fn cancelled(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Cancelled, message)
    }

    pub fn panic(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Panic, message)
    }

    pub fn internal(message: impl Into<Arc<str>>) -> Self {
        Self::new(ErrorKind::Internal, message)
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            ErrorKind::Transient | ErrorKind::Timeout | ErrorKind::CircuitOpen
        )
    }

    pub(crate) fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.kind = kind;
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::invalid_input(value.to_string())
    }
}

/// Run identifier. It is used for attribution, never for global lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId(Arc<str>);

impl RunId {
    pub fn new() -> Self {
        Self(Arc::from(uuid::Uuid::new_v4().to_string()))
    }

    pub fn from_string(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::invalid_input("run id cannot be empty"));
        }
        Ok(Self(Arc::from(value)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Dynamic input and consumer metadata supplied to a run.
#[derive(Debug, Clone)]
pub struct RunInput {
    pub input: Value,
    pub metadata: Map<String, Value>,
}

impl RunInput {
    pub fn new(input: Value) -> Self {
        Self {
            input,
            metadata: Map::new(),
        }
    }

    pub fn text(input: impl Into<String>) -> Self {
        Self::new(Value::String(input.into()))
    }

    pub fn with_metadata(mut self, metadata: Map<String, Value>) -> Self {
        self.metadata = metadata;
        self
    }
}

impl From<&str> for RunInput {
    fn from(value: &str) -> Self {
        Self::text(value)
    }
}

impl From<String> for RunInput {
    fn from(value: String) -> Self {
        Self::text(value)
    }
}

impl From<Value> for RunInput {
    fn from(value: Value) -> Self {
        Self::new(value)
    }
}

/// Resource usage accumulated by actions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub llm_calls: u32,
    pub tool_calls: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl AddAssign for Usage {
    fn add_assign(&mut self, rhs: Self) {
        self.llm_calls = self.llm_calls.saturating_add(rhs.llm_calls);
        self.tool_calls = self.tool_calls.saturating_add(rhs.tool_calls);
        self.input_tokens = self.input_tokens.saturating_add(rhs.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(rhs.output_tokens);
    }
}

impl Sub for Usage {
    type Output = Usage;

    fn sub(self, rhs: Self) -> Self::Output {
        Usage {
            llm_calls: self.llm_calls.saturating_sub(rhs.llm_calls),
            tool_calls: self.tool_calls.saturating_sub(rhs.tool_calls),
            input_tokens: self.input_tokens.saturating_sub(rhs.input_tokens),
            output_tokens: self.output_tokens.saturating_sub(rhs.output_tokens),
        }
    }
}

/// Identity shared by all events produced during one stage attempt.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StageAttempt {
    pub stage: Arc<str>,
    pub visit: u32,
    pub attempt: u32,
}

/// The execution phase that produced a stage-attempt failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageFailurePhase {
    Action,
    Routing,
    StateReduction,
    Control,
}

/// One failure retained on an attempt. Multiple phases may fail, so history
/// keeps every cause instead of overwriting the original action error.
#[derive(Debug, Clone)]
pub struct StageFailure {
    pub phase: StageFailurePhase,
    pub error: Error,
}

impl StageFailure {
    pub fn new(phase: StageFailurePhase, error: Error) -> Self {
        Self { phase, error }
    }
}

/// One executed stage attempt. Repeated visits and retries remain distinct.
#[derive(Debug, Clone)]
pub struct StageRecord {
    pub stage: Arc<str>,
    pub visit: u32,
    pub attempt: u32,
    pub output: Option<Value>,
    pub failures: Vec<StageFailure>,
    pub usage: Usage,
    pub duration: Duration,
}

impl StageRecord {
    pub fn succeeded(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn last_failure(&self) -> Option<&StageFailure> {
        self.failures.last()
    }
}

/// Read-only run data supplied to actions, prompts, routing, and reducers.
#[derive(Debug, Clone, Copy)]
pub struct RunView<'a> {
    pub run_id: &'a RunId,
    pub input: &'a Value,
    pub metadata: &'a Map<String, Value>,
    pub state: &'a Value,
    pub history: &'a [StageRecord],
    pub usage: Usage,
}

impl RunView<'_> {
    pub fn latest_output(&self, stage: &str) -> Option<&Value> {
        self.history
            .iter()
            .rev()
            .find(|record| record.stage.as_ref() == stage && record.succeeded())
            .and_then(|record| record.output.as_ref())
    }

    pub fn output_history<'a>(&'a self, stage: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
        self.history.iter().filter_map(move |record| {
            (record.stage.as_ref() == stage && record.succeeded())
                .then_some(record.output.as_ref())
                .flatten()
        })
    }

    pub fn input_text(&self) -> Option<&str> {
        self.input.as_str()
    }
}

/// Completed execution data shared by terminal outcomes and events.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub run_id: RunId,
    pub workflow: Arc<str>,
    pub latest_outputs: HashMap<Arc<str>, Value>,
    pub history: Vec<StageRecord>,
    pub state: Value,
    pub usage: Usage,
    pub duration: Duration,
}

/// The configured bound that ended a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    StageExecutions,
    LlmCalls,
    ToolCalls,
    StageVisits,
    ToolRounds,
    Deadline,
}

/// Exactly one terminal outcome is produced for each run.
#[derive(Debug, Clone)]
pub enum RunOutcome {
    Completed(RunResult),
    Failed { result: RunResult, error: Error },
    Cancelled(RunResult),
    LimitExceeded { result: RunResult, limit: LimitKind },
}

impl RunOutcome {
    pub fn result(&self) -> &RunResult {
        match self {
            Self::Completed(result)
            | Self::Cancelled(result)
            | Self::Failed { result, .. }
            | Self::LimitExceeded { result, .. } => result,
        }
    }

    pub fn completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }
}
