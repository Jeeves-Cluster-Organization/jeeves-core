//! Worker module — embedded agent execution (pipeline loop, LLM, tools).
//!
//! Single process: kernel actor ↔ agent tasks ↔ LLM HTTP calls.

pub mod actor;
pub mod agent;
pub mod handle;
pub mod llm;
pub mod mcp;
#[cfg(feature = "mcp-stdio")]
pub mod mcp_server;
pub mod prompts;
pub mod tools;

use std::sync::Arc;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::{InstructionKind, PipelineConfig};
use crate::types::{ProcessId, Result};

use agent::{Agent, AgentContext, AgentOutput, AgentRegistry, DeterministicAgent};
use handle::KernelHandle;
use llm::PipelineEvent;
use tokio::sync::mpsc;

/// Result of running a pipeline to completion.
#[derive(Debug)]
pub struct WorkerResult {
    pub process_id: ProcessId,
    pub terminated: bool,
    pub terminal_reason: Option<crate::envelope::TerminalReason>,
    pub outputs: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

/// Run a pipeline to completion: init session + loop (buffered, no streaming).
pub async fn run_pipeline(
    handle: &KernelHandle,
    process_id: ProcessId,
    pipeline_config: PipelineConfig,
    raw_input: &str,
    user_id: &str,
    session_id: &str,
    agents: &AgentRegistry,
) -> Result<WorkerResult> {
    let pipeline_name = pipeline_config.name.clone();
    let envelope = Envelope::new_minimal(user_id, session_id, raw_input, None);
    let _session = handle
        .initialize_session(process_id.clone(), pipeline_config, envelope, false)
        .await?;
    run_pipeline_loop(handle, &process_id, agents, None, &pipeline_name).await
}

/// Run a pipeline to completion with a pre-built Envelope (supports metadata).
pub async fn run_pipeline_with_envelope(
    handle: &KernelHandle,
    process_id: ProcessId,
    pipeline_config: PipelineConfig,
    envelope: Envelope,
    agents: &AgentRegistry,
) -> Result<WorkerResult> {
    let pipeline_name = pipeline_config.name.clone();
    let _session = handle
        .initialize_session(process_id.clone(), pipeline_config, envelope, false)
        .await?;
    run_pipeline_loop(handle, &process_id, agents, None, &pipeline_name).await
}

/// Run a pipeline with streaming events. Returns a join handle and event receiver.
/// The receiver yields PipelineEvent items (StageStarted, Delta, ToolCallStart, etc.).
/// Session is initialized before spawning so rate-limit errors surface to the caller.
pub async fn run_pipeline_streaming(
    handle: KernelHandle,
    process_id: ProcessId,
    pipeline_config: PipelineConfig,
    envelope: Envelope,
    agents: Arc<AgentRegistry>,
) -> Result<(
    tokio::task::JoinHandle<Result<WorkerResult>>,
    mpsc::Receiver<PipelineEvent>,
)> {
    let pipeline_name = pipeline_config.name.clone();
    handle
        .initialize_session(process_id.clone(), pipeline_config, envelope, false)
        .await?;
    let (tx, rx) = mpsc::channel(64);
    let jh = tokio::spawn(async move {
        run_pipeline_loop(&handle, &process_id, &agents, Some(tx), &pipeline_name).await
    });
    Ok((jh, rx))
}

/// Run the pipeline loop for an already-initialized session.
/// Pass `event_tx = Some(tx)` for streaming events, `None` for buffered mode.
pub async fn run_pipeline_loop(
    handle: &KernelHandle,
    process_id: &ProcessId,
    agents: &AgentRegistry,
    event_tx: Option<mpsc::Sender<PipelineEvent>>,
    pipeline_name: &str,
) -> Result<WorkerResult> {
    loop {
        let instruction = handle.get_next_instruction(process_id).await?;

        match instruction.kind {
            InstructionKind::Terminate => {
                let outputs = instruction
                    .agent_context
                    .as_ref()
                    .and_then(|c| c.get("outputs"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::Done {
                            process_id: process_id.as_str().to_string(),
                            terminated: true,
                            terminal_reason: instruction
                                .terminal_reason
                                .as_ref()
                                .map(|r| format!("{:?}", r)),
                            outputs: Some(serde_json::to_value(&outputs).unwrap_or_default()),
                            pipeline: pipeline_name.to_string(),
                        })
                        .await;
                }

                return Ok(WorkerResult {
                    process_id: process_id.clone(),
                    terminated: true,
                    terminal_reason: instruction.terminal_reason,
                    outputs,
                });
            }

            InstructionKind::RunAgent => {
                let agent_name = instruction
                    .agents
                    .first()
                    .ok_or_else(|| crate::types::Error::internal("RunAgent with no agent name"))?;

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageStarted {
                            stage: agent_name.clone(),
                            pipeline: pipeline_name.to_string(),
                        })
                        .await;
                }

                let ctx = build_agent_context(&instruction, event_tx.clone(), Some(agent_name.clone()), pipeline_name);
                let output = execute_agent(agents, agent_name, &ctx).await;

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageCompleted {
                            stage: agent_name.clone(),
                            pipeline: pipeline_name.to_string(),
                        })
                        .await;
                }

