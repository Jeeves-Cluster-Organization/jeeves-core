//! Kernel orchestration methods — initialize, get_next_instruction, process_agent_result.

use std::collections::HashMap;

use tracing::instrument;

use crate::run::{Run, FlowInterrupt};
use crate::types::{Error, RunId, RequestId, Result, SessionId, UserId};

use super::merge_state_field;
use super::orchestrator;
use crate::workflow::ContextOverflow;
use super::{Kernel, RunStatus, RemainingBudget, ResourceQuota, SystemStatus};

impl Kernel {
    /// Stores `envelope` in `runs` and hands it to the orchestrator
    /// to seed the session. The orchestrator updates the envelope's pipeline
    /// bounds in place; ownership stays with `runs`.
    #[instrument(skip(self, pipeline_config, envelope), fields(run_id = %run_id))]
    pub fn initialize_orchestration(
        &mut self,
        run_id: RunId,
        pipeline_config: orchestrator::Workflow,
        mut envelope: Run,
        force: bool,
    ) -> Result<orchestrator::RunSnapshot> {
        let state = self.orchestrator
            .initialize_session(run_id.clone(), pipeline_config, &mut envelope, force)?;
        self.runs.insert(run_id, envelope);

        Ok(state)
    }

    /// Fetches and enriches the next instruction for `run_id`. The
    /// orchestrator may mutate the envelope on its way to a `Terminate`
    /// (bounds, errors); enrichment then layers in agent context, stage
    /// timeouts, response format, and any routing decision from the previous
    /// `report_agent_result`.
    #[instrument(skip(self), fields(run_id = %run_id))]
    pub fn get_next_instruction(
        &mut self,
        run_id: &RunId,
    ) -> Result<orchestrator::Instruction> {
        let envelope = self.runs.get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Run not found for process: {}", run_id)))?;
        let mut instruction = self.orchestrator.get_next_instruction(run_id, envelope)?;

        match &mut instruction {
            orchestrator::Instruction::RunAgent { agent: _, context }=> {
                let enrichment = self.build_enrichment_context(run_id);
                if let Some((agent_context, max_ctx, overflow)) = enrichment {
                    context.agent_context = Some(agent_context);
                    context.max_context_tokens = max_ctx;
                    context.context_overflow = overflow;
                }

                if let Some(env) = self.runs.get_mut(run_id) {
                    context.interrupt_response = env.audit.metadata.remove("_interrupt_response");
                }

                let stage_name = self.runs.get(run_id)
                    .map(|e| e.pipeline.current_stage.clone())
                    .unwrap_or_default();

                if let Some(sc) = self.orchestrator.get_stage_config(run_id, &stage_name) {
                    context.timeout_seconds = sc.timeout_seconds;
                    context.retry_policy = sc.retry_policy.clone();
                }

                context.response_format = self.orchestrator.get_stage_response_format(run_id, &stage_name);
            }
            orchestrator::Instruction::Terminate { context, .. } => {
                if let Some(envelope) = self.runs.get(run_id) {
                    let total_duration_ms = (chrono::Utc::now() - envelope.audit.created_at)
                        .num_milliseconds();
                    context.agent_context = Some(serde_json::json!({
                        "outputs": &envelope.outputs,
                        "aggregate_metrics": {
                            "total_duration_ms": total_duration_ms,
                            "total_llm_calls": envelope.bounds.llm_call_count,
                            "total_tool_calls": envelope.bounds.tool_call_count,
                            "total_tokens_in": envelope.bounds.tokens_in,
                            "total_tokens_out": envelope.bounds.tokens_out,
                            "stages_executed": &envelope.pipeline.stage_order,
                        }
                    }));
                }
            }
            _ => {}
        }

        if let Some(decision) = self.orchestrator.take_last_routing_decision(run_id) {
            match &mut instruction {
                orchestrator::Instruction::RunAgent { context, .. } |
                orchestrator::Instruction::Terminate { context, .. } => {
                    context.last_routing_decision = Some(decision);
                }
                _ => {}
            }
        }

        Ok(instruction)
    }

