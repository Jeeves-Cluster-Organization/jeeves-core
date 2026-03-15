//! Integration test: kernel actor ↔ agent task round-trip.
//!
//! Verifies the full pipeline flow: spawn kernel → init session → get
//! instruction → report result → terminate.
//!
//! Test groups:
//! A. Tool execution loop (SequentialMockLlmProvider + tool dispatch)
//! B. Streaming (PipelineEvent emission)
//! C. Pipeline topologies (Gate, Fork, linear)
//! D. Routing & bounds (max_visits, error_next, conditional)
//! E. Error handling (LLM failure, tool failure)
//! F. Validation (definition-time rejection)

use jeeves_core::envelope::TerminalReason;
use jeeves_core::kernel::Kernel;
use jeeves_core::kernel::orchestrator_types::PipelineConfig;
use jeeves_core::types::ProcessId;
use jeeves_core::worker::actor::spawn_kernel;
use jeeves_core::worker::agent::{AgentRegistry, DeterministicAgent, LlmAgent};
use jeeves_core::worker::llm::mock::SequentialMockLlmProvider;
use jeeves_core::worker::llm::{ChatResponse, PipelineEvent, TokenUsage, ToolCall};
use jeeves_core::worker::prompts::PromptRegistry;
use jeeves_core::worker::tools::{ToolExecutor, ToolInfo, ToolRegistry};
use jeeves_core::worker::{run_pipeline, run_pipeline_loop};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// =============================================================================
// Test helpers
// =============================================================================

fn two_stage_pipeline() -> PipelineConfig {
    serde_json::from_value(serde_json::json!({
        "name": "test_pipeline",
        "stages": [
            {
                "name": "understand",
                "agent": "understand",
                "default_next": "respond",
                "has_llm": false
            },
            {
                "name": "respond",
                "agent": "respond",
                "has_llm": false
            }
        ],
        "max_iterations": 10,
        "max_llm_calls": 5,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .expect("valid pipeline config")
}

fn make_text_response(content: &str) -> ChatResponse {
    ChatResponse {
        content: Some(content.to_string()),
        tool_calls: vec![],
        usage: TokenUsage { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
        model: "mock".to_string(),
    }
}

fn make_tool_call_response(calls: Vec<(&str, &str, &str)>) -> ChatResponse {
    ChatResponse {
        content: None,
        tool_calls: calls
            .into_iter()
            .map(|(id, name, args)| ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: args.to_string(),
            })
            .collect(),
        usage: TokenUsage { prompt_tokens: 15, completion_tokens: 10, total_tokens: 25 },
        model: "mock".to_string(),
    }
}

/// Simple tool executor that echoes the tool name and params.
#[derive(Debug)]
struct EchoToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for EchoToolExecutor {
    async fn execute(&self, name: &str, params: serde_json::Value) -> jeeves_core::types::Result<serde_json::Value> {
        Ok(serde_json::json!({"tool": name, "params": params}))
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![
            ToolInfo {
                name: "search".to_string(),
                description: "Search for something".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {"q": {"type": "string"}}}),
            },
            ToolInfo {
                name: "execute".to_string(),
                description: "Execute a command".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {"cmd": {"type": "string"}}}),
            },
        ]
    }
}

/// Tool executor that always fails.
#[derive(Debug)]
struct FailingToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for FailingToolExecutor {
    async fn execute(&self, name: &str, _params: serde_json::Value) -> jeeves_core::types::Result<serde_json::Value> {
        Err(jeeves_core::types::Error::internal(format!("Tool '{}' failed", name)))
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![ToolInfo {
            name: "broken".to_string(),
            description: "A broken tool".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }]
    }
}

fn make_llm_agent(
    llm: Arc<dyn jeeves_core::worker::llm::LlmProvider>,
    tools: Arc<ToolRegistry>,
) -> LlmAgent {
    LlmAgent {
        llm,
        prompts: Arc::new(PromptRegistry::empty()),
        tools,
        prompt_key: "test".to_string(),
        temperature: None,
        max_tokens: None,
        model: None,
        max_tool_rounds: 10,
    }
}

