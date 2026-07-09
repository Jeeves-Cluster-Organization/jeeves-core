#![allow(clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use jeeves_core::llm::ToolCall;
use jeeves_core::prelude::*;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn engine(workflow: Workflow) -> Engine {
    Engine::builder()
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build")
}

fn completed(outcome: &RunOutcome) -> &RunResult {
    match outcome {
        RunOutcome::Completed(result) => result,
        other => panic!("expected completed outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn deterministic_pipeline_routes_and_keeps_stage_history() {
    let workflow = Workflow::builder("linear")
        .stage(Stage::deterministic_fn("first", |_| Ok(json!({"value": 4}))).next("second"))
        .stage(Stage::deterministic_fn("second", |run| {
            let value = run
                .latest_output("first")
                .and_then(|value| value.get("value"))
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::invalid_input("first.value missing"))?;
            Ok(json!({"value": value * 2}))
        }))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow)
        .run("linear", RunInput::text("hello"))
        .await
        .expect("run result");
    let result = completed(&outcome);
    assert_eq!(result.latest_outputs["second"], json!({"value": 8}));
    assert_eq!(result.history.len(), 2);
    assert_eq!(result.history[0].stage.as_ref(), "first");
    assert_eq!(result.history[1].stage.as_ref(), "second");
}

#[tokio::test]
async fn exact_stage_and_llm_limits_allow_normal_completion() {
    let llm = Arc::new(MockLlmProvider::text("finished"));
    let workflow = Workflow::builder("exact")
        .limits(RunLimits {
            max_stage_executions: 1,
            max_llm_calls: 1,
            max_tool_calls: 1,
            deadline: None,
        })
        .stage(Stage::llm("only", LlmAction::text(Prompt::text("respond"))))
        .build()
        .expect("valid workflow");
    let engine = Engine::builder()
        .llm(llm)
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build");

    let outcome = engine.run("exact", "input").await.expect("run result");
    let result = completed(&outcome);
    assert_eq!(result.latest_outputs["only"], json!("finished"));
    assert_eq!(result.usage.llm_calls, 1);
}

#[tokio::test]
async fn retry_any_failure_is_explicit_and_preserves_failed_attempts() {
    let calls = Arc::new(AtomicU32::new(0));
    let calls_for_action = calls.clone();
    let workflow = Workflow::builder("retry")
        .stage(
            Stage::deterministic_fn("flaky", move |_| {
                let call = calls_for_action.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    Err(Error::invalid_input("bad first result"))
                } else {
                    Ok(json!({"ok": true}))
                }
            })
            .retry(RetryPolicy::exponential(2, Duration::ZERO).retry_on_any_failure()),
        )
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow).run("retry", "input").await.expect("run");
    let result = completed(&outcome);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(result.history.len(), 2);
    assert!(!result.history[0].failures.is_empty());
    assert!(result.history[1].succeeded());
}

#[tokio::test]
async fn exhausted_failure_routes_to_recovery_and_can_complete() {
    let workflow = Workflow::builder("recover")
        .stage(
            Stage::deterministic_fn("work", |_| Err(Error::permanent("failed")))
                .on_error("recover"),
        )
        .stage(Stage::deterministic_fn("recover", |_| {
            Ok(json!({"recovered": true}))
        }))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow).run("recover", "input").await.expect("run");
    let result = completed(&outcome);
    assert_eq!(result.latest_outputs["recover"], json!({"recovered": true}));
    assert_eq!(result.history.len(), 2);
    assert!(!result.history[0].failures.is_empty());
}

#[tokio::test]
async fn unrecovered_failure_is_not_completed() {
    let workflow = Workflow::builder("fail")
        .stage(Stage::deterministic_fn("work", |_| {
            Err(Error::permanent("failed"))
        }))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow).run("fail", "input").await.expect("run");
    assert!(matches!(outcome.as_ref(), RunOutcome::Failed { .. }));
}

#[tokio::test]
async fn reducer_derives_shared_state_from_attempt_history() {
    let workflow = Workflow::builder("state")
        .state(
            json!({"successes": 0}),
            |state: &Value, record: &StageRecord| {
                let mut next = state.clone();
                if record.succeeded() {
                    let count = next["successes"].as_u64().unwrap_or_default();
                    next["successes"] = json!(count + 1);
                }
                Ok(next)
            },
        )
        .stage(Stage::deterministic_fn("one", |_| Ok(json!(1))).next("two"))
        .stage(Stage::deterministic_fn("two", |_| Ok(json!(2))))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow).run("state", "input").await.expect("run");
    assert_eq!(completed(&outcome).state, json!({"successes": 2}));
}

