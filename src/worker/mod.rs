//! Worker module — embedded agent execution (pipeline loop, LLM, tools).
//!
//! Single process: kernel actor ↔ agent tasks ↔ LLM HTTP calls.

pub mod actor;
pub mod agent;
pub mod gateway;
pub mod handle;
pub mod llm;
pub mod prompts;
pub mod tools;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::{InstructionKind, PipelineConfig};
use crate::types::{ProcessId, Result};

use agent::{Agent, AgentContext, AgentOutput, AgentRegistry, DeterministicAgent};
use handle::KernelHandle;

/// Result of running a pipeline to completion.
#[derive(Debug)]
pub struct WorkerResult {
    pub process_id: ProcessId,
    pub terminated: bool,
    pub terminal_reason: Option<crate::envelope::TerminalReason>,
    pub outputs: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

/// Run a pipeline to completion: init session + loop.
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
    let _session = handle
        .initialize_session(process_id.clone(), pipeline_config, envelope, false)
        .await?;
    run_pipeline_loop(handle, &process_id, agents).await
}

/// Run the pipeline loop for an already-initialized session.
pub async fn run_pipeline_loop(
    handle: &KernelHandle,
    process_id: &ProcessId,
    agents: &AgentRegistry,
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

                let ctx = build_agent_context(&instruction);
                let output = execute_agent(agents, agent_name, &ctx).await;

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
                execute_parallel(handle, process_id, &instruction, agents).await?;
            }

            InstructionKind::WaitParallel => continue,

            InstructionKind::WaitInterrupt => {
                tracing::warn!("WaitInterrupt not yet implemented in embedded worker");
                return Err(crate::types::Error::internal(
                    "WaitInterrupt not supported in embedded worker",
                ));
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
) -> Result<()> {
    let mut join_handles = Vec::new();

    for agent_name in &instruction.agents {
        let ctx = build_agent_context(instruction);
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
    }
}

/// Execute a single agent. Falls back to DeterministicAgent if not registered.
async fn execute_agent(
    agents: &AgentRegistry,
    agent_name: &str,
    ctx: &AgentContext,
) -> AgentOutput {
    if let Some(agent) = agents.get(agent_name) {
        agent.process(ctx).await.unwrap_or_else(|e| AgentOutput {
            output: serde_json::json!({"error": e.to_string()}),
            metrics: Default::default(),
            success: false,
            error_message: e.to_string(),
        })
    } else {
        DeterministicAgent
            .process(ctx)
            .await
            .unwrap_or_else(|e| AgentOutput {
                output: serde_json::json!({"error": e.to_string()}),
                metrics: Default::default(),
                success: false,
                error_message: e.to_string(),
            })
    }
}
