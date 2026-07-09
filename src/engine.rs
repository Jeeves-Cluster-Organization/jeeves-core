//! Direct workflow execution with per-run ownership.

use futures::{FutureExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::events::{RoutingReason, RunEvent};
use crate::llm::{
    LlmAction, LlmOutput, LlmProvider, Message, ModelRequest, ModelResponse, ModelStreamEvent,
    ToolCall, ToolDecision, ToolResult,
};
use crate::tools::{
    ApprovalRequest, ApprovalResponse, DenialBehavior, ToolContext, ToolDefinition,
};
use crate::types::{
    Error, ErrorKind, LimitKind, Result, RunId, RunInput, RunOutcome, RunResult, RunView,
    StageAttempt, StageFailure, StageFailurePhase, StageRecord, Usage,
};
use crate::workflow::{RetryOn, Route, Stage, StageAction, StageRouting, ToolAction, Workflow};

/// Immutable collection of workflows and shared provider resources.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    workflows: HashMap<Arc<str>, Arc<Workflow>>,
    llm: Option<Arc<dyn LlmProvider>>,
}

impl Engine {
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    pub fn start(&self, workflow: &str, input: impl Into<RunInput>) -> Result<RunHandle> {
        let workflow = self
            .inner
            .workflows
            .get(workflow)
            .cloned()
            .ok_or_else(|| Error::not_found(format!("workflow '{workflow}' not found")))?;
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| Error::configuration("Engine::start requires a Tokio runtime"))?;

        let run_id = RunId::new();
        let cancellation = CancellationToken::new();
        let lifetime = Arc::new(RunLifetime {
            cancellation: cancellation.clone(),
        });
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (approval_tx, approval_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = watch::channel(None);

        let mut execution = Execution {
            workflow,
            llm: self.inner.llm.clone(),
            state: ExecutionState::new(run_id.clone(), input.into()),
            event_tx,
            approval_rx,
            cancellation,
        };
        runtime.spawn(async move {
            let stop = match AssertUnwindSafe(execution.run()).catch_unwind().await {
                Ok(stop) => stop,
                Err(payload) => Stop::Failed(Error::panic(format!(
                    "workflow execution panicked: {}",
                    panic_message(payload)
                ))),
            };
            let outcome = execution.finish(stop);
            let _ = execution_finished_event(&outcome, &result_tx);
        });

        Ok(RunHandle {
            run_id,
            lifetime,
            approval_tx,
            result_rx,
            events: Some(event_rx),
        })
    }

    /// Execute without consuming events. The same streaming provider path is
    /// collected internally.
    pub async fn run(&self, workflow: &str, input: impl Into<RunInput>) -> Result<Arc<RunOutcome>> {
        let mut handle = self.start(workflow, input)?;
        // No consumer will observe this receiver, so close it before execution
        // can accumulate an unbounded stream of events.
        drop(handle.take_events());
        handle.result().await
    }
}

fn execution_finished_event(
    outcome: &Arc<RunOutcome>,
    result_tx: &watch::Sender<Option<Arc<RunOutcome>>>,
) -> Result<()> {
    // Result publication is deliberately independent of event consumption.
    result_tx
        .send(Some(outcome.clone()))
        .map_err(|_| Error::internal("all run handles dropped before result publication"))
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("workflows", &self.inner.workflows.keys())
            .field("has_llm", &self.inner.llm.is_some())
            .finish()
    }
}

/// Engine construction. Workflows are validated before they reach this layer.
#[derive(Default)]
pub struct EngineBuilder {
    workflows: HashMap<Arc<str>, Arc<Workflow>>,
    llm: Option<Arc<dyn LlmProvider>>,
}

impl EngineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn llm(mut self, llm: Arc<dyn LlmProvider>) -> Self {
        self.llm = Some(llm);
        self
    }

    pub fn workflow(mut self, workflow: Workflow) -> Result<Self> {
        let name: Arc<str> = Arc::from(workflow.name());
        if self
            .workflows
            .insert(name.clone(), Arc::new(workflow))
            .is_some()
        {
            return Err(Error::configuration(format!("duplicate workflow '{name}'")));
        }
        Ok(self)
    }

    pub fn build(self) -> Result<Engine> {
        if self.workflows.is_empty() {
            return Err(Error::configuration(
                "engine must contain at least one workflow",
            ));
        }
        let needs_llm = self.workflows.values().any(|workflow| {
            workflow
                .stages()
                .iter()
                .any(|stage| matches!(stage.action, StageAction::Llm(_)))
        });
        if needs_llm && self.llm.is_none() {
            return Err(Error::configuration(
                "an LLM provider is required by at least one workflow",
            ));
        }
        Ok(Engine {
            inner: Arc::new(EngineInner {
                workflows: self.workflows,
                llm: self.llm,
            }),
        })
    }
}