                handle
                    .process_agent_result(
                        process_id,
                        agent_name,
                        output.output,
                        None,
                        output.metrics,
                        output.success,
                        &output.error_message,
                        false,
                    )
                    .await?;
            }

            InstructionKind::RunAgents => {
                execute_parallel(handle, process_id, &instruction, agents, event_tx.clone(), pipeline_name)
                    .await?;
            }

            InstructionKind::WaitParallel => continue,

            InstructionKind::WaitInterrupt => {
                let interrupt = &instruction.interrupt;
                let interrupt_id = interrupt.as_ref().map(|i| i.id.clone()).unwrap_or_default();

                if let Some(ref tx) = event_tx {
                    // Streaming: emit event, poll until resolved
                    let _ = tx.send(PipelineEvent::InterruptPending {
                        process_id: process_id.as_str().to_string(),
                        interrupt_id: interrupt_id.clone(),
                        kind: interrupt.as_ref().map(|i| format!("{:?}", i.kind)).unwrap_or_default(),
                        question: interrupt.as_ref().and_then(|i| i.question.clone()),
                        message: interrupt.as_ref().and_then(|i| i.message.clone()),
                        pipeline: pipeline_name.to_string(),
                    }).await;
                    // Poll: kernel returns WaitInterrupt until resolved, then RunAgent/Terminate
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        match handle.get_next_instruction(process_id).await?.kind {
                            InstructionKind::WaitInterrupt => continue,
                            _ => break, // resolved — outer loop re-fetches
                        }
                    }
                } else {
                    // Buffered: return incomplete result, caller resolves + re-enters
                    return Ok(WorkerResult {
                        process_id: process_id.clone(),
                        terminated: false,
                        terminal_reason: None,
                        outputs: Default::default(),
                    });
                }
            }
        }
    }
}

/// Execute agents in parallel (fork fan-out) and report each result.
async fn execute_parallel(
    handle: &KernelHandle,
    process_id: &ProcessId,
    instruction: &crate::kernel::orchestrator_types::Instruction,
    agents: &AgentRegistry,
    event_tx: Option<mpsc::Sender<PipelineEvent>>,
    pipeline_name: &str,
) -> Result<()> {
    let mut join_handles = Vec::new();

    for agent_name in &instruction.agents {
        let ctx = build_agent_context(instruction, event_tx.clone(), Some(agent_name.clone()), pipeline_name);
        let name = agent_name.clone();
        let agent_impl = agents.get(&name).cloned();

        join_handles.push(tokio::spawn(async move {
            let output: AgentOutput = if let Some(a) = agent_impl {
                a.process(&ctx).await.unwrap_or_else(|e| AgentOutput {
                    output: serde_json::json!({"error": e.to_string()}),
                    metrics: Default::default(),
                    success: false,
                    error_message: e.to_string(),
                })
            } else {
                DeterministicAgent.process(&ctx).await.unwrap_or_else(|e| AgentOutput {
                    output: serde_json::json!({"error": e.to_string()}),
                    metrics: Default::default(),
                    success: false,
                    error_message: e.to_string(),
                })
            };
            (name, output)
        }));
    }

    for jh in join_handles {
        if let Ok((name, output)) = jh.await {
            handle
                .process_agent_result(
                    process_id,
                    &name,
                    output.output,
                    None,
                    output.metrics,
                    output.success,
                    &output.error_message,
                    false,
                )
                .await?;
        }
    }

    Ok(())
}

/// Build an AgentContext from an instruction's agent_context field.
fn build_agent_context(
    instruction: &crate::kernel::orchestrator_types::Instruction,
    event_tx: Option<mpsc::Sender<PipelineEvent>>,
    stage_name: Option<String>,
    pipeline_name: &str,
) -> AgentContext {
    let ctx_val = instruction.agent_context.as_ref();

    AgentContext {
        raw_input: ctx_val
            .and_then(|c| c.get("raw_input"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        outputs: ctx_val
            .and_then(|c| c.get("outputs"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        state: ctx_val
            .and_then(|c| c.get("state"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        metadata: ctx_val
            .and_then(|c| c.get("metadata"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        allowed_tools: instruction.allowed_tools.clone().unwrap_or_default(),
        event_tx,
        stage_name,
        pipeline_name: pipeline_name.to_string(),
    }
}

/// Execute a single agent. Falls back to DeterministicAgent if not registered.
/// Emits PipelineEvent::Error on agent failure when streaming.
async fn execute_agent(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
) -> AgentOutput {
    let result = if let Some(agent) = agents.get(agent_name) {
        agent.process(ctx).await
    } else {
        DeterministicAgent.process(ctx).await
    };

    match result {
        Ok(output) => {
            if !output.success {
                if let Some(ref tx) = ctx.event_tx {
                    let _ = tx
                        .send(PipelineEvent::Error {
                            message: output.error_message.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.pipeline_name.clone(),
                        })
                        .await;
                }
            }
            output
        }
        Err(e) => {
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(PipelineEvent::Error {
                        message: e.to_string(),
                        stage: ctx.stage_name.clone(),
                        pipeline: ctx.pipeline_name.clone(),
                    })
                    .await;
            }
            AgentOutput {
                output: serde_json::json!({"error": e.to_string()}),
                metrics: Default::default(),
                success: false,
                error_message: e.to_string(),
            }
        }
    }
}
