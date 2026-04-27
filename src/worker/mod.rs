//! Worker module — embedded agent execution (pipeline loop, LLM, tools).
//!
//! Single process: kernel actor ↔ agent tasks ↔ LLM HTTP calls.

pub mod actor;
pub mod agent;
pub mod agent_factory;
pub mod handle;
pub mod llm;
pub mod mcp;
pub mod prompts;
pub mod tools;

use std::sync::Arc;

use tracing::{instrument, Instrument};

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::{AgentDispatchContext, AgentExecutionMetrics, Instruction, PipelineConfig};
use crate::types::{ProcessId, Result};

use agent::{Agent, AgentContext, AgentOutput, AgentRegistry, DeterministicAgent};
use handle::KernelHandle;
use llm::PipelineEvent;
use tokio::sync::mpsc;

/// Result of running a pipeline to completion.
#[must_use]
#[derive(Debug)]
pub struct WorkerResult {
    pub process_id: ProcessId,
    pub termination: Option<crate::envelope::Termination>,
    pub outputs: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
    pub aggregate_metrics: Option<llm::AggregateMetrics>,
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
    let _state = handle
        .initialize_session(process_id.clone(), pipeline_config, envelope, false)
        .await?;
    let (tx, rx) = mpsc::channel(64);
    let process_id_for_span = process_id.clone();
    let pipeline_name_for_span = pipeline_name.clone();
    let task = tokio::spawn(async move {
        run_pipeline_loop(&handle, &process_id, &agents, Some(tx), &pipeline_name).await
    }.instrument(tracing::info_span!("pipeline_stream", process_id = %process_id_for_span, pipeline = %pipeline_name_for_span)));
    Ok((task, rx))
}