impl fmt::Debug for EngineBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineBuilder")
            .field("workflows", &self.workflows.keys())
            .field("has_llm", &self.llm.is_some())
            .finish()
    }
}

struct RunLifetime {
    cancellation: CancellationToken,
}

impl Drop for RunLifetime {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

/// Handle-owned run control. Clones control the same run, while the event
/// receiver can be taken exactly once from the original handle.
pub struct RunHandle {
    run_id: RunId,
    lifetime: Arc<RunLifetime>,
    approval_tx: mpsc::UnboundedSender<ApprovalResponse>,
    result_rx: watch::Receiver<Option<Arc<RunOutcome>>>,
    events: Option<mpsc::UnboundedReceiver<RunEvent>>,
}

impl RunHandle {
    pub fn id(&self) -> &RunId {
        &self.run_id
    }

    pub fn cancel(&self) {
        self.lifetime.cancellation.cancel();
    }

    pub fn respond_to_approval(&self, response: ApprovalResponse) -> Result<()> {
        self.approval_tx
            .send(response)
            .map_err(|_| Error::cancelled("run is no longer awaiting control messages"))
    }

    pub fn take_events(&mut self) -> Option<mpsc::UnboundedReceiver<RunEvent>> {
        self.events.take()
    }

    /// Close the event stream when the caller only needs control and result.
    /// A later approval request will fail because it cannot be presented.
    pub fn discard_events(&mut self) {
        drop(self.events.take());
    }

    pub async fn result(&self) -> Result<Arc<RunOutcome>> {
        let mut receiver = self.result_rx.clone();
        loop {
            if let Some(outcome) = receiver.borrow().clone() {
                return Ok(outcome);
            }
            receiver
                .changed()
                .await
                .map_err(|_| Error::internal("run ended without publishing a result"))?;
        }
    }
}

impl Clone for RunHandle {
    fn clone(&self) -> Self {
        Self {
            run_id: self.run_id.clone(),
            lifetime: self.lifetime.clone(),
            approval_tx: self.approval_tx.clone(),
            result_rx: self.result_rx.clone(),
            events: None,
        }
    }
}

impl fmt::Debug for RunHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunHandle")
            .field("run_id", &self.run_id)
            .field("owns_events", &self.events.is_some())
            .finish_non_exhaustive()
    }
}

struct ExecutionState {
    run_id: RunId,
    input: RunInput,
    state: Value,
    history: Vec<StageRecord>,
    usage: Usage,
    stage_executions: u32,
    visits: HashMap<Arc<str>, u32>,
    started: Instant,
}

impl ExecutionState {
    fn new(run_id: RunId, input: RunInput) -> Self {
        Self {
            run_id,
            input,
            state: Value::Null,
            history: Vec::new(),
            usage: Usage::default(),
            stage_executions: 0,
            visits: HashMap::new(),
            started: Instant::now(),
        }
    }

    fn view(&self) -> RunView<'_> {
        RunView {
            run_id: &self.run_id,
            input: &self.input.input,
            metadata: &self.input.metadata,
            state: &self.state,
            history: &self.history,
            usage: self.usage,
        }
    }
}

struct Execution {
    workflow: Arc<Workflow>,
    llm: Option<Arc<dyn LlmProvider>>,
    state: ExecutionState,
    event_tx: mpsc::UnboundedSender<RunEvent>,
    approval_rx: mpsc::UnboundedReceiver<ApprovalResponse>,
    cancellation: CancellationToken,
}

enum Stop {
    Completed,
    Failed(Error),
    Cancelled,
    Limit(LimitKind),
}

enum ActionStop {
    Error(Error),
    Cancelled,
    Limit(LimitKind),
}

