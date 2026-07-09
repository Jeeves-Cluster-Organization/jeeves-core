//! LLM-callable tools and composable reliability wrappers.

use async_trait::async_trait;
use serde_json::{Map, Value};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::types::{Error, ErrorKind, Result, RunId, StageAttempt};

/// Model-visible metadata for one tool.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: Arc<str>,
    pub description: Arc<str>,
    pub parameters: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<Arc<str>>,
        description: impl Into<Arc<str>>,
        parameters: Value,
    ) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(Error::configuration("tool name cannot be empty"));
        }
        Ok(Self {
            name,
            description: description.into(),
            parameters,
        })
    }
}

/// Human approval requested before a tool executes.
#[derive(Debug, Clone)]
pub struct ApprovalPrompt {
    pub message: Arc<str>,
    pub data: Option<Value>,
}

impl ApprovalPrompt {
    pub fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Trusted invocation data supplied by the runtime, never by the model.
#[derive(Debug, Clone)]
pub struct ToolContext<'a> {
    pub run_id: &'a RunId,
    pub workflow: &'a str,
    pub attempt: &'a StageAttempt,
    pub call_id: &'a str,
    pub metadata: &'a Map<String, Value>,
    pub cancellation: CancellationToken,
}

/// A single callable tool implementation.
#[async_trait]
pub trait Tool: Send + Sync {
    async fn call(&self, context: &ToolContext<'_>, arguments: Value) -> Result<Value>;

    /// Return an approval prompt for this invocation, or `None` to execute it.
    async fn approval(
        &self,
        _context: &ToolContext<'_>,
        _arguments: &Value,
    ) -> Result<Option<ApprovalPrompt>> {
        Ok(None)
    }
}

/// Whether a tool may be invoked again after an indeterminate stage failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaySafety {
    Unsafe,
    Idempotent,
}

/// One authoritative tool record: specification and matching handler.
#[derive(Clone)]
pub struct ToolDefinition {
    pub spec: ToolSpec,
    pub handler: Arc<dyn Tool>,
    pub replay_safety: ReplaySafety,
}

impl ToolDefinition {
    pub fn new(spec: ToolSpec, handler: Arc<dyn Tool>) -> Self {
        Self {
            spec,
            handler,
            replay_safety: ReplaySafety::Unsafe,
        }
    }

    pub fn with_replay_safety(mut self, replay_safety: ReplaySafety) -> Self {
        self.replay_safety = replay_safety;
        self
    }

    pub fn idempotent(self) -> Self {
        self.with_replay_safety(ReplaySafety::Idempotent)
    }
}

impl fmt::Debug for ToolDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolDefinition")
            .field("spec", &self.spec)
            .field("replay_safety", &self.replay_safety)
            .finish_non_exhaustive()
    }
}

/// Consumer response to an in-memory approval request.
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    pub request_id: Arc<str>,
    pub approved: bool,
}

impl ApprovalResponse {
    pub fn approve(request_id: impl Into<Arc<str>>) -> Self {
        Self {
            request_id: request_id.into(),
            approved: true,
        }
    }

    pub fn deny(request_id: impl Into<Arc<str>>) -> Self {
        Self {
            request_id: request_id.into(),
            approved: false,
        }
    }
}

/// Approval request emitted by a running workflow.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub request_id: Arc<str>,
    pub attempt: StageAttempt,
    pub call_id: Arc<str>,
    pub tool: Arc<str>,
    pub arguments: Value,
    pub prompt: ApprovalPrompt,
}

/// Behavior applied when a consumer denies an invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialBehavior {
    Continue,
    FailStage,
    CancelRun,
}

/// Minimal circuit-breaker configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitFailurePolicy {
    /// Count service-side and infrastructure failures, not caller mistakes or
    /// control-flow outcomes.
    ServiceFailures,
    /// Count every returned error.
    AllErrors,
}

#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub cooldown: Duration,
    pub failure_policy: CircuitFailurePolicy,
}

impl CircuitBreakerConfig {
    pub fn validate(self) -> Result<Self> {
        if self.failure_threshold == 0 {
            return Err(Error::configuration(
                "circuit-breaker failure threshold must be positive",
            ));
        }
        Ok(self)
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooldown: Duration::from_secs(30),
            failure_policy: CircuitFailurePolicy::ServiceFailures,
        }
    }
}

/// Inspectable public breaker state. Half-open probing remains internal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerStatus {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
enum BreakerState {
    Closed { consecutive_failures: u32 },
    Open { opened_at: Instant },
    HalfOpen,
}

/// Opt-in tool wrapper. It has no dependency on an engine or global registry.
pub struct CircuitBreakerTool {
    inner: Arc<dyn Tool>,
    config: CircuitBreakerConfig,
    state: Mutex<BreakerState>,
}

impl CircuitBreakerTool {
    pub fn new(inner: Arc<dyn Tool>, config: CircuitBreakerConfig) -> Result<Self> {
        Ok(Self {
            inner,
            config: config.validate()?,
            state: Mutex::new(BreakerState::Closed {
                consecutive_failures: 0,
            }),
        })
    }