    /// Merges an agent's output into the envelope, reports it to the
    /// orchestrator, and applies the metrics delta to the PCB. The caller
    /// pulls the next instruction separately — the split is what keeps
    /// fork/parallel paths deadlock-free.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self, output, metrics), fields(run_id = %run_id))]
    pub fn process_agent_result(
        &mut self,
        run_id: &RunId,
        agent_name: &str,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: orchestrator::AgentExecutionMetrics,
        success: bool,
        error_message: &str,
        break_loop: bool,
    ) -> Result<()> {
        // Pull scalars now so we can move `metrics` into the orchestrator below.
        let llm_calls = metrics.llm_calls;
        let tool_calls = metrics.tool_calls;
        let tokens_in = metrics.tokens_in.unwrap_or(0);
        let tokens_out = metrics.tokens_out.unwrap_or(0);
        let duration_ms = metrics.duration_ms;

        for tool_result in &metrics.tool_results {
            self.tools.health.record_execution(&tool_result.name, tool_result.success, tool_result.latency_ms, tool_result.error_type.clone());
        }

        // Lift state_schema + output_key out before the &mut envelope borrow.
        let state_schema = self.orchestrator.get_state_schema(run_id).cloned().unwrap_or_default();
        let output_key = self.orchestrator.get_stage_output_key(run_id, agent_name)
            .unwrap_or_else(|| agent_name.to_string());

        {
            let envelope = self.runs.get_mut(run_id)
                .ok_or_else(|| Error::not_found(format!("Run not found: {}", run_id)))?;

            let mut agent_output = std::collections::HashMap::new();
            if let serde_json::Value::Object(output_map) = output {
                for (key, value) in output_map {
                    agent_output.insert(key, value);
                }
            }
            if !success {
                agent_output.insert("success".to_string(), serde_json::Value::Bool(false));
                if !error_message.is_empty() {
                    agent_output.insert("error".to_string(), serde_json::Value::String(error_message.to_string()));
                }
                envelope.audit.metadata.insert(
                    "last_agent_failure".to_string(),
                    serde_json::json!({
                        "agent_name": agent_name,
                        "error": error_message,
                    }),
                );
            }
            envelope.outputs.insert(agent_name.to_string(), agent_output);

            let mut state_matched = false;
            for field in &state_schema {
                if field.key == output_key {
                    let output_value = serde_json::Value::Object(
                        envelope.outputs.get(agent_name)
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default()
                    );
                    merge_state_field(&mut envelope.state, &field.key, output_value, field.merge);
                    state_matched = true;
                    break;
                }
            }
            if !state_schema.is_empty() && !state_matched {
                tracing::debug!(output_key = %output_key, "output_key has no matching state_schema entry");
            }

            if let Some(meta_updates) = metadata_updates {
                for (key, value) in meta_updates {
                    envelope.audit.metadata.insert(key, value);
                }
            }

            let effective_failed = !success;
            self.orchestrator.report_agent_result(run_id, agent_name, metrics, envelope, effective_failed, break_loop)?;

            let now = chrono::Utc::now();
            envelope.audit.processing_history.push(crate::run::ProcessingRecord {
                agent: agent_name.to_string(),
                stage_order: envelope.pipeline.current_stage_number,
                started_at: now - chrono::Duration::milliseconds(duration_ms),
                completed_at: Some(now),
                duration_ms: duration_ms as i32,
                status: if !effective_failed { crate::run::ProcessingStatus::Success } else { crate::run::ProcessingStatus::Error },
                error: if error_message.is_empty() { None } else { Some(error_message.to_string()) },
                llm_calls,
                tool_calls,
                tokens_in,
                tokens_out,
            });
        }

        if let Some(uid) = self.lifecycle.get(run_id).map(|p| p.user_id.as_str().to_string()) {
            self.record_usage(run_id, &uid, llm_calls, tool_calls, tokens_in, tokens_out);
        }

        Ok(())
    }

    /// Get orchestration session state.
    pub fn get_orchestration_state(
        &self,
        run_id: &RunId,
    ) -> Result<orchestrator::RunSnapshot> {
        let envelope = self.runs.get(run_id)
            .ok_or_else(|| Error::not_found(format!("Run not found for process: {}", run_id)))?;
        self.orchestrator.get_session_state(run_id, envelope)
    }