struct ToolCallGuard {
    event_tx: mpsc::UnboundedSender<RunEvent>,
    attempt: StageAttempt,
    call_id: Arc<str>,
    tool: Arc<str>,
    started: Instant,
    finished: bool,
}

impl ToolCallGuard {
    fn new(
        event_tx: mpsc::UnboundedSender<RunEvent>,
        attempt: StageAttempt,
        call_id: Arc<str>,
        tool: Arc<str>,
    ) -> Self {
        Self {
            event_tx,
            attempt,
            call_id,
            tool,
            started: Instant::now(),
            finished: false,
        }
    }

    fn finish(mut self, result: Result<Value>) {
        self.finished = true;
        let _ = self.event_tx.send(RunEvent::ToolCallFinished {
            attempt: self.attempt.clone(),
            call_id: self.call_id.clone(),
            tool: self.tool.clone(),
            result,
            duration: self.started.elapsed(),
        });
    }
}

impl Drop for ToolCallGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.event_tx.send(RunEvent::ToolCallAborted {
                attempt: self.attempt.clone(),
                call_id: self.call_id.clone(),
                tool: self.tool.clone(),
                duration: self.started.elapsed(),
            });
        }
    }
}

impl Execution {
    async fn run(&mut self) -> Stop {
        self.state.state = self.workflow.initial_state.clone();
        self.execute_loop().await
    }

    fn finish(&self, stop: Stop) -> Arc<RunOutcome> {
        let result = self.finish_result();
        let outcome = match stop {
            Stop::Completed => RunOutcome::Completed(result),
            Stop::Failed(error) => RunOutcome::Failed { result, error },
            Stop::Cancelled => RunOutcome::Cancelled(result),
            Stop::Limit(limit) => RunOutcome::LimitExceeded { result, limit },
        };
        let outcome = Arc::new(outcome);
        let _ = self.event_tx.send(RunEvent::Finished(outcome.clone()));
        outcome
    }