// =============================================================================
// Existing tests (original 5)
// =============================================================================

#[tokio::test]
async fn test_kernel_actor_pipeline_round_trip() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let mut agents = AgentRegistry::new();
    agents.register("understand", Arc::new(DeterministicAgent));
    agents.register("respond", Arc::new(DeterministicAgent));

    let pid = ProcessId::must("test-p1");
    let result = run_pipeline(
        &handle, pid, two_stage_pipeline(), "hello world", "test-user", "test-session", &agents,
    )
    .await
    .expect("pipeline should complete");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

#[tokio::test]
async fn test_kernel_actor_session_state() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let pid = ProcessId::must("state-test");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user1", "sess1", "test input", None);

    let session = handle
        .initialize_session(pid.clone(), two_stage_pipeline(), envelope, false)
        .await
        .expect("init should succeed");

    assert_eq!(session.process_id.as_str(), "state-test");
    assert_eq!(session.stage_order.len(), 2);
    assert!(!session.terminated);

    let state = handle.get_session_state(&pid).await.expect("state query should succeed");
    assert_eq!(state.process_id.as_str(), "state-test");
    cancel.cancel();
}

#[tokio::test]
async fn test_kernel_actor_system_status() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let status = handle.get_system_status().await;
    assert_eq!(status.processes_total, 0);
    assert_eq!(status.active_orchestration_sessions, 0);
    cancel.cancel();
}

#[tokio::test]
async fn test_kernel_actor_terminate_process() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let pid = ProcessId::must("term-test");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user1", "sess1", "hello", None);

    handle
        .initialize_session(pid.clone(), two_stage_pipeline(), envelope, false)
        .await
        .expect("init should succeed");

    handle.terminate_process(&pid).await.expect("terminate should succeed");
    cancel.cancel();
}

#[tokio::test]
async fn test_pipeline_with_three_stages() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "three_stage",
        "stages": [
            {"name": "a", "agent": "a", "default_next": "b", "has_llm": false},
            {"name": "b", "agent": "b", "default_next": "c", "has_llm": false},
            {"name": "c", "agent": "c", "has_llm": false}
        ],
        "max_iterations": 20,
        "max_llm_calls": 10,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    let mut agents = AgentRegistry::new();
    agents.register("a", Arc::new(DeterministicAgent));
    agents.register("b", Arc::new(DeterministicAgent));
    agents.register("c", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle, ProcessId::must("three-stage"), config, "input", "user", "sess", &agents,
    )
    .await
    .expect("three-stage pipeline should complete");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

// =============================================================================
// Group A: Tool Execution Loop
// =============================================================================