    /// Reads the envelope and stage config, packs them into the JSON shape
    /// the worker expects, and returns it alongside the per-stage context-window
    /// bounds.
    fn build_enrichment_context(
        &self,
        run_id: &RunId,
    ) -> Option<(serde_json::Value, Option<i64>, Option<ContextOverflow>)> {
        let envelope = self.runs.get(run_id)?;

        let mut template_vars = serde_json::Map::new();
        for (agent_name, output) in &envelope.outputs {
            for (key, value) in output {
                template_vars.insert(format!("{}_{}", agent_name, key), value.clone());
            }
        }
        for (key, value) in &envelope.audit.metadata {
            template_vars.insert(key.clone(), value.clone());
        }

        let agent_context = serde_json::json!({
            "envelope_id": envelope.identity.envelope_id.as_str(),
            "request_id": envelope.identity.request_id.as_str(),
            "user_id": envelope.identity.user_id.as_str(),
            "session_id": envelope.identity.session_id.as_str(),
            "raw_input": &envelope.raw_input,
            "outputs": &envelope.outputs,
            "state": &envelope.state,
            "metadata": &envelope.audit.metadata,
            "template_vars": serde_json::Value::Object(template_vars),
            "llm_call_count": envelope.bounds.llm_call_count,
            "agent_hop_count": envelope.bounds.agent_hop_count,
            "tokens_in": envelope.bounds.tokens_in,
            "tokens_out": envelope.bounds.tokens_out,
            "circuit_broken_tools": self.tools.health.get_circuit_broken_tools(),
        });

        let stage_name = envelope.pipeline.current_stage.clone();
        let (max_context_tokens, context_overflow) = self.orchestrator
            .get_stage_config(run_id, &stage_name)
            .map(|sc| {
                let overflow = if sc.max_context_tokens.is_some() {
                    Some(sc.context_overflow)
                } else {
                    None
                };
                (sc.max_context_tokens, overflow)
            })
            .unwrap_or((None, None));

        Some((agent_context, max_context_tokens, context_overflow))
    }

    /// Create a new process.
    #[instrument(skip(self), fields(pid = %pid))]
    pub fn create_process(
        &mut self,
        pid: RunId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        quota: Option<ResourceQuota>,
    ) -> Result<super::RunRecord> {
        self.lifecycle.create(pid, request_id, user_id, session_id, quota)
    }

    /// Check process quota.
    pub fn check_quota(&self, pid: &RunId) -> Result<()> {
        let pcb = self
            .lifecycle
            .get(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
        self.resources.check_quota(pcb)
    }

    /// Record resource usage.
    pub fn record_usage(
        &mut self,
        pid: &RunId,
        user_id: &str,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    ) {
        self.resources
            .record_usage(user_id, llm_calls, tool_calls, tokens_in, tokens_out);
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.usage.llm_calls += llm_calls;
            pcb.usage.tool_calls += tool_calls;
            pcb.usage.tokens_in += tokens_in;
            pcb.usage.tokens_out += tokens_out;
        }
    }

    /// Set a tool-confirmation interrupt on a process. The pipeline loop
    /// suspends the stage; the consumer resolves via `resolve_process_interrupt`.
    pub fn set_process_interrupt(&mut self, pid: &RunId, interrupt: FlowInterrupt) -> Result<()> {
        // Register in interrupt manager (so resolve_interrupt can find it by ID)
        let interrupt_id = interrupt.id.clone();
        if let Some(env) = self.runs.get(pid) {
            self.interrupts.register_flow_interrupt(
                interrupt.clone(),
                env.identity.request_id.as_str(),
                env.identity.user_id.as_str(),
                env.identity.session_id.as_str(),
                env.identity.envelope_id.as_str(),
            );
        }

        // Mark on PCB for resolve_interrupt's awareness.
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.pending_interrupt = Some(interrupt_id);
        }