    async fn execute_loop(&mut self) -> Stop {
        let mut current = self.workflow.stages[0].name.clone();
        loop {
            if self.cancellation.is_cancelled() {
                return Stop::Cancelled;
            }
            if self.deadline_expired() {
                return Stop::Limit(LimitKind::Deadline);
            }

            let Some(stage) = self.workflow.stage(&current).cloned() else {
                return Stop::Failed(Error::routing(format!(
                    "stage '{}' does not exist",
                    current
                )));
            };
            let visit = self.state.visits.entry(current.clone()).or_default();
            *visit = visit.saturating_add(1);
            let visit = *visit;
            if stage.max_visits.is_some_and(|maximum| visit > maximum) {
                return Stop::Limit(LimitKind::StageVisits);
            }

            let mut attempt_number = 1;
            loop {
                if self.state.stage_executions >= self.workflow.limits.max_stage_executions {
                    return Stop::Limit(LimitKind::StageExecutions);
                }
                self.state.stage_executions += 1;
                let attempt = StageAttempt {
                    stage: stage.name.clone(),
                    visit,
                    attempt: attempt_number,
                };
                let _ = self.event_tx.send(RunEvent::StageStarted {
                    attempt: attempt.clone(),
                });

                let before_usage = self.state.usage;
                let started = Instant::now();
                let action_result = match AssertUnwindSafe(self.execute_attempt(&stage, &attempt))
                    .catch_unwind()
                    .await
                {
                    Ok(result) => result,
                    Err(payload) => Err(ActionStop::Error(Error::panic(format!(
                        "stage '{}' action panicked: {}",
                        stage.name,
                        panic_message(payload)
                    )))),
                };
                let mut record = StageRecord {
                    stage: stage.name.clone(),
                    visit,
                    attempt: attempt_number,
                    output: action_result.as_ref().ok().cloned(),
                    failures: Vec::new(),
                    usage: self.state.usage - before_usage,
                    duration: started.elapsed(),
                };

                match action_result {
                    Ok(_) => {
                        // Routing can inspect this attempt's output, but state
                        // remains the value committed by prior attempts.
                        self.state.history.push(record.clone());
                        let routing = self.route_success(&stage);
                        let _ = self.state.history.pop();

                        let (route, routing_error) = match routing {
                            Ok(route) => (Some(route), None),
                            Err(error) => {
                                record.failures.push(StageFailure::new(
                                    StageFailurePhase::Routing,
                                    error.clone(),
                                ));
                                (None, Some(error))
                            }
                        };
                        let reduction_error = self.reduce_record(&mut record);
                        self.publish_record(record);

                        if let Some(error) = reduction_error.or(routing_error) {
                            if let Some(next) = &stage.on_error {
                                let _ = self.event_tx.send(RunEvent::Routed {
                                    from: stage.name.clone(),
                                    to: Some(next.clone()),
                                    reason: RoutingReason::ErrorRecovery,
                                });
                                current = next.clone();
                                break;
                            }
                            return Stop::Failed(error);
                        }

                        let Some(route) = route else {
                            return Stop::Failed(Error::internal(
                                "routing result missing without a routing failure",
                            ));
                        };
                        match route {
                            Some(next) => {
                                let reason = if matches!(stage.routing, StageRouting::Dynamic(_)) {
                                    RoutingReason::Dynamic
                                } else {
                                    RoutingReason::Success
                                };
                                let _ = self.event_tx.send(RunEvent::Routed {
                                    from: stage.name.clone(),
                                    to: Some(next.clone()),
                                    reason,
                                });
                                current = next;
                                break;
                            }
                            None => {
                                let _ = self.event_tx.send(RunEvent::Routed {
                                    from: stage.name.clone(),
                                    to: None,
                                    reason: RoutingReason::Success,
                                });
                                return Stop::Completed;
                            }
                        }
                    }
                    Err(ActionStop::Cancelled) => {
                        record.failures.push(StageFailure::new(
                            StageFailurePhase::Control,
                            Error::cancelled("run cancelled during stage execution"),
                        ));
                        self.publish_record(record);
                        return Stop::Cancelled;
                    }
                    Err(ActionStop::Limit(limit)) => {
                        record.failures.push(StageFailure::new(
                            StageFailurePhase::Control,
                            Error::limit(format!("run limit reached: {limit:?}")),
                        ));
                        self.publish_record(record);
                        return Stop::Limit(limit);
                    }
                    Err(ActionStop::Error(error)) => {
                        record
                            .failures
                            .push(StageFailure::new(StageFailurePhase::Action, error.clone()));
                        let reduction_error = self.reduce_record(&mut record);
                        self.publish_record(record);

                        if let Some(reduction_error) = reduction_error {
                            if let Some(next) = &stage.on_error {
                                let _ = self.event_tx.send(RunEvent::Routed {
                                    from: stage.name.clone(),
                                    to: Some(next.clone()),
                                    reason: RoutingReason::ErrorRecovery,
                                });
                                current = next.clone();
                                break;
                            }
                            return Stop::Failed(reduction_error);
                        }

                        if self.should_retry(&stage, &error, attempt_number) {
                            let Some(policy) = stage.retry else {
                                return Stop::Failed(Error::internal(
                                    "retry selected without a retry policy",
                                ));
                            };
                            let backoff = policy.backoff_for_retry(attempt_number);
                            match self.wait_backoff(backoff).await {
                                Ok(()) => {}
                                Err(ActionStop::Cancelled) => return Stop::Cancelled,
                                Err(ActionStop::Limit(limit)) => return Stop::Limit(limit),
                                Err(ActionStop::Error(error)) => return Stop::Failed(error),
                            }
                            attempt_number += 1;
                            continue;
                        }
                        if let Some(next) = &stage.on_error {
                            let _ = self.event_tx.send(RunEvent::Routed {
                                from: stage.name.clone(),
                                to: Some(next.clone()),
                                reason: RoutingReason::ErrorRecovery,
                            });
                            current = next.clone();
                            break;
                        }
                        return Stop::Failed(error);
                    }
                }
            }
        }
    }

