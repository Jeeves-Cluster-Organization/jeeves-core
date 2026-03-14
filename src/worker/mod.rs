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
use crate::kernel::orchestrator_types::{AgentDispatchContext, Instruction, PipelineConfig};
use crate::types::{ProcessId, Result};

use agent::{Agent, AgentContext, AgentOutput, AgentRegistry, DeterministicAgent};
use handle::KernelHandle;
use llm::PipelineEvent;
use tokio::sync::mpsc;

/// Result of running a pipeline to completion.
#[derive(Debug)]
pub struct WorkerResult {
    pub process_id: ProcessId,
    pub termination: Option<crate::envelope::Termination>,
    pub outputs: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

impl WorkerResult {
    pub fn terminated(&self) -> bool {
        self.termination.is_some()
    }
    pub fn terminal_reason(&self) -> Option<crate::envelope::TerminalReason> {
        self.termination.as_ref().map(|t| t.reason)
    }
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
    let envelope = Envelope::new_minimal(user_id, session_id, raw_input, None);
    run_pipeline_with_envelope(handle, process_id, pipeline_config, envelope, agents).await
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
    let task = tokio::spawn(async move {
        run_pipeline_loop(&handle, &process_id, &agents, Some(tx), &pipeline_name).await
    });
    Ok((task, rx))
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
    let pipeline_name: Arc<str> = Arc::from(pipeline_name);
    loop {
        let instruction = handle.get_next_instruction(process_id).await?;

        match instruction {
            Instruction::Terminate { reason, message, context } => {
                let outputs = context
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
                            terminal_reason: Some(format!("{:?}", reason)),
                            outputs: Some(serde_json::to_value(&outputs).unwrap_or_default()),
                            pipeline: pipeline_name.clone(),
                        })
                        .await;
                }

                return Ok(WorkerResult {
                    process_id: process_id.clone(),
                    termination: Some(crate::envelope::Termination { reason, message }),
                    outputs,
                });
            }

            Instruction::RunAgent { ref agent, ref context } => {
                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageStarted {
                            stage: agent.clone(),
                            pipeline: pipeline_name.clone(),
                        })
                        .await;
                }

                let ctx = build_agent_context(context, event_tx.clone(), Some(agent.clone()), pipeline_name.clone());
                let output = execute_agent(agents, agent, &ctx).await;

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageCompleted {
                            stage: agent.clone(),
                            pipeline: pipeline_name.clone(),
                        })
                        .await;
                }

                handle
                    .process_agent_result(
                        process_id,
                        agent,
                        output.output,
                        None,
                        output.metrics,
                        output.success,
                        &output.error_message,
                        false,
                    )
                    .await?;
            }

            Instruction::RunAgents { agents: ref agent_names, ref context } => {
                execute_parallel(handle, process_id, agent_names, context, agents, event_tx.clone(), pipeline_name.clone())
                    .await?;
            }

            Instruction::WaitParallel => continue,

            Instruction::WaitInterrupt { ref interrupt } => {
                let interrupt_id = interrupt.as_ref().map(|i| i.id.clone()).unwrap_or_default();

                if let Some(ref tx) = event_tx {
                    // Streaming: emit event, poll until resolved
                    let _ = tx.send(PipelineEvent::InterruptPending {
                        process_id: process_id.as_str().to_string(),
                        interrupt_id: interrupt_id.clone(),
                        kind: interrupt.as_ref().map(|i| format!("{:?}", i.kind)).unwrap_or_default(),
                        question: interrupt.as_ref().and_then(|i| i.question.clone()),
                        message: interrupt.as_ref().and_then(|i| i.message.clone()),
                        pipeline: pipeline_name.clone(),
                    }).await;
                    // Poll: kernel returns WaitInterrupt until resolved, then RunAgent/Terminate
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        if !matches!(handle.get_next_instruction(process_id).await?, Instruction::WaitInterrupt { .. }) {
                            break; // resolved — outer loop re-fetches
                        }
                    }
                } else {
                    // Buffered: return incomplete result, caller resolves + re-enters
                    return Ok(WorkerResult {
                        process_id: process_id.clone(),
                        termination: None,
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
    agent_names: &[String],
    context: &AgentDispatchContext,
    agents: &AgentRegistry,
    event_tx: Option<mpsc::Sender<PipelineEvent>>,
    pipeline_name: Arc<str>,
) -> Result<()> {
    let mut join_handles = Vec::new();

    for agent_name in agent_names {
        let ctx = build_agent_context(context, event_tx.clone(), Some(agent_name.clone()), pipeline_name.clone());
        let agent_name = agent_name.clone();
        let agent_impl = agents.get(&agent_name).cloned();

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
            (agent_name, output)
        }));
    }

    for task in join_handles {
        if let Ok((agent_name, output)) = task.await {
            handle
                .process_agent_result(
                    process_id,
                    &agent_name,
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

/// Build an AgentContext from an AgentDispatchContext.
fn build_agent_context(
    context: &AgentDispatchContext,
    event_tx: Option<mpsc::Sender<PipelineEvent>>,
    stage_name: Option<String>,
    pipeline_name: Arc<str>,
) -> AgentContext {
    let dispatch_payload = context.agent_context.as_ref();

    AgentContext {
        raw_input: dispatch_payload
            .and_then(|c| c.get("raw_input"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        outputs: dispatch_payload
            .and_then(|c| c.get("outputs"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        state: dispatch_payload
            .and_then(|c| c.get("state"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        metadata: dispatch_payload
            .and_then(|c| c.get("metadata"))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        allowed_tools: context.allowed_tools.clone().unwrap_or_default(),
        event_tx,
        stage_name,
        pipeline_name,
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