#[tokio::test]
async fn dynamic_routing_reads_latest_stage_output() {
    let workflow = Workflow::builder("route")
        .stage(
            Stage::deterministic_fn("choose", |_| Ok(json!({"target": "right"}))).route(
                |run: &RunView<'_>| {
                    let target = run
                        .latest_output("choose")
                        .and_then(|value| value.get("target"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| Error::routing("target missing"))?;
                    Ok(Route::next(target))
                },
            ),
        )
        .stage(Stage::deterministic_fn("left", |_| Ok(json!("left"))))
        .stage(Stage::deterministic_fn("right", |_| Ok(json!("right"))))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow).run("route", "input").await.expect("run");
    let result = completed(&outcome);
    assert!(result.latest_outputs.contains_key("right"));
    assert!(!result.latest_outputs.contains_key("left"));
}

#[tokio::test]
async fn text_output_streams_deltas_and_finishes_with_same_result() {
    let llm = Arc::new(MockLlmProvider::new([vec![
        ModelStreamEvent::Text("hello ".into()),
        ModelStreamEvent::Text("world".into()),
    ]]));
    let workflow = Workflow::builder("stream")
        .stage(Stage::llm("speak", LlmAction::text(Prompt::text("speak"))))
        .build()
        .expect("valid workflow");
    let engine = Engine::builder()
        .llm(llm)
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build");

    let mut handle = engine.start("stream", "input").expect("start");
    let mut events = handle.take_events().expect("event stream");
    let outcome = handle.result().await.expect("result");
    assert_eq!(
        completed(&outcome).latest_outputs["speak"],
        json!("hello world")
    );

    let mut deltas = String::new();
    let mut finished = None;
    while let Some(event) = events.recv().await {
        match event {
            RunEvent::TextDelta { content, .. } => deltas.push_str(&content),
            RunEvent::Finished(event_outcome) => {
                finished = Some(event_outcome);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(deltas, "hello world");
    assert!(Arc::ptr_eq(&outcome, &finished.expect("finished event")));
}

#[tokio::test]
async fn structured_output_is_collected_validated_and_retried() {
    let llm = Arc::new(MockLlmProvider::new([
        vec![ModelStreamEvent::Text("not-json".into())],
        vec![
            ModelStreamEvent::Text("{\"answer\":".into()),
            ModelStreamEvent::Text("42}".into()),
        ],
    ]));
    let action =
        LlmAction::structured(Prompt::text("answer"), json!({"type": "object"}), |value| {
            value
                .get("answer")
                .and_then(Value::as_i64)
                .map(|_| ())
                .ok_or_else(|| Error::invalid_input("answer must be an integer"))
        });
    let workflow = Workflow::builder("structured")
        .stage(
            Stage::llm("answer", action)
                .retry(RetryPolicy::exponential(2, Duration::ZERO).retry_on_any_failure()),
        )
        .build()
        .expect("valid workflow");
    let engine = Engine::builder()
        .llm(llm)
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build");

    let mut handle = engine.start("structured", "input").expect("start");
    let mut events = handle.take_events().expect("events");
    let outcome = handle.result().await.expect("run");
    let result = completed(&outcome);
    assert_eq!(result.latest_outputs["answer"], json!({"answer": 42}));
    assert_eq!(result.history.len(), 2);
    assert_eq!(result.usage.llm_calls, 2);
    while let Some(event) = events.recv().await {
        assert!(!matches!(event, RunEvent::TextDelta { .. }));
    }
}

struct EchoTool {
    calls: AtomicU32,
}

#[async_trait]
impl Tool for EchoTool {
    async fn call(&self, _context: &ToolContext<'_>, arguments: Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(arguments)
    }
}

fn echo_tool() -> (Arc<EchoTool>, Arc<ToolDefinition>) {
    let handler = Arc::new(EchoTool {
        calls: AtomicU32::new(0),
    });
    let spec =
        ToolSpec::new("echo", "echo arguments", json!({"type": "object"})).expect("tool spec");
    (
        handler.clone(),
        Arc::new(ToolDefinition::new(spec, handler)),
    )
}

#[tokio::test]
async fn direct_tool_arguments_can_be_derived_from_run_history() {
    let (tool, definition) = echo_tool();
    let workflow = Workflow::builder("direct-tool")
        .limits(RunLimits {
            max_stage_executions: 2,
            max_llm_calls: 0,
            max_tool_calls: 1,
            deadline: None,
        })
        .stage(Stage::deterministic_fn("prepare", |_| Ok(json!({"x": 7}))).next("echo"))
        .stage(Stage::tool(
            "echo",
            ToolAction::new(definition).arguments(|run| {
                run.latest_output("prepare")
                    .cloned()
                    .ok_or_else(|| Error::invalid_input("prepare output missing"))
            }),
        ))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow)
        .run("direct-tool", "ignored")
        .await
        .expect("run");
    assert_eq!(completed(&outcome).latest_outputs["echo"], json!({"x": 7}));
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn zero_tool_budget_prevents_tool_execution() {
    let (tool, definition) = echo_tool();
    let workflow = Workflow::builder("bounded-tool")
        .limits(RunLimits {
            max_stage_executions: 1,
            max_llm_calls: 0,
            max_tool_calls: 0,
            deadline: None,
        })
        .stage(Stage::tool("echo", ToolAction::new(definition)))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow)
        .run("bounded-tool", json!({"x": 1}))
        .await
        .expect("run");
    assert!(matches!(
        outcome.as_ref(),
        RunOutcome::LimitExceeded {
            limit: LimitKind::ToolCalls,
            ..
        }
    ));
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
}

struct ApprovalTool {
    calls: AtomicU32,
}

#[async_trait]
impl Tool for ApprovalTool {
    async fn call(&self, _context: &ToolContext<'_>, arguments: Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"executed": arguments}))
    }

    async fn approval(
        &self,
        _context: &ToolContext<'_>,
        _arguments: &Value,
    ) -> Result<Option<ApprovalPrompt>> {
        Ok(Some(ApprovalPrompt::new("approve test tool")))
    }
}

fn approval_tool() -> (Arc<ApprovalTool>, Arc<ToolDefinition>) {
    let handler = Arc::new(ApprovalTool {
        calls: AtomicU32::new(0),
    });
    let spec = ToolSpec::new("test_tool", "test", json!({"type": "object"})).expect("tool spec");
    (
        handler.clone(),
        Arc::new(ToolDefinition::new(spec, handler)),
    )
}

#[tokio::test]
async fn approval_pauses_in_place_and_resumes_the_same_llm_loop() {
    let (tool, definition) = approval_tool();
    let llm = Arc::new(MockLlmProvider::new([
        vec![ModelStreamEvent::ToolCall(ToolCall {
            id: "call-1".into(),
            name: "test_tool".into(),
            arguments: json!({"x": 1}),
        })],
        vec![ModelStreamEvent::Text("done".into())],
    ]));
    let workflow = Workflow::builder("approval")
        .stage(Stage::llm(
            "act",
            LlmAction::text(Prompt::text("act")).with_tools([definition]),
        ))
        .build()
        .expect("valid workflow");
    let engine = Engine::builder()
        .llm(llm)
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build");

    let mut handle = engine.start("approval", "input").expect("start");
    let mut events = handle.take_events().expect("events");
    let request = loop {
        if let RunEvent::ApprovalRequested(request) = events.recv().await.expect("approval event") {
            break request;
        }
    };
    handle
        .respond_to_approval(ApprovalResponse::approve(request.request_id))
        .expect("approval response");
    let outcome = handle.result().await.expect("result");
    assert_eq!(completed(&outcome).latest_outputs["act"], json!("done"));
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    assert_eq!(completed(&outcome).usage.llm_calls, 2);
}

#[tokio::test]
async fn denied_llm_tool_returns_to_model_without_executing() {
    let (tool, definition) = approval_tool();
    let llm = Arc::new(MockLlmProvider::new([
        vec![ModelStreamEvent::ToolCall(ToolCall {
            id: "call-1".into(),
            name: "test_tool".into(),
            arguments: json!({"x": 1}),
        })],
        vec![ModelStreamEvent::Text("alternative".into())],
    ]));
    let workflow = Workflow::builder("denied")
        .stage(Stage::llm(
            "act",
            LlmAction::text(Prompt::text("act")).with_tools([definition]),
        ))
        .build()
        .expect("valid workflow");
    let engine = Engine::builder()
        .llm(llm)
        .workflow(workflow)
        .expect("workflow registration")
        .build()
        .expect("engine build");

    let mut handle = engine.start("denied", "input").expect("start");
    let mut events = handle.take_events().expect("events");
    let request = loop {
        if let RunEvent::ApprovalRequested(request) = events.recv().await.expect("approval event") {
            break request;
        }
    };
    handle
        .respond_to_approval(ApprovalResponse::deny(request.request_id))
        .expect("denial response");
    let outcome = handle.result().await.expect("result");
    assert_eq!(
        completed(&outcome).latest_outputs["act"],
        json!("alternative")
    );
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn approval_without_an_event_consumer_fails_instead_of_waiting_forever() {
    let (tool, definition) = approval_tool();
    let workflow = Workflow::builder("unobserved-approval")
        .stage(Stage::tool("act", ToolAction::new(definition)))
        .build()
        .expect("valid workflow");

    let outcome = engine(workflow)
        .run("unobserved-approval", json!({"x": 1}))
        .await
        .expect("run");
    assert!(matches!(
        outcome.as_ref(),
        RunOutcome::Failed { error, .. }
            if error.kind() == ErrorKind::Configuration
    ));
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
}

struct PendingAction;

#[async_trait]
impl DeterministicAction for PendingAction {
    async fn execute(&self, _run: &RunView<'_>) -> Result<Value> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn explicit_cancellation_finishes_as_cancelled() {
    let workflow = Workflow::builder("cancel")
        .stage(Stage::deterministic("wait", Arc::new(PendingAction)))
        .build()
        .expect("valid workflow");
    let engine = engine(workflow);
    let handle = engine.start("cancel", "input").expect("start");
    handle.cancel();
    let outcome = handle.result().await.expect("result");
    assert!(matches!(outcome.as_ref(), RunOutcome::Cancelled(_)));
}