    /// Apply state reduction transactionally. Control stops deliberately skip
    /// this path, so cancellation and limit outcomes cannot be replaced by a
    /// reducer failure.
    fn reduce_record(&mut self, record: &mut StageRecord) -> Option<Error> {
        let Some(reducer) = &self.workflow.reducer else {
            return None;
        };
        let reduced = std::panic::catch_unwind(AssertUnwindSafe(|| {
            reducer.reduce(&self.state.state, record)
        }));
        match reduced {
            Ok(Ok(next_state)) => {
                self.state.state = next_state;
                None
            }
            Ok(Err(error)) => {
                let error = error.with_kind(ErrorKind::StateReduction);
                record.failures.push(StageFailure::new(
                    StageFailurePhase::StateReduction,
                    error.clone(),
                ));
                Some(error)
            }
            Err(payload) => {
                let error = Error::panic(format!(
                    "state reducer panicked: {}",
                    panic_message(payload)
                ));
                record.failures.push(StageFailure::new(
                    StageFailurePhase::StateReduction,
                    error.clone(),
                ));
                Some(error)
            }
        }
    }

    fn publish_record(&mut self, record: StageRecord) {
        self.state.history.push(record.clone());
        let _ = self
            .event_tx
            .send(RunEvent::StageAttemptFinished(Arc::new(record)));
    }

    fn should_retry(&self, stage: &Stage, error: &Error, attempt: u32) -> bool {
        let Some(policy) = stage.retry else {
            return false;
        };
        if attempt >= policy.max_attempts {
            return false;
        }
        match policy.retry_on {
            RetryOn::Retryable => error.is_retryable(),
            RetryOn::AnyFailure => true,
        }
    }

    async fn wait_backoff(&self, duration: Duration) -> std::result::Result<(), ActionStop> {
        let (duration, deadline_limited) = match self.workflow.limits.deadline {
            Some(deadline) => {
                let Some(remaining) = deadline.checked_sub(self.state.started.elapsed()) else {
                    return Err(ActionStop::Limit(LimitKind::Deadline));
                };
                if remaining <= duration {
                    (remaining, true)
                } else {
                    (duration, false)
                }
            }
            None => (duration, false),
        };
        tokio::select! {
            _ = self.cancellation.cancelled() => Err(ActionStop::Cancelled),
            _ = tokio::time::sleep(duration) => {
                if deadline_limited {
                    Err(ActionStop::Limit(LimitKind::Deadline))
                } else {
                    Ok(())
                }
            },
        }
    }

    fn route_success(&self, stage: &Stage) -> Result<Option<Arc<str>>> {
        let route = match &stage.routing {
            StageRouting::Complete => Route::Complete,
            StageRouting::Next(next) => Route::Next(next.clone()),
            StageRouting::Dynamic(router) => {
                match std::panic::catch_unwind(AssertUnwindSafe(|| {
                    router.route(&self.state.view())
                })) {
                    Ok(result) => result.map_err(|error| error.with_kind(ErrorKind::Routing))?,
                    Err(payload) => {
                        return Err(Error::panic(format!(
                            "router for stage '{}' panicked: {}",
                            stage.name,
                            panic_message(payload)
                        )))
                    }
                }
            }
        };
        match route {
            Route::Complete => Ok(None),
            Route::Next(next) => {
                if self.workflow.stage(&next).is_none() {
                    return Err(Error::routing(format!(
                        "routing selected missing stage '{next}'"
                    )));
                }
                Ok(Some(next))
            }
        }
    }

    async fn execute_attempt(
        &mut self,
        stage: &Stage,
        attempt: &StageAttempt,
    ) -> std::result::Result<Value, ActionStop> {
        let timeout = self.effective_timeout(stage.timeout)?;
        let cancellation = self.cancellation.clone();
        let action = self.execute_action(stage, attempt);
        match timeout {
            Some((duration, deadline_limited)) => {
                tokio::select! {
                    _ = cancellation.cancelled() => Err(ActionStop::Cancelled),
                    result = tokio::time::timeout(duration, action) => match result {
                        Ok(result) => result,
                        Err(_) if deadline_limited => Err(ActionStop::Limit(LimitKind::Deadline)),
                        Err(_) => Err(ActionStop::Error(Error::timeout(format!(
                            "stage '{}' timed out after {:?}", stage.name, duration
                        )))),
                    }
                }
            }
            None => tokio::select! {
                _ = cancellation.cancelled() => Err(ActionStop::Cancelled),
                result = action => result,
            },
        }
    }