#[tokio::test]
async fn test_single_tool_call_round_trip() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // LLM first returns a tool call, then returns final text
    let llm = Arc::new(SequentialMockLlmProvider::new(vec![
        make_tool_call_response(vec![("call_1", "search", r#"{"q":"weather"}"#)]),
        make_text_response(r#"{"response":"It's sunny"}"#),
    ]));

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register("echo", Arc::new(EchoToolExecutor));
    let tools = Arc::new(tool_reg);

    let agent = make_llm_agent(llm, tools);

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "tool_test",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let result = run_pipeline(
        &handle, ProcessId::must("tool-1"), config, "what's the weather?", "user", "sess", &agents,
    )
    .await
    .expect("pipeline should complete");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

#[tokio::test]
async fn test_multi_tool_sequential() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // LLM returns 2 tool calls in one response, then final text
    let llm = Arc::new(SequentialMockLlmProvider::new(vec![
        make_tool_call_response(vec![
            ("call_1", "search", r#"{"q":"weather"}"#),
            ("call_2", "search", r#"{"q":"news"}"#),
        ]),
        make_text_response(r#"{"response":"Weather and news"}"#),
    ]));

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register("echo", Arc::new(EchoToolExecutor));

    let agent = make_llm_agent(llm, Arc::new(tool_reg));

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "multi_tool",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let result = run_pipeline(
        &handle, ProcessId::must("multi-tool"), config, "weather and news", "user", "sess", &agents,
    )
    .await
    .expect("pipeline should complete");

    assert!(result.terminated());
    cancel.cancel();
}

#[tokio::test]
async fn test_tool_loop_max_rounds() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // LLM always returns tool calls — should stop after max_tool_rounds
    let responses: Vec<ChatResponse> = (0..15)
        .map(|i| make_tool_call_response(vec![(&format!("call_{}", i), "search", r#"{"q":"loop"}"#)]))
        .collect();
    let llm = Arc::new(SequentialMockLlmProvider::new(responses));

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register("echo", Arc::new(EchoToolExecutor));

    let mut agent = make_llm_agent(llm, Arc::new(tool_reg));
    agent.max_tool_rounds = 3;

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "max_rounds",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 20,
        "max_llm_calls": 20,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    let result = run_pipeline(
        &handle, ProcessId::must("max-rounds"), config, "loop forever", "user", "sess", &agents,
    )
    .await
    .expect("pipeline should complete (tool loop capped)");

    assert!(result.terminated());
    cancel.cancel();
}

// =============================================================================
// Group B: Streaming
// =============================================================================

#[tokio::test]
async fn test_streaming_emits_stage_events() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let mut agents = AgentRegistry::new();
    agents.register("a", Arc::new(DeterministicAgent));
    agents.register("b", Arc::new(DeterministicAgent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "stream_test",
        "stages": [
            {"name": "a", "agent": "a", "default_next": "b", "has_llm": false},
            {"name": "b", "agent": "b", "has_llm": false}
        ],
        "max_iterations": 10,
        "max_llm_calls": 5,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let pid = ProcessId::must("stream-1");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user", "sess", "hello", None);
    handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let result = run_pipeline_loop(&handle, &pid, &agents, Some(tx), "test").await.unwrap();

    assert!(result.terminated());

    // Collect all events
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    // Should have: StageStarted(a), StageCompleted(a), StageStarted(b), StageCompleted(b), Done
    let event_types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
    assert!(event_types.contains(&"stage_started"), "expected stage_started event");
    assert!(event_types.contains(&"stage_completed"), "expected stage_completed event");
    assert!(event_types.contains(&"done"), "expected done event");

    // Verify at least 2 stage_started events (one per stage)
    let stage_started_count = event_types.iter().filter(|&&t| t == "stage_started").count();
    assert_eq!(stage_started_count, 2, "expected 2 stage_started events");

    cancel.cancel();
}

#[tokio::test]
async fn test_streaming_emits_tool_events() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let llm = Arc::new(SequentialMockLlmProvider::new(vec![
        make_tool_call_response(vec![("call_1", "search", r#"{"q":"test"}"#)]),
        make_text_response("done"),
    ]));

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register("echo", Arc::new(EchoToolExecutor));

    let agent = make_llm_agent(llm, Arc::new(tool_reg));

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "stream_tool_test",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let pid = ProcessId::must("stream-tool");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user", "sess", "hello", None);
    handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    run_pipeline_loop(&handle, &pid, &agents, Some(tx), "test").await.unwrap();

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    let event_types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
    assert!(event_types.contains(&"tool_call_start"), "expected tool_call_start event");
    assert!(event_types.contains(&"tool_result"), "expected tool_result event");

    cancel.cancel();
}

#[tokio::test]
async fn test_streaming_emits_done() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(DeterministicAgent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "done_test",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": false}],
        "max_iterations": 10,
        "max_llm_calls": 5,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let pid = ProcessId::must("done-test");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user", "sess", "hi", None);
    handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    run_pipeline_loop(&handle, &pid, &agents, Some(tx), "test").await.unwrap();

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    // Last event should be Done
    let last = events.last().expect("should have events");
    assert_eq!(last.event_type(), "done");
    if let PipelineEvent::Done { process_id, terminated, .. } = last {
        assert_eq!(process_id, "done-test");
        assert!(*terminated);
    } else {
        panic!("last event should be Done");
    }

    cancel.cancel();
}

// =============================================================================
// Group C: Pipeline Topologies
// =============================================================================

#[tokio::test]
async fn test_gate_routing() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Gate node with default_next → skips agent execution, routes directly
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "gate_test",
        "stages": [
            {"name": "entry", "agent": "entry", "default_next": "router", "has_llm": false},
            {"name": "router", "agent": "router", "node_kind": "Gate", "default_next": "target", "has_llm": false},
            {"name": "target", "agent": "target", "has_llm": false}
        ],
        "max_iterations": 20,
        "max_llm_calls": 10,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    let mut agents = AgentRegistry::new();
    agents.register("entry", Arc::new(DeterministicAgent));
    agents.register("target", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle, ProcessId::must("gate-1"), config, "hello", "user", "sess", &agents,
    )
    .await
    .expect("gate pipeline should complete");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

#[tokio::test]
async fn test_fork_join_parallel() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Fork node: 2 Always rules → 2 parallel agents, then join into "final"
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "fork_join_test",
        "stages": [
            {
                "name": "entry",
                "agent": "entry",
                "default_next": "splitter",
                "has_llm": false
            },
            {
                "name": "splitter",
                "agent": "splitter",
                "node_kind": "Fork",
                "routing": [
                    {"expr": {"op": "Always"}, "target": "branch_a"},
                    {"expr": {"op": "Always"}, "target": "branch_b"}
                ],
                "default_next": "final",
                "has_llm": false
            },
            {"name": "branch_a", "agent": "branch_a", "has_llm": false},
            {"name": "branch_b", "agent": "branch_b", "has_llm": false},
            {"name": "final", "agent": "final", "has_llm": false}
        ],
        "max_iterations": 30,
        "max_llm_calls": 10,
        "max_agent_hops": 20,
        "edge_limits": []
    }))
    .unwrap();

    let mut agents = AgentRegistry::new();
    agents.register("entry", Arc::new(DeterministicAgent));
    agents.register("branch_a", Arc::new(DeterministicAgent));
    agents.register("branch_b", Arc::new(DeterministicAgent));
    agents.register("final", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle,
        ProcessId::must("fork-join"),
        config,
        "hello",
        "user",
        "sess",
        &agents,
    )
    .await
    .expect("fork/join pipeline should complete without deadlock");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));

    // Pipeline completed without deadlock — fork/join works
    // (output keys depend on agent registration names, not stage names)

    cancel.cancel();
}

// =============================================================================
// Group D: Routing & Bounds
// =============================================================================

#[tokio::test]
async fn test_max_visits_terminates() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Loop stage with max_visits=2 — should terminate after 2 visits
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "max_visits_test",
        "stages": [{
            "name": "looper",
            "agent": "looper",
            "default_next": "looper",
            "max_visits": 2,
            "has_llm": false
        }],
        "max_iterations": 20,
        "max_llm_calls": 10,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    let mut agents = AgentRegistry::new();
    agents.register("looper", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle, ProcessId::must("max-visits"), config, "loop", "user", "sess", &agents,
    )
    .await
    .expect("should terminate via max_visits");

    assert!(result.terminated());
    assert_eq!(
        result.terminal_reason(),
        Some(TerminalReason::MaxStageVisitsExceeded)
    );
    cancel.cancel();
}

#[tokio::test]
async fn test_error_next_routing() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Stage with error_next — on failure, routes to error handler
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "error_next_test",
        "stages": [
            {"name": "worker", "agent": "worker", "error_next": "handler", "has_llm": true},
            {"name": "handler", "agent": "handler", "has_llm": false}
        ],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    // LLM agent that will fail (provider returns error)
    let llm = Arc::new(SequentialMockLlmProvider::new(vec![])); // empty = error on first call

    let agent = make_llm_agent(llm, Arc::new(ToolRegistry::new()));

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));
    agents.register("handler", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle, ProcessId::must("error-next"), config, "hello", "user", "sess", &agents,
    )
    .await
    .expect("should route to error handler");

    assert!(result.terminated());
    // Should complete via the error handler path
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

#[tokio::test]
async fn test_conditional_routing() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Stage with routing rule: if understand.choice == "a" → go to agent_a
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "conditional_test",
        "stages": [
            {
                "name": "understand",
                "agent": "understand",
                "routing": [
                    {
                        "expr": {"op": "Eq", "field": {"scope": "Agent", "agent": "understand", "key": "choice"}, "value": "a"},
                        "target": "agent_a"
                    }
                ],
                "default_next": "agent_b",
                "has_llm": false
            },
            {"name": "agent_a", "agent": "agent_a", "has_llm": false},
            {"name": "agent_b", "agent": "agent_b", "has_llm": false}
        ],
        "max_iterations": 20,
        "max_llm_calls": 10,
        "max_agent_hops": 10,
        "edge_limits": []
    }))
    .unwrap();

    let mut agents = AgentRegistry::new();
    // DeterministicAgent returns {}, so "choice" field won't exist → no match → default_next → agent_b
    agents.register("understand", Arc::new(DeterministicAgent));
    agents.register("agent_a", Arc::new(DeterministicAgent));
    agents.register("agent_b", Arc::new(DeterministicAgent));

    let result = run_pipeline(
        &handle, ProcessId::must("cond-1"), config, "hello", "user", "sess", &agents,
    )
    .await
    .expect("conditional pipeline should complete");

    assert!(result.terminated());
    assert_eq!(result.terminal_reason(), Some(TerminalReason::Completed));
    cancel.cancel();
}

