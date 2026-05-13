//! Pipeline runner: drives `KernelHandle` through the Instruction → Agent
//! dispatch loop, with optional streaming events and per-stage timeout/retry.

use std::sync::Arc;

use tracing::{instrument, Instrument};

use crate::agent::llm::{self, RunEvent};
use crate::agent::metrics::AgentExecutionMetrics;
use crate::agent::{Agent, AgentContext, AgentOutput, AgentRegistry, DeterministicAgent};
use crate::run::Run;
use crate::kernel::handle::KernelHandle;
use crate::kernel::protocol::{AgentDispatchContext, Instruction};
use crate::types::{RunId, Result};
use crate::workflow::Workflow;
use tokio::sync::mpsc;

/// Result of running a workflow to completion.
#[must_use]
#[derive(Debug)]
pub struct WorkerResult {
    pub run_id: RunId,
    pub termination: Option<crate::run::Termination>,
    pub outputs: std::collections::HashMap<crate::types::AgentName, std::collections::HashMap<crate::types::OutputKey, serde_json::Value>>,
    pub aggregate_metrics: Option<llm::AggregateMetrics>,
}

impl WorkerResult {
    pub fn terminated(&self) -> bool {
        self.termination.is_some()
    }
    pub fn terminal_reason(&self) -> Option<crate::run::TerminalReason> {
        self.termination.as_ref().map(|t| t.reason)
    }
}

/// Run a workflow to completion with a pre-built `Run` (supports metadata).
pub async fn run(
    handle: &KernelHandle,
    run_id: RunId,
    workflow: Workflow,
    run: Run,
    agents: &AgentRegistry,
) -> Result<WorkerResult> {
    let workflow_name = workflow.name.clone();
    let _session = handle
        .initialize_session(run_id.clone(), workflow, run, false)
        .await?;
    run_loop(handle, &run_id, agents, None, &workflow_name).await
}

/// Run a workflow with streaming events. Returns a join handle and event receiver.
/// The receiver yields `RunEvent` items (StageStarted, Delta, ToolCallStart, etc.).
/// Session is initialized before spawning so rate-limit errors surface to the caller.
pub async fn run_streaming(
    handle: KernelHandle,
    run_id: RunId,
    workflow: Workflow,
    run: Run,
    agents: Arc<AgentRegistry>,
) -> Result<(
    tokio::task::JoinHandle<Result<WorkerResult>>,
    mpsc::Receiver<RunEvent>,
)> {
    let workflow_name = workflow.name.clone();
    let _state = handle
        .initialize_session(run_id.clone(), workflow, run, false)
        .await?;
    let (tx, rx) = mpsc::channel(64);
    let run_id_for_span = run_id.clone();
    let workflow_name_for_span = workflow_name.clone();
    let task = tokio::spawn(async move {
        run_loop(&handle, &run_id, &agents, Some(tx), &workflow_name).await
    }.instrument(tracing::info_span!("run_stream", run_id = %run_id_for_span, workflow = %workflow_name_for_span)));
    Ok((task, rx))
}