    fn effective_timeout(
        &self,
        stage_timeout: Option<Duration>,
    ) -> std::result::Result<Option<(Duration, bool)>, ActionStop> {
        let remaining = self.workflow.limits.deadline.map(|deadline| {
            deadline
                .checked_sub(self.state.started.elapsed())
                .unwrap_or(Duration::ZERO)
        });
        if remaining == Some(Duration::ZERO) {
            return Err(ActionStop::Limit(LimitKind::Deadline));
        }
        Ok(match (stage_timeout, remaining) {
            (Some(stage), Some(run)) if run <= stage => Some((run, true)),
            (Some(stage), Some(_)) => Some((stage, false)),
            (Some(stage), None) => Some((stage, false)),
            (None, Some(run)) => Some((run, true)),
            (None, None) => None,
        })
    }

    fn deadline_expired(&self) -> bool {
        self.workflow
            .limits
            .deadline
            .is_some_and(|deadline| self.state.started.elapsed() >= deadline)
    }

    async fn execute_action(
        &mut self,
        stage: &Stage,
        attempt: &StageAttempt,
    ) -> std::result::Result<Value, ActionStop> {
        match &stage.action {
            StageAction::RouteOnly => Ok(Value::Null),
            StageAction::Deterministic(action) => action
                .execute(&self.state.view())
                .await
                .map_err(ActionStop::Error),
            StageAction::Tool(action) => self.execute_direct_tool(attempt, action).await,
            StageAction::Llm(action) => self.execute_llm(attempt, action).await,
        }
    }

    async fn execute_direct_tool(
        &mut self,
        attempt: &StageAttempt,
        action: &ToolAction,
    ) -> std::result::Result<Value, ActionStop> {
        let arguments = action
            .build_arguments(&self.state.view())
            .map_err(ActionStop::Error)?;
        let call_id: Arc<str> = Arc::from(uuid::Uuid::new_v4().to_string());
        let approval = {
            let context = self.tool_context(attempt, &call_id);
            action
                .tool
                .handler
                .approval(&context, &arguments)
                .await
                .map_err(ActionStop::Error)?
        };
        if let Some(prompt) = approval {
            let approved = self
                .request_approval(attempt, &call_id, &action.tool, arguments.clone(), prompt)
                .await?;
            if !approved {
                return match action.on_denied {
                    DenialBehavior::Continue => Ok(serde_json::json!({"denied": true})),
                    DenialBehavior::FailStage => {
                        Err(ActionStop::Error(Error::denied("tool call denied")))
                    }
                    DenialBehavior::CancelRun => Err(ActionStop::Cancelled),
                };
            }
        }
        self.call_tool(attempt, &action.tool, call_id, arguments)
            .await
    }