/// Run the pipeline loop for an already-initialized session.
/// Pass `event_tx = Some(tx)` for streaming events, `None` for buffered mode.
#[instrument(skip(handle, agents, event_tx), fields(process_id = %process_id, pipeline = %pipeline_name))]
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
                // Emit routing decision from the previous stage that led to termination
                if let Some(ref decision) = context.last_routing_decision {
                    if let Some(ref tx) = event_tx {
                        let _ = tx
                            .send(PipelineEvent::RoutingDecision {
                                from_stage: decision.from_stage.clone(),
                                to_stage: decision.target.clone(),
                                reason: decision.reason.clone(),
                                pipeline: pipeline_name.clone(),
                            })
                            .await;
                    }
                }

                let outputs = context
                    .agent_context
                    .as_ref()
                    .and_then(|c| c.get("outputs"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let aggregate_metrics: Option<llm::AggregateMetrics> = context
                    .agent_context
                    .as_ref()
                    .and_then(|c| c.get("aggregate_metrics"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok());

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::Done {
                            process_id: process_id.as_str().to_string(),
                            terminated: true,
                            terminal_reason: Some(format!("{:?}", reason)),
                            outputs: Some(serde_json::to_value(&outputs).unwrap_or_default()),
                            pipeline: pipeline_name.clone(),
                            aggregate_metrics: aggregate_metrics.clone(),
                        })
                        .await;
                }

                return Ok(WorkerResult {
                    process_id: process_id.clone(),
                    termination: Some(crate::envelope::Termination { reason, message }),
                    outputs,
                    aggregate_metrics,
                });
            }

            Instruction::RunAgent { ref agent, ref context } => {
                // Emit routing decision from the previous stage that led here
                if let Some(ref decision) = context.last_routing_decision {
                    if let Some(ref tx) = event_tx {
                        let _ = tx
                            .send(PipelineEvent::RoutingDecision {
                                from_stage: decision.from_stage.clone(),
                                to_stage: decision.target.clone(),
                                reason: decision.reason.clone(),
                                pipeline: pipeline_name.clone(),
                            })
                            .await;
                    }
                }

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageStarted {
                            stage: agent.clone(),
                            pipeline: pipeline_name.clone(),
                        })
                        .await;
                }

                let ctx = build_agent_context(context, event_tx.clone(), Some(agent.clone()), pipeline_name.clone());
                let output = execute_agent_with_policy(
                    agents, agent, &ctx,
                    context.timeout_seconds,
                    context.retry_policy.as_ref(),
                ).await;

                // Tool confirmation gate: if agent requests an interrupt, suspend stage
                if let Some(interrupt) = output.interrupt_request {
                    handle.set_process_interrupt(process_id, interrupt).await?;
                    continue; // get_next_instruction → WaitInterrupt
                }

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(PipelineEvent::StageCompleted {
                            stage: agent.clone(),
                            pipeline: pipeline_name.clone(),
                            metrics: Some(llm::StageMetrics {
                                duration_ms: output.metrics.duration_ms,
                                llm_calls: output.metrics.llm_calls,
                                tool_calls: output.metrics.tool_calls,
                                tokens_in: output.metrics.tokens_in.unwrap_or(0),
                                tokens_out: output.metrics.tokens_out.unwrap_or(0),
                                tool_results: output.metrics.tool_results.clone(),
                                success: output.success,
                            }),
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
                        aggregate_metrics: None,
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

    let timeout_secs = context.timeout_seconds;

    for agent_name in agent_names {
        let ctx = build_agent_context(context, event_tx.clone(), Some(agent_name.clone()), pipeline_name.clone());
        let agent_name = agent_name.clone();
        let agent_impl = agents.get(&agent_name).cloned();

        let agent_name_for_span = agent_name.clone();
        join_handles.push(tokio::spawn(async move {
            let agent_future = async {
                if let Some(a) = agent_impl {
                    a.process(&ctx).await.unwrap_or_else(|e| AgentOutput {
                        output: serde_json::json!({"error": e.to_string()}),
                        metrics: Default::default(),
                        success: false,
                        error_message: e.to_string(),
                        interrupt_request: None,
                    })
                } else {
                    DeterministicAgent.process(&ctx).await.unwrap_or_else(|e| AgentOutput {
                        output: serde_json::json!({"error": e.to_string()}),
                        metrics: Default::default(),
                        success: false,
                        error_message: e.to_string(),
                        interrupt_request: None,
                    })
                }
            };
            let output = if let Some(secs) = timeout_secs {
                match tokio::time::timeout(std::time::Duration::from_secs(secs), agent_future).await {
                    Ok(output) => output,
                    Err(_) => AgentOutput {
                        output: serde_json::json!({"error": format!("Stage timeout after {}s", secs)}),
                        metrics: Default::default(),
                        success: false,
                        error_message: format!("Stage timeout after {}s", secs),
                        interrupt_request: None,
                    },
                }
            } else {
                agent_future.await
            };
            (agent_name, output)
        }.instrument(tracing::debug_span!("parallel_agent", agent = %agent_name_for_span))));
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
        event_tx,
        stage_name,
        pipeline_name,
        max_context_tokens: context.max_context_tokens,
        context_overflow: context.context_overflow,
        interrupt_response: context.interrupt_response.clone(),
    }
}

/// Execute an agent with per-stage timeout and retry policy.
///
/// Wraps `execute_agent` with:
/// - `tokio::time::timeout` (per-stage wall-clock deadline)
/// - Retry loop with exponential backoff on transient failures
///
/// Timeout/retry are no-ops when None (existing behavior preserved).
#[instrument(skip(agents, ctx, retry_policy), fields(agent = %agent_name))]
async fn execute_agent_with_policy(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
    timeout_seconds: Option<u64>,
    retry_policy: Option<&crate::kernel::orchestrator_types::RetryPolicy>,
) -> AgentOutput {
    let max_attempts = retry_policy.map(|r| r.max_retries + 1).unwrap_or(1);
    let mut accumulated_metrics = AgentExecutionMetrics::default();

    for attempt in 0..max_attempts {
        let output = execute_agent_with_timeout(agents, agent_name, ctx, timeout_seconds, attempt).await;

        // Accumulate metrics across attempts so bounds accounting is accurate
        accumulated_metrics.llm_calls += output.metrics.llm_calls;
        accumulated_metrics.tool_calls += output.metrics.tool_calls;
        accumulated_metrics.tokens_in = Some(
            accumulated_metrics.tokens_in.unwrap_or(0) + output.metrics.tokens_in.unwrap_or(0),
        );
        accumulated_metrics.tokens_out = Some(
            accumulated_metrics.tokens_out.unwrap_or(0) + output.metrics.tokens_out.unwrap_or(0),
        );
        accumulated_metrics.duration_ms += output.metrics.duration_ms;
        accumulated_metrics.tool_results.extend(output.metrics.tool_results.clone());

        // Terminal: success, interrupt, or final attempt
        if output.success || output.interrupt_request.is_some() || attempt + 1 >= max_attempts {
            return AgentOutput {
                metrics: accumulated_metrics,
                ..output
            };
        }

        // Transient failure — backoff before next attempt
        if let Some(policy) = retry_policy {
            let backoff_ms = (policy.initial_backoff_ms as f64
                * policy.backoff_multiplier.powi(attempt as i32)) as u64;
            let capped_ms = backoff_ms.min(policy.max_backoff_ms);
            tracing::info!(agent = %agent_name, attempt, backoff_ms = capped_ms, "agent_retry");
            tokio::time::sleep(std::time::Duration::from_millis(capped_ms)).await;
        }
    }

    unreachable!("loop always returns on final attempt")
}

/// Single agent execution attempt with optional timeout.
async fn execute_agent_with_timeout(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
    timeout_seconds: Option<u64>,
    attempt: u32,
) -> AgentOutput {
    let Some(secs) = timeout_seconds else {
        return execute_agent(agents, agent_name, ctx).await;
    };

    match tokio::time::timeout(std::time::Duration::from_secs(secs), execute_agent(agents, agent_name, ctx)).await {
        Ok(output) => output,
        Err(_elapsed) => {
            let msg = format!("Stage timeout after {}s (attempt {})", secs, attempt);
            tracing::warn!(agent = %agent_name, timeout_secs = secs, attempt, "stage_timeout");
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx.send(PipelineEvent::Error {
                    message: msg.clone(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.pipeline_name.clone(),
                }).await;
            }
            AgentOutput {
                output: serde_json::json!({"error": &msg}),
                metrics: Default::default(),
                success: false,
                error_message: msg,
                interrupt_request: None,
            }
        }
    }
}

/// Execute a single agent. Falls back to DeterministicAgent if not registered.
/// Emits PipelineEvent::Error on agent failure when streaming.
#[instrument(skip(agents, ctx), fields(agent = %agent_name))]
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
                interrupt_request: None,
            }
        }
    }
}