    pub fn status(&self) -> CircuitBreakerStatus {
        let Ok(state) = self.state.lock() else {
            return CircuitBreakerStatus::Open;
        };
        match *state {
            BreakerState::Closed { .. } => CircuitBreakerStatus::Closed,
            BreakerState::Open { opened_at } if opened_at.elapsed() >= self.config.cooldown => {
                CircuitBreakerStatus::HalfOpen
            }
            BreakerState::Open { .. } => CircuitBreakerStatus::Open,
            BreakerState::HalfOpen => CircuitBreakerStatus::HalfOpen,
        }
    }

    fn begin_call(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::internal("circuit-breaker state poisoned"))?;
        match *state {
            BreakerState::Closed { .. } => Ok(()),
            BreakerState::Open { opened_at } if opened_at.elapsed() >= self.config.cooldown => {
                *state = BreakerState::HalfOpen;
                Ok(())
            }
            BreakerState::Open { .. } | BreakerState::HalfOpen => {
                Err(Error::circuit_open("tool circuit breaker is open"))
            }
        }
    }

    fn counts_failure(&self, error: &Error) -> bool {
        match self.config.failure_policy {
            CircuitFailurePolicy::AllErrors => true,
            CircuitFailurePolicy::ServiceFailures => matches!(
                error.kind(),
                ErrorKind::Transient
                    | ErrorKind::Timeout
                    | ErrorKind::Permanent
                    | ErrorKind::CircuitOpen
                    | ErrorKind::Panic
                    | ErrorKind::Internal
            ),
        }
    }

    fn finish_call(&self, result: &Result<Value>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        match result {
            Ok(_) => {
                *state = BreakerState::Closed {
                    consecutive_failures: 0,
                };
            }
            Err(error) if self.counts_failure(error) => {
                let next_failures = match *state {
                    BreakerState::Closed {
                        consecutive_failures,
                    } => consecutive_failures.saturating_add(1),
                    BreakerState::HalfOpen | BreakerState::Open { .. } => {
                        self.config.failure_threshold
                    }
                };
                if next_failures >= self.config.failure_threshold {
                    *state = BreakerState::Open {
                        opened_at: Instant::now(),
                    };
                } else {
                    *state = BreakerState::Closed {
                        consecutive_failures: next_failures,
                    };
                }
            }
            Err(_) if matches!(*state, BreakerState::HalfOpen) => {
                // A caller error does not prove recovery; reopen and permit a
                // later valid probe after the cooldown.
                *state = BreakerState::Open {
                    opened_at: Instant::now(),
                };
            }
            Err(_) => {}
        }
    }
}

impl fmt::Debug for CircuitBreakerTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CircuitBreakerTool")
            .field("config", &self.config)
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Tool for CircuitBreakerTool {
    async fn call(&self, context: &ToolContext<'_>, arguments: Value) -> Result<Value> {
        self.begin_call()?;
        let result = self.inner.call(context, arguments).await;
        self.finish_call(&result);
        result
    }

    async fn approval(
        &self,
        context: &ToolContext<'_>,
        arguments: &Value,
    ) -> Result<Option<ApprovalPrompt>> {
        self.inner.approval(context, arguments).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FailsTwice(AtomicU32);

    #[async_trait]
    impl Tool for FailsTwice {
        async fn call(&self, _context: &ToolContext<'_>, _arguments: Value) -> Result<Value> {
            let call = self.0.fetch_add(1, Ordering::SeqCst);
            if call < 2 {
                Err(Error::transient("not yet"))
            } else {
                Ok(Value::Bool(true))
            }
        }
    }

    #[tokio::test]
    async fn breaker_opens_and_recovers_after_cooldown() -> Result<()> {
        let breaker = CircuitBreakerTool::new(
            Arc::new(FailsTwice(AtomicU32::new(0))),
            CircuitBreakerConfig {
                failure_threshold: 2,
                cooldown: Duration::from_millis(1),
                failure_policy: CircuitFailurePolicy::ServiceFailures,
            },
        )?;

        let run_id = RunId::new();
        let attempt = StageAttempt {
            stage: Arc::from("test"),
            visit: 1,
            attempt: 1,
        };
        let metadata = Map::new();
        let context = ToolContext {
            run_id: &run_id,
            workflow: "test",
            attempt: &attempt,
            call_id: "call-1",
            metadata: &metadata,
            cancellation: CancellationToken::new(),
        };

        assert!(breaker.call(&context, Value::Null).await.is_err());
        assert!(breaker.call(&context, Value::Null).await.is_err());
        assert_eq!(breaker.status(), CircuitBreakerStatus::Open);
        assert!(matches!(
            breaker.call(&context, Value::Null).await,
            Err(error) if error.kind() == crate::types::ErrorKind::CircuitOpen
        ));

        tokio::time::sleep(Duration::from_millis(2)).await;
        assert_eq!(breaker.status(), CircuitBreakerStatus::HalfOpen);
        assert_eq!(
            breaker.call(&context, Value::Null).await,
            Ok(Value::Bool(true))
        );
        assert_eq!(breaker.status(), CircuitBreakerStatus::Closed);
        Ok(())
    }
}