    async fn execute_llm(
        &mut self,
        attempt: &StageAttempt,
        action: &LlmAction,
    ) -> std::result::Result<Value, ActionStop> {
        let provider = self
            .llm
            .clone()
            .ok_or_else(|| ActionStop::Error(Error::configuration("LLM provider missing")))?;
        let prompt = action
            .prompt
            .build(&self.state.view())
            .map_err(ActionStop::Error)?;
        let user_input = match &self.state.input.input {
            Value::String(text) => text.clone(),
            value => value.to_string(),
        };
        let mut messages = vec![Message::system(prompt), Message::user(user_input)];
        let mut tool_rounds = 0;
        let mut model_call = 0;

        loop {
            self.consume_llm_call()?;
            model_call += 1;
            for hook in &action.hooks {
                hook.before_model(&mut messages)
                    .await
                    .map_err(ActionStop::Error)?;
            }
            let request = ModelRequest {
                messages: messages.clone(),
                tools: action.tools.iter().map(|tool| tool.spec.clone()).collect(),
                temperature: action.temperature,
                max_tokens: action.max_tokens,
                model: action.model.clone(),
                response_schema: match &action.output {
                    LlmOutput::Text => None,
                    LlmOutput::Structured { schema, .. } => Some(schema.clone()),
                },
            };
            let mut stream = provider.stream(&request).await.map_err(ActionStop::Error)?;
            let mut response = ModelResponse::default();
            while let Some(event) = stream.next().await {
                match event.map_err(ActionStop::Error)? {
                    ModelStreamEvent::Text(content) => {
                        response.text.push_str(&content);
                        if matches!(action.output, LlmOutput::Text) {
                            let _ = self.event_tx.send(RunEvent::TextDelta {
                                attempt: attempt.clone(),
                                model_call,
                                content,
                            });
                        }
                    }
                    ModelStreamEvent::ToolCall(call) => {
                        merge_tool_call(&mut response.tool_calls, call);
                    }
                    ModelStreamEvent::Usage(usage) => response.usage = usage,
                }
            }
            self.state.usage.input_tokens = self
                .state
                .usage
                .input_tokens
                .saturating_add(response.usage.input_tokens);
            self.state.usage.output_tokens = self
                .state
                .usage
                .output_tokens
                .saturating_add(response.usage.output_tokens);
            for hook in &action.hooks {
                hook.after_model(&mut response)
                    .await
                    .map_err(ActionStop::Error)?;
            }

            if response.tool_calls.is_empty() {
                return self.interpret_llm_output(action, response.text);
            }
            if tool_rounds >= action.max_tool_rounds {
                return Err(ActionStop::Limit(LimitKind::ToolRounds));
            }
            tool_rounds += 1;
            messages.push(Message::assistant(
                response.text.clone(),
                response.tool_calls.clone(),
            ));

            for call in response.tool_calls {
                let mut decision = ToolDecision::Continue;
                for hook in &action.hooks {
                    let hook_decision = hook.before_tool(&call).await.map_err(ActionStop::Error)?;
                    if !matches!(hook_decision, ToolDecision::Continue) {
                        decision = hook_decision;
                        break;
                    }
                }

                let mut result = match decision {
                    ToolDecision::Reject { reason } => ToolResult {
                        value: serde_json::json!({
                            "error": "tool_call_rejected",
                            "reason": reason,
                        }),
                        succeeded: false,
                    },
                    ToolDecision::Replace(value) => ToolResult {
                        value,
                        succeeded: true,
                    },
                    ToolDecision::Continue => self.execute_llm_tool(attempt, action, &call).await?,
                };
                for hook in &action.hooks {
                    hook.after_tool(&call, &mut result)
                        .await
                        .map_err(ActionStop::Error)?;
                }
                messages.push(Message::tool(&call.id, result.value.to_string()));
            }
        }
    }

    fn interpret_llm_output(
        &self,
        action: &LlmAction,
        text: String,
    ) -> std::result::Result<Value, ActionStop> {
        match &action.output {
            LlmOutput::Text => Ok(Value::String(text)),
            LlmOutput::Structured { validate, .. } => {
                let value: Value = serde_json::from_str(&text).map_err(|error| {
                    ActionStop::Error(Error::invalid_input(format!(
                        "structured LLM output is invalid JSON: {error}"
                    )))
                })?;
                validate(&value).map_err(ActionStop::Error)?;
                Ok(value)
            }
        }
    }

    async fn execute_llm_tool(
        &mut self,
        attempt: &StageAttempt,
        action: &LlmAction,
        call: &ToolCall,
    ) -> std::result::Result<ToolResult, ActionStop> {
        let Some(tool) = action
            .tools
            .iter()
            .find(|tool| tool.spec.name.as_ref() == call.name)
            .cloned()
        else {
            return Ok(ToolResult {
                value: serde_json::json!({"error": "tool_not_available", "tool": call.name}),
                succeeded: false,
            });
        };

        let call_id: Arc<str> = Arc::from(call.id.as_str());
        let approval = {
            let context = self.tool_context(attempt, &call_id);
            tool.handler
                .approval(&context, &call.arguments)
                .await
                .map_err(ActionStop::Error)?
        };
        if let Some(prompt) = approval {
            let approved = self
                .request_approval(attempt, &call_id, &tool, call.arguments.clone(), prompt)
                .await?;
            if !approved {
                return match action.on_denied {
                    DenialBehavior::Continue => Ok(ToolResult {
                        value: serde_json::json!({
                            "error": "approval_denied",
                            "tool": call.name,
                        }),
                        succeeded: false,
                    }),
                    DenialBehavior::FailStage => {
                        Err(ActionStop::Error(Error::denied("tool call denied")))
                    }
                    DenialBehavior::CancelRun => Err(ActionStop::Cancelled),
                };
            }
        }

        match self
            .call_tool(attempt, &tool, call_id, call.arguments.clone())
            .await
        {
            Ok(value) => Ok(ToolResult {
                value,
                succeeded: true,
            }),
            Err(ActionStop::Error(error)) => Ok(ToolResult {
                value: serde_json::json!({"error": error.to_string()}),
                succeeded: false,
            }),
            Err(stop) => Err(stop),
        }
    }