// =============================================================================
// Group E: Error Handling
// =============================================================================

#[tokio::test]
async fn test_llm_failure_propagates() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Empty sequential mock = fails on first call
    let llm = Arc::new(SequentialMockLlmProvider::new(vec![]));

    let agent = make_llm_agent(llm, Arc::new(ToolRegistry::new()));

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "llm_fail_test",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let result = run_pipeline(
        &handle, ProcessId::must("llm-fail"), config, "hello", "user", "sess", &agents,
    )
    .await
    .expect("pipeline should still complete (agent reports failure)");

    // Pipeline terminates because agent reports failure and no error_next
    assert!(result.terminated());
    cancel.cancel();
}

#[tokio::test]
async fn test_tool_execution_failure() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // LLM calls a tool that fails, then gets error message, then returns text
    let llm = Arc::new(SequentialMockLlmProvider::new(vec![
        make_tool_call_response(vec![("call_1", "broken", "{}")]),
        make_text_response("handled the error"),
    ]));

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register("broken_exec", Arc::new(FailingToolExecutor));

    let agent = make_llm_agent(llm, Arc::new(tool_reg));

    let mut agents = AgentRegistry::new();
    agents.register("worker", Arc::new(agent));

    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "tool_fail_test",
        "stages": [{"name": "worker", "agent": "worker", "has_llm": true}],
        "max_iterations": 10,
        "max_llm_calls": 10,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let result = run_pipeline(
        &handle, ProcessId::must("tool-fail"), config, "hello", "user", "sess", &agents,
    )
    .await
    .expect("pipeline should complete (tool error sent back to LLM)");

    assert!(result.terminated());
    cancel.cancel();
}