        // Set on envelope (get_next_instruction will see it → WaitInterrupt)
        let env = self.runs.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Run not found: {}", pid)))?;
        env.set_interrupt(interrupt);
        Ok(())
    }

    /// Resolve a pending interrupt and stash the response for the next agent dispatch.
    pub fn resolve_process_interrupt(
        &mut self,
        pid: &RunId,
        interrupt_id: &str,
        response: crate::run::InterruptResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_value(&response).unwrap_or_default();
        if !self.interrupts.resolve(interrupt_id, response, None) {
            return Err(Error::not_found(format!("Interrupt {} not found", interrupt_id)));
        }

        if let Some(env) = self.runs.get_mut(pid) {
            env.audit.metadata.insert("_interrupt_response".to_string(), response_json);
            env.clear_interrupt();
        }
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.pending_interrupt = None;
        }
        Ok(())
    }

    /// Terminate a process and remove it from the kernel.
    pub fn terminate_process(&mut self, pid: &RunId) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        if let Some(env) = self.runs.get_mut(pid) {
            env.terminate("Process terminated");
        }
        self.runs.remove(pid);
        self.orchestrator.cleanup_session(pid);
        Ok(())
    }

    /// Record a tool call for a process.
    pub fn record_tool_call(&mut self, pid: &RunId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_tool_call", pid)))?;
        pcb.usage.tool_calls += 1;
        Ok(())
    }

    /// Record an agent hop for a process.
    pub fn record_agent_hop(&mut self, pid: &RunId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_agent_hop", pid)))?;
        pcb.usage.agent_hops += 1;
        Ok(())
    }

    /// Cleanup stale orchestration sessions and their envelopes.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> usize {
        let removed_pids = self.orchestrator.cleanup_stale_sessions(max_age_seconds);
        let count = removed_pids.len();
        for pid in &removed_pids {
            self.runs.remove(pid);
        }
        count
    }

    /// Cleanup stale user usage entries.
    pub fn cleanup_stale_user_usage(&mut self, max_entries: usize) -> usize {
        let active_user_ids = self.lifecycle.get_active_user_ids();
        self.resources.cleanup_stale_users(&active_user_ids, max_entries)
    }

    /// Get a full system status snapshot.
    pub fn get_system_status(&self) -> SystemStatus {
        let total = self.lifecycle.count();
        let mut by_state = HashMap::new();
        for state in &[RunStatus::Ready, RunStatus::Running, RunStatus::Terminated] {
            by_state.insert(*state, self.lifecycle.count_by_state(*state));
        }

        let orchestrator_sessions = self.orchestrator.get_session_count();

        SystemStatus {
            processes_total: total,
            processes_by_state: by_state,
            active_orchestration_sessions: orchestrator_sessions,
        }
    }

    /// Get remaining resource budget for a process.
    pub fn get_remaining_budget(&self, pid: &RunId) -> Option<RemainingBudget> {
        let pcb = self.lifecycle.get(pid)?;
        Some(RemainingBudget {
            llm_calls_remaining: (pcb.quota.max_llm_calls - pcb.usage.llm_calls).max(0),
            iterations_remaining: (pcb.quota.max_iterations - pcb.usage.iterations).max(0),
            agent_hops_remaining: (pcb.quota.max_agent_hops - pcb.usage.agent_hops).max(0),
            tokens_in_remaining: (pcb.quota.max_input_tokens as i64 - pcb.usage.tokens_in).max(0),
            tokens_out_remaining: (pcb.quota.max_output_tokens as i64 - pcb.usage.tokens_out)
                .max(0),
            time_remaining_seconds: if pcb.quota.timeout_seconds > 0 {
                (pcb.quota.timeout_seconds as f64 - pcb.usage.elapsed_seconds).max(0.0)
            } else {
                f64::MAX
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::run::Run;
    use crate::kernel::orchestrator::{
        Instruction, Workflow, Stage,
    };
    use crate::kernel::Kernel;
    use crate::types::{RunId, RequestId, UserId, SessionId};

    fn test_stage(name: &str) -> Stage {
        Stage {
            name: name.to_string(),
            agent: name.to_string(),
            ..Stage::default()
        }
    }

    fn _test_config(stages: Vec<Stage>) -> Workflow {
        Workflow::test_default("test", stages)
    }

    fn _setup_kernel_with_process(kernel: &mut Kernel, pid: &RunId) {
        let _ = kernel.create_process(
            pid.clone(),
            RequestId::must("req-1"),
            UserId::must("user-1"),
            SessionId::must("sess-1"),
            None,
        );
    }

    // No tests remain in this module after the CommBus prune.
    // Helpers retained as `_`-prefixed for future reuse.
}
