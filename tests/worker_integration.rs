//! Integration test: kernel actor ↔ agent task round-trip.
//!
//! Verifies the full pipeline flow: spawn kernel → init session → get
//! instruction → report result → terminate.

use jeeves_core::kernel::Kernel;
use jeeves_core::kernel::orchestrator_types::PipelineConfig;
use jeeves_core::types::ProcessId;
use jeeves_core::worker::actor::spawn_kernel;
use jeeves_core::worker::agent::{AgentRegistry, DeterministicAgent};
use jeeves_core::worker::run_pipeline;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

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

#[tokio::test]
async fn test_kernel_actor_pipeline_round_trip() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    // Register deterministic agents (no LLM calls needed)
    let mut agents = AgentRegistry::new();
    agents.register("understand", Arc::new(DeterministicAgent));
    agents.register("respond", Arc::new(DeterministicAgent));

    let pid = ProcessId::must("test-p1");
    let result = run_pipeline(
        &handle,
        pid,
        two_stage_pipeline(),
        "hello world",
        "test-user",
        "test-session",
        &agents,
    )
    .await
    .expect("pipeline should complete");

    assert!(result.terminated);
    assert_eq!(
        result.terminal_reason,
        Some(jeeves_core::envelope::TerminalReason::Completed)
    );

    cancel.cancel();
}

#[tokio::test]
async fn test_kernel_actor_session_state() {
    let kernel = Kernel::new();
    let cancel = CancellationToken::new();
    let handle = spawn_kernel(kernel, cancel.clone());

    let pid = ProcessId::must("state-test");
    let envelope = jeeves_core::envelope::Envelope::new_minimal(
        "user1",
        "sess1",
        "test input",
        None,
    );

    let session = handle
        .initialize_session(pid.clone(), two_stage_pipeline(), envelope, false)
        .await
        .expect("init should succeed");

    assert_eq!(session.process_id.as_str(), "state-test");
    assert_eq!(session.stage_order.len(), 2);
    assert!(!session.terminated);

    // Query session state
    let state = handle
        .get_session_state(&pid)
        .await
        .expect("state query should succeed");

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
    let envelope = jeeves_core::envelope::Envelope::new_minimal(
        "user1",
        "sess1",
        "hello",
        None,
    );

    // Init session (auto-creates PCB)
    handle
        .initialize_session(pid.clone(), two_stage_pipeline(), envelope, false)
        .await
        .expect("init should succeed");

    // Terminate
    handle
        .terminate_process(&pid)
        .await
        .expect("terminate should succeed");

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
        &handle,
        ProcessId::must("three-stage"),
        config,
        "input",
        "user",
        "sess",
        &agents,
    )
    .await
    .expect("three-stage pipeline should complete");

    assert!(result.terminated);
    assert_eq!(
        result.terminal_reason,
        Some(jeeves_core::envelope::TerminalReason::Completed)
    );

    cancel.cancel();
}