// =============================================================================
// Group F: Validation
// =============================================================================

#[tokio::test]
async fn test_invalid_pipeline_rejected() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Pipeline with duplicate stage names
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "invalid",
        "stages": [
            {"name": "a", "agent": "a", "has_llm": false},
            {"name": "a", "agent": "a", "has_llm": false}
        ],
        "max_iterations": 10,
        "max_llm_calls": 5,
        "max_agent_hops": 5,
        "edge_limits": []
    }))
    .unwrap();

    let pid = ProcessId::must("invalid-pipeline");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user", "sess", "hello", None);

    let result = handle
        .initialize_session(pid, config, envelope, false)
        .await;

    assert!(result.is_err(), "duplicate stage names should be rejected");
    cancel.cancel();
}

#[tokio::test]
async fn test_valid_complex_pipeline() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Complex pipeline with Gate + Fork
    let config: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "complex",
        "stages": [
            {"name": "entry", "agent": "entry", "default_next": "gate", "has_llm": false},
            {"name": "gate", "agent": "gate", "node_kind": "Gate", "default_next": "fork", "has_llm": false},
            {
                "name": "fork",
                "agent": "fork",
                "node_kind": "Fork",
                "routing": [
                    {"expr": {"op": "Always"}, "target": "a"},
                    {"expr": {"op": "Always"}, "target": "b"}
                ],
                "default_next": "final",
                "has_llm": false
            },
            {"name": "a", "agent": "a", "has_llm": false},
            {"name": "b", "agent": "b", "has_llm": false},
            {"name": "final", "agent": "final", "has_llm": false}
        ],
        "max_iterations": 30,
        "max_llm_calls": 10,
        "max_agent_hops": 20,
        "edge_limits": []
    }))
    .unwrap();

    let pid = ProcessId::must("complex-pipeline");
    let envelope = jeeves_core::envelope::Envelope::new_minimal("user", "sess", "hello", None);

    let result = handle
        .initialize_session(pid, config, envelope, false)
        .await;

    assert!(result.is_ok(), "complex pipeline should pass validation");
    cancel.cancel();
}

