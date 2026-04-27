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
                arguments: serde_json::from_str(args).unwrap_or(serde_json::json!({})),
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
    async fn execute(&self, name: &str, params: serde_json::Value) -> jeeves_core::types::Result<jeeves_core::worker::tools::ToolOutput> {
        Ok(jeeves_core::worker::tools::ToolOutput::json(serde_json::json!({"tool": name, "params": params})))
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
    async fn execute(&self, name: &str, _params: serde_json::Value) -> jeeves_core::types::Result<jeeves_core::worker::tools::ToolOutput> {
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
        content_resolver: None,
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

    let _state = handle
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
    let _state = handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

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
    let _state = handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let _ = run_pipeline_loop(&handle, &pid, &agents, Some(tx), "test").await.unwrap();

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
    let _state = handle.initialize_session(pid.clone(), config, envelope, false).await.unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let _ = run_pipeline_loop(&handle, &pid, &agents, Some(tx), "test").await.unwrap();

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
    use jeeves_core::kernel::orchestrator::{RoutingResult, RoutingContext};

    let mut kernel = Kernel::new();
    kernel.register_routing_fn("fan_ab", Arc::new(|_: &RoutingContext<'_>| {
        RoutingResult::Fan(vec!["branch_a".into(), "branch_b".into()])
    }));
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Fork node: routing_fn fans out to branch_a + branch_b, then join into "final"
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
                "routing_fn": "fan_ab",
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
    use jeeves_core::kernel::orchestrator::{RoutingResult, RoutingContext};

    let mut kernel = Kernel::new();
    kernel.register_routing_fn("fan_ab", Arc::new(|_: &RoutingContext<'_>| {
        RoutingResult::Fan(vec!["a".into(), "b".into()])
    }));
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
                "routing_fn": "fan_ab",
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
// H. Tool Confirmation Gate (interrupt flow)
// =============================================================================

/// Tool executor that requires confirmation for "dangerous_op" but not "safe_op".
#[derive(Debug)]
struct ConfirmableToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for ConfirmableToolExecutor {
    async fn execute(&self, name: &str, _params: serde_json::Value) -> jeeves_core::types::Result<jeeves_core::worker::tools::ToolOutput> {
        Ok(jeeves_core::worker::tools::ToolOutput::json(serde_json::json!({"tool": name, "executed": true})))
    }
    fn list_tools(&self) -> Vec<jeeves_core::worker::tools::ToolInfo> {
        vec![
            ToolInfo { name: "safe_op".into(), description: "safe".into(), parameters: serde_json::json!({"type": "object"}) },
            ToolInfo { name: "dangerous_op".into(), description: "destructive".into(), parameters: serde_json::json!({"type": "object"}) },
        ]
    }
    fn requires_confirmation(&self, name: &str, _params: &serde_json::Value) -> Option<jeeves_core::worker::tools::ConfirmationRequest> {
        if name == "dangerous_op" {
            Some(jeeves_core::worker::tools::ConfirmationRequest {
                message: "This is destructive!".into(),
                action_data: Some(serde_json::json!({"target": "all"})),
            })
        } else {
            None
        }
    }
}

/// McpDelegatingAgent with confirmable tool → first run returns interrupt, resolve → re-run executes.
#[tokio::test]
async fn test_confirmation_gate_mcp_agent_buffered() {
    use jeeves_core::worker::agent::{McpDelegatingAgent, Agent, AgentContext};
    use std::collections::HashMap;

    let mut tool_reg = ToolRegistry::new();
    let executor = Arc::new(ConfirmableToolExecutor);
    for t in executor.list_tools() {
        tool_reg.register(t.name.clone(), executor.clone());
    }
    let tools = Arc::new(tool_reg);

    let agent = McpDelegatingAgent {
        tool_name: "dangerous_op".to_string(),
        tools: tools.clone(),
    };

    // First call: no interrupt_response → should return interrupt_request
    let ctx = AgentContext {
        raw_input: "do dangerous thing".into(),
        outputs: HashMap::new(),
        state: HashMap::new(),
        metadata: HashMap::new(),
        event_tx: None,
        stage_name: Some("execute".into()),
        pipeline_name: Arc::from("test"),
        max_context_tokens: None,
        context_overflow: None,
        interrupt_response: None,
    };

    let output = agent.process(&ctx).await.unwrap();
    assert!(output.interrupt_request.is_some(), "Should request confirmation");
    assert!(output.success, "Interrupt request is not a failure");
    let interrupt = output.interrupt_request.unwrap();
    assert_eq!(interrupt.kind, jeeves_core::envelope::InterruptKind::Confirmation);
    assert!(interrupt.message.as_ref().unwrap().contains("destructive"));

    // Second call: with interrupt_response → should skip confirmation and execute
    let ctx_resumed = AgentContext {
        interrupt_response: Some(serde_json::json!({"approved": true})),
        ..ctx
    };

    let output2 = agent.process(&ctx_resumed).await.unwrap();
    assert!(output2.interrupt_request.is_none(), "Should NOT request confirmation on resume");
    assert!(output2.success);
    assert_eq!(output2.output["tool"], "dangerous_op");
    assert_eq!(output2.output["executed"], true);
}