/// Run the dispatch loop for an already-initialized session.
/// Pass `event_tx = Some(tx)` for streaming events, `None` for buffered mode.
#[instrument(skip(handle, agents, event_tx), fields(run_id = %run_id, workflow = %workflow_name))]
pub async fn run_loop(
    handle: &KernelHandle,
    run_id: &RunId,
    agents: &AgentRegistry,
    event_tx: Option<mpsc::Sender<RunEvent>>,
    workflow_name: &str,
) -> Result<WorkerResult> {
    let workflow_name: Arc<str> = Arc::from(workflow_name);
    loop {
        let instruction = handle.get_next_instruction(run_id).await?;

        match instruction {
            Instruction::Terminate { reason, message, context } => {
                // Emit routing decision from the previous stage that led to termination
                if let Some(ref decision) = context.last_routing_decision {
                    if let Some(ref tx) = event_tx {
                        let _ = tx
                            .send(RunEvent::RoutingDecision {
                                from_stage: decision.from_stage.as_str().to_string(),
                                to_stage: decision.target.as_ref().map(|s| s.as_str().to_string()),
                                reason: decision.reason.clone(),
                                pipeline: workflow_name.clone(),
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
                        .send(RunEvent::Done {
                            run_id: run_id.as_str().to_string(),
                            terminated: true,
                            terminal_reason: Some(format!("{:?}", reason)),
                            outputs: Some(serde_json::to_value(&outputs).unwrap_or_default()),
                            pipeline: workflow_name.clone(),
                            aggregate_metrics: aggregate_metrics.clone(),
                        })
                        .await;
                }

                return Ok(WorkerResult {
                    run_id: run_id.clone(),
                    termination: Some(crate::run::Termination { reason, message }),
                    outputs,
                    aggregate_metrics,
                });
            }

            Instruction::RunAgent { ref agent, ref context } => {
                // Emit routing decision from the previous stage that led here
                if let Some(ref decision) = context.last_routing_decision {
                    if let Some(ref tx) = event_tx {
                        let _ = tx
                            .send(RunEvent::RoutingDecision {
                                from_stage: decision.from_stage.as_str().to_string(),
                                to_stage: decision.target.as_ref().map(|s| s.as_str().to_string()),
                                reason: decision.reason.clone(),
                                pipeline: workflow_name.clone(),
                            })
                            .await;
                    }
                }

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(RunEvent::StageStarted {
                            stage: agent.clone(),
                            pipeline: workflow_name.clone(),
                        })
                        .await;
                }

                let ctx = build_agent_context(context, event_tx.clone(), Some(agent.clone()), workflow_name.clone());
                let output = execute_agent_with_policy(
                    agents, agent, &ctx,
                    context.timeout_seconds,
                    context.retry_policy.as_ref(),
                ).await;

                // Tool confirmation gate: if agent requests an interrupt, suspend stage
                if let Some(interrupt) = output.interrupt_request {
                    handle.set_run_interrupt(run_id, interrupt).await?;
                    continue; // get_next_instruction → WaitInterrupt
                }

                if let Some(ref tx) = event_tx {
                    let _ = tx
                        .send(RunEvent::StageCompleted {
                            stage: agent.clone(),
                            pipeline: workflow_name.clone(),
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
                        run_id,
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

            Instruction::WaitInterrupt { ref interrupt } => {
                let interrupt_id = interrupt.as_ref().map(|i| i.id.as_str().to_string()).unwrap_or_default();

                if let Some(ref tx) = event_tx {
                    // Streaming: emit event, poll until resolved
                    let _ = tx.send(RunEvent::InterruptPending {
                        run_id: run_id.as_str().to_string(),
                        interrupt_id: interrupt_id.clone(),
                        kind: String::new(),
                        question: interrupt.as_ref().and_then(|i| i.question.clone()),
                        message: interrupt.as_ref().and_then(|i| i.message.clone()),
                        pipeline: workflow_name.clone(),
                    }).await;
                    // Poll: kernel returns WaitInterrupt until resolved, then RunAgent/Terminate
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        if !matches!(handle.get_next_instruction(run_id).await?, Instruction::WaitInterrupt { .. }) {
                            break; // resolved — outer loop re-fetches
                        }
                    }
                } else {
                    // Buffered: return incomplete result, caller resolves + re-enters
                    return Ok(WorkerResult {
                        run_id: run_id.clone(),
                        termination: None,
                        outputs: Default::default(),
                        aggregate_metrics: None,
                    });
                }
            }
        }
    }
}

/// Build an AgentContext from an AgentDispatchContext.
fn build_agent_context(
    context: &AgentDispatchContext,
    event_tx: Option<mpsc::Sender<RunEvent>>,
    stage_name: Option<String>,
    workflow_name: Arc<str>,
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
        workflow_name,
        max_context_tokens: context.max_context_tokens,
        context_overflow: context.context_overflow,
        interrupt_response: context.interrupt_response.clone(),
        response_format: context.response_format.clone(),
    }
}

/// Execute an agent with per-stage timeout and retry policy, brackted by
/// `AgentHook::before_agent` / `after_agent` fires (once per logical
/// execution, outside the retry envelope).
#[instrument(skip(agents, ctx, retry_policy), fields(agent = %agent_name))]
async fn execute_agent_with_policy(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
    timeout_seconds: Option<u64>,
    retry_policy: Option<&crate::workflow::RetryPolicy>,
) -> AgentOutput {
    for hook in agents.agent_hooks() {
        hook.before_agent(ctx).await;
    }

    let mut output = execute_with_retry(agents, agent_name, ctx, timeout_seconds, retry_policy).await;

    for hook in agents.agent_hooks() {
        hook.after_agent(ctx, &mut output).await;
    }

    output
}

/// Retry loop with exponential backoff. Per-stage timeout applies to each
/// attempt; metrics accumulate across attempts so bounds accounting is
/// accurate.
async fn execute_with_retry(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
    timeout_seconds: Option<u64>,
    retry_policy: Option<&crate::workflow::RetryPolicy>,
) -> AgentOutput {
    let max_attempts = retry_policy.map(|r| r.max_retries + 1).unwrap_or(1);
    let mut accumulated_metrics = AgentExecutionMetrics::default();

    for attempt in 0..max_attempts {
        let output = execute_agent_with_timeout(agents, agent_name, ctx, timeout_seconds, attempt).await;

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

        if output.success || output.interrupt_request.is_some() || attempt + 1 >= max_attempts {
            return AgentOutput {
                metrics: accumulated_metrics,
                ..output
            };
        }

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
                let _ = tx.send(RunEvent::Error {
                    message: msg.clone(),
                    stage: ctx.stage_name.clone(),
                    pipeline: ctx.workflow_name.clone(),
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
/// Emits RunEvent::Error on agent failure when streaming.
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
                        .send(RunEvent::Error {
                            message: output.error_message.clone(),
                            stage: ctx.stage_name.clone(),
                            pipeline: ctx.workflow_name.clone(),
                        })
                        .await;
                }
            }
            output
        }
        Err(e) => {
            if let Some(ref tx) = ctx.event_tx {
                let _ = tx
                    .send(RunEvent::Error {
                        message: e.to_string(),
                        stage: ctx.stage_name.clone(),
                        pipeline: ctx.workflow_name.clone(),
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