// =============================================================================
// Group G: MCP Tool Executor
// =============================================================================

#[tokio::test]
async fn test_mcp_http_tool_executor() {
    use axum::{routing::post, Json, Router};
    use jeeves_core::worker::mcp::{McpToolExecutor, McpTransport};
    use serde_json::json;
    use std::collections::HashMap;

    // Stand up a mock MCP server that handles tools/list and tools/call
    let app = Router::new().route(
        "/",
        post(|Json(req): Json<serde_json::Value>| async move {
            let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
            let id = req.get("id").and_then(|i| i.as_u64()).unwrap_or(0);

            match method {
                "tools/list" => Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "Echoes input back",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "message": {"type": "string"}
                                    }
                                }
                            },
                            {
                                "name": "add",
                                "description": "Adds two numbers",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "a": {"type": "number"},
                                        "b": {"type": "number"}
                                    }
                                }
                            }
                        ]
                    }
                })),
                "tools/call" => {
                    let params = req.get("params").cloned().unwrap_or(json!({}));
                    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args = params.get("arguments").cloned().unwrap_or(json!({}));

                    let result_text = match tool_name {
                        "echo" => {
                            let msg = args.get("message").and_then(|m| m.as_str()).unwrap_or("(empty)");
                            msg.to_string()
                        }
                        "add" => {
                            let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            format!("{}", a + b)
                        }
                        _ => format!("unknown tool: {tool_name}"),
                    };

                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{"type": "text", "text": result_text}],
                            "isError": false
                        }
                    }))
                }
                _ => Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "Method not found"}
                })),
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let url = format!("http://{addr}/");

    // Connect and discover tools
    let executor = McpToolExecutor::connect(McpTransport::Http { url, headers: HashMap::new() })
        .await
        .expect("MCP connect should succeed");

    let tools = executor.list_tools();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "echo");
    assert_eq!(tools[1].name, "add");

    // Execute echo tool
    let result = executor
        .execute("echo", json!({"message": "hello world"}))
        .await
        .expect("echo should succeed");
    assert_eq!(result, json!({"result": "hello world"}));

    // Execute add tool
    let result = executor
        .execute("add", json!({"a": 3, "b": 4}))
        .await
        .expect("add should succeed");
    // "7" is a valid JSON number, so it should parse as such
    assert_eq!(result, json!(7));

    // Register into ToolRegistry and verify
    let mut registry = ToolRegistry::new();
    let executor = Arc::new(executor);
    for tool in executor.list_tools() {
        registry.register(tool.name.clone(), executor.clone());
    }
    let all_tools = registry.list_all_tools();
    assert!(all_tools.iter().any(|t| t.name == "echo"));
    assert!(all_tools.iter().any(|t| t.name == "add"));

    // Execute through registry
    let result = registry
        .execute("echo", json!({"message": "via registry"}))
        .await
        .expect("registry execute should work");
    assert_eq!(result, json!({"result": "via registry"}));
}