    async fn request_approval(
        &mut self,
        attempt: &StageAttempt,
        call_id: &Arc<str>,
        tool: &Arc<ToolDefinition>,
        arguments: Value,
        prompt: crate::tools::ApprovalPrompt,
    ) -> std::result::Result<bool, ActionStop> {
        let request_id: Arc<str> = Arc::from(uuid::Uuid::new_v4().to_string());
        let request = ApprovalRequest {
            request_id: request_id.clone(),
            attempt: attempt.clone(),
            call_id: call_id.clone(),
            tool: tool.spec.name.clone(),
            arguments,
            prompt,
        };
        self.event_tx
            .send(RunEvent::ApprovalRequested(request))
            .map_err(|_| {
                ActionStop::Error(Error::configuration(
                    "tool approval requires a retained event receiver",
                ))
            })?;
        loop {
            tokio::select! {
                _ = self.cancellation.cancelled() => return Err(ActionStop::Cancelled),
                _ = self.event_tx.closed() => {
                    return Err(ActionStop::Error(Error::configuration(
                        "tool approval event receiver was dropped",
                    )));
                }
                response = self.approval_rx.recv() => {
                    let Some(response) = response else {
                        return Err(ActionStop::Cancelled);
                    };
                    if response.request_id == request_id {
                        return Ok(response.approved);
                    }
                }
            }
        }
    }

    async fn call_tool(
        &mut self,
        attempt: &StageAttempt,
        tool: &Arc<ToolDefinition>,
        call_id: Arc<str>,
        arguments: Value,
    ) -> std::result::Result<Value, ActionStop> {
        self.consume_tool_call()?;
        let _ = self.event_tx.send(RunEvent::ToolCallStarted {
            attempt: attempt.clone(),
            call_id: call_id.clone(),
            tool: tool.spec.name.clone(),
            arguments: arguments.clone(),
        });
        let guard = ToolCallGuard::new(
            self.event_tx.clone(),
            attempt.clone(),
            call_id.clone(),
            tool.spec.name.clone(),
        );
        let result = {
            let context = self.tool_context(attempt, &call_id);
            tool.handler.call(&context, arguments).await
        };
        guard.finish(result.clone());
        result.map_err(ActionStop::Error)
    }

    fn tool_context<'a>(&'a self, attempt: &'a StageAttempt, call_id: &'a str) -> ToolContext<'a> {
        ToolContext {
            run_id: &self.state.run_id,
            workflow: &self.workflow.name,
            attempt,
            call_id,
            metadata: &self.state.input.metadata,
            cancellation: self.cancellation.clone(),
        }
    }

    fn consume_llm_call(&mut self) -> std::result::Result<(), ActionStop> {
        if self.state.usage.llm_calls >= self.workflow.limits.max_llm_calls {
            return Err(ActionStop::Limit(LimitKind::LlmCalls));
        }
        self.state.usage.llm_calls += 1;
        Ok(())
    }

    fn consume_tool_call(&mut self) -> std::result::Result<(), ActionStop> {
        if self.state.usage.tool_calls >= self.workflow.limits.max_tool_calls {
            return Err(ActionStop::Limit(LimitKind::ToolCalls));
        }
        self.state.usage.tool_calls += 1;
        Ok(())
    }

    fn finish_result(&self) -> RunResult {
        let mut latest_outputs = HashMap::new();
        for record in &self.state.history {
            if record.succeeded() {
                if let Some(output) = &record.output {
                    latest_outputs.insert(record.stage.clone(), output.clone());
                }
            }
        }
        RunResult {
            run_id: self.state.run_id.clone(),
            workflow: self.workflow.name.clone(),
            latest_outputs,
            history: self.state.history.clone(),
            state: self.state.state.clone(),
            usage: self.state.usage,
            duration: self.state.started.elapsed(),
        }
    }
}

fn merge_tool_call(calls: &mut Vec<ToolCall>, incoming: ToolCall) {
    if let Some(existing) = calls
        .iter_mut()
        .find(|call| !incoming.id.is_empty() && call.id == incoming.id)
    {
        *existing = incoming;
    } else {
        calls.push(incoming);
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