/// McpDelegatingAgent with safe tool → no interrupt, executes directly.
#[tokio::test]
async fn test_confirmation_gate_safe_tool_no_interrupt() {
    use jeeves_core::worker::agent::{McpDelegatingAgent, Agent, AgentContext};
    use std::collections::HashMap;

    let mut tool_reg = ToolRegistry::new();
    let executor = Arc::new(ConfirmableToolExecutor);
    for t in executor.list_tools() {
        tool_reg.register(t.name.clone(), executor.clone());
    }

    let agent = McpDelegatingAgent {
        tool_name: "safe_op".to_string(),
        tools: Arc::new(tool_reg),
    };

    let ctx = AgentContext {
        raw_input: "do safe thing".into(),
        outputs: HashMap::new(),
        state: HashMap::new(),
        metadata: HashMap::new(),
        event_tx: None,
        stage_name: Some("execute".into()),
        pipeline_name: Arc::from("test"),
        max_context_tokens: None,
        context_overflow: None,
        interrupt_response: None,
    };

    let output = agent.process(&ctx).await.unwrap();
    assert!(output.interrupt_request.is_none(), "Safe tool should not need confirmation");
    assert!(output.success);
    assert_eq!(output.output["tool"], "safe_op");
    assert_eq!(output.output["executed"], true);
}

/// Full pipeline: stage with confirmable tool → WaitInterrupt (buffered) → resolve → re-run → complete.
#[tokio::test]
async fn test_confirmation_gate_full_pipeline_buffered() {
    use jeeves_core::envelope::Envelope;
    use jeeves_core::worker::agent::McpDelegatingAgent;

    let cancel = CancellationToken::new();
    let handle = spawn_kernel(Kernel::new(), cancel.clone());

    let pipeline: PipelineConfig = serde_json::from_value(serde_json::json!({
        "name": "confirm_test",
        "stages": [
            {
                "name": "execute",
                "agent": "execute",
                "has_llm": false,
                "allowed_tools": ["dangerous_op"]
            }
        ],
        "max_iterations": 5,
        "max_llm_calls": 5,
        "max_agent_hops": 5
    })).unwrap();

    let mut tool_reg = ToolRegistry::new();
    let executor = Arc::new(ConfirmableToolExecutor);
    for t in executor.list_tools() {
        tool_reg.register(t.name.clone(), executor.clone());
    }

    let mut agents = AgentRegistry::new();
    agents.register("execute", Arc::new(McpDelegatingAgent {
        tool_name: "dangerous_op".to_string(),
        tools: Arc::new(tool_reg),
    }));

    let pid = ProcessId::new();
    let envelope = Envelope::new_minimal("user-1", "sess-1", "delete everything", None);

    // Initialize session
    let _state = handle.initialize_session(pid.clone(), pipeline.clone(), envelope, false).await.unwrap();

    // Run pipeline loop in buffered mode — should return incomplete (WaitInterrupt)
    let result = run_pipeline_loop(&handle, &pid, &agents, None, "confirm_test").await.unwrap();
    assert!(!result.terminated(), "Pipeline should NOT terminate — waiting for interrupt");

    // Get next instruction — should be WaitInterrupt
    let instr = handle.get_next_instruction(&pid).await.unwrap();
    assert!(matches!(instr, jeeves_core::kernel::orchestrator_types::Instruction::WaitInterrupt { .. }));

    // Resolve the interrupt
    let interrupt_id = if let jeeves_core::kernel::orchestrator_types::Instruction::WaitInterrupt { ref interrupt } = instr {
        interrupt.as_ref().unwrap().id.clone()
    } else {
        panic!("Expected WaitInterrupt");
    };
    handle.resolve_interrupt(&pid, &interrupt_id, jeeves_core::envelope::InterruptResponse {
        text: None,
        approved: Some(true),
        decision: None,
        data: None,
        received_at: chrono::Utc::now(),
    }).await.unwrap();

    // Re-run pipeline loop — should now execute the tool and complete
    let result2 = run_pipeline_loop(&handle, &pid, &agents, None, "confirm_test").await.unwrap();
    assert!(result2.terminated(), "Pipeline should complete after interrupt resolved");
    assert_eq!(result2.terminal_reason(), Some(TerminalReason::Completed));

    // Verify the tool executed
    let execute_output = result2.outputs.get("execute").expect("execute output should exist");
    assert_eq!(execute_output.get("tool").and_then(|v| v.as_str()), Some("dangerous_op"));
    assert_eq!(execute_output.get("executed").and_then(|v| v.as_bool()), Some(true));

    cancel.cancel();
}
