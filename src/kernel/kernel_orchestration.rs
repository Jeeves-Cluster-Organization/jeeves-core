//! Kernel orchestration methods — initialize, get_next_instruction, process_agent_result.

use std::collections::HashMap;

use tracing::instrument;

use crate::envelope::{Envelope, FlowInterrupt};
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

use super::merge_state_field;
use super::orchestrator;
use super::orchestrator_types::ContextOverflow;
use super::{Kernel, ProcessState, RemainingBudget, ResourceQuota, SystemStatus};

impl Kernel {
    /// Initialize an orchestration session.
    ///
    /// Stores the envelope in `process_envelopes`, then initializes the pipeline
    /// session in the orchestrator. The orchestrator sets pipeline bounds on the
    /// envelope but does not own it.
    #[instrument(skip(self, pipeline_config, envelope), fields(process_id = %process_id))]
    pub fn initialize_orchestration(
        &mut self,
        process_id: ProcessId,
        pipeline_config: orchestrator::PipelineConfig,
        mut envelope: Envelope,
        force: bool,
    ) -> Result<orchestrator::SessionState> {
        // Wire tool access from pipeline stages (before pipeline_config is moved)
        for stage in &pipeline_config.stages {
            if let Some(ref tools) = stage.allowed_tools {
                self.tools.access.grant_many(&stage.agent, tools);
            }
        }

        let state = self.orchestrator
            .initialize_session(process_id.clone(), pipeline_config, &mut envelope, force)?;
        self.process_envelopes.insert(process_id, envelope);

        Ok(state)
    }

    /// Get the next instruction for a process.
    ///
    /// Extracts the envelope from `process_envelopes`, passes it to the orchestrator
    /// (which may mutate it for bounds termination), then re-stores it.
    #[instrument(skip(self), fields(process_id = %process_id))]
    pub fn get_next_instruction(
        &mut self,
        process_id: &ProcessId,
    ) -> Result<orchestrator::Instruction> {
        // Phase 1: Get instruction (needs &mut envelope)
        let envelope = self.process_envelopes.get_mut(process_id)
            .ok_or_else(|| Error::not_found(format!("Envelope not found for process: {}", process_id)))?;
        let mut instruction = self.orchestrator.get_next_instruction(process_id, envelope)?;
        // &mut envelope borrow ends here (we don't use it below)

        // Phase 2: Enrich instruction with context for the worker
        match &mut instruction {
            orchestrator::Instruction::RunAgent { agent, context }=> {
                let enrichment = self.build_enrichment_context(process_id);
                if let Some((agent_context, max_ctx, overflow)) = enrichment {
                    context.agent_context = Some(agent_context);
                    context.max_context_tokens = max_ctx;
                    context.context_overflow = overflow;
                }

                // Extract interrupt response from metadata into typed field (consume it)
                if let Some(env) = self.process_envelopes.get_mut(process_id) {
                    context.interrupt_response = env.audit.metadata.remove("_interrupt_response");
                }

                let stage_name = self.process_envelopes.get(process_id)
                    .map(|e| e.pipeline.current_stage.clone())
                    .unwrap_or_default();

                // Pass per-stage timeout + retry policy from stage config
                if let Some(sc) = self.orchestrator.get_stage_config(process_id, &stage_name) {
                    context.timeout_seconds = sc.timeout_seconds;
                    context.retry_policy = sc.retry_policy.clone();
                }

                context.output_schema = self.orchestrator.get_stage_output_schema(process_id, &stage_name);

                let allowed = self.tools.access.tools_for_agent(agent);
                if !allowed.is_empty() {
                    context.allowed_tools = Some(allowed);
                }
            }
            orchestrator::Instruction::RunAgents { agents, context } => {
                let enrichment = self.build_enrichment_context(process_id);
                if let Some((agent_context, max_ctx, overflow)) = enrichment {
                    context.agent_context = Some(agent_context);
                    context.max_context_tokens = max_ctx;
                    context.context_overflow = overflow;
                }

                if let Some(agent_name) = agents.first() {
                    let stage_name = self.process_envelopes.get(process_id)
                        .map(|e| e.pipeline.current_stage.clone())
                        .unwrap_or_default();
                    context.output_schema = self.orchestrator.get_stage_output_schema(process_id, &stage_name);

                    let allowed = self.tools.access.tools_for_agent(agent_name);
                    if !allowed.is_empty() {
                        context.allowed_tools = Some(allowed);
                    }
                }
            }
            orchestrator::Instruction::Terminate { context, .. } => {
                // Attach final outputs + aggregate metrics so the worker can return them
                if let Some(envelope) = self.process_envelopes.get(process_id) {
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

        // Phase 2b: Thread routing decision from last report_agent_result into this instruction's context
        if let Some(decision) = self.orchestrator.take_last_routing_decision(process_id) {
            match &mut instruction {
                orchestrator::Instruction::RunAgent { context, .. } |
                orchestrator::Instruction::Terminate { context, .. } => {
                    context.last_routing_decision = Some(decision);
                }
                orchestrator::Instruction::RunAgents { context, .. } => {
                    context.last_routing_decision = Some(decision);
                }
                _ => {}
            }
        }

        Ok(instruction)
    }

    /// Process a complete agent result: merge output, report to orchestrator,
    /// sync PCB counters, and emit snapshot.
    ///
    /// Mutation only — caller fetches the next instruction separately via
    /// `get_next_instruction()`. This decoupling prevents fork/parallel deadlocks.
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self, output, metrics), fields(process_id = %process_id))]
    pub fn process_agent_result(
        &mut self,
        process_id: &ProcessId,
        agent_name: &str,
        output: serde_json::Value,
        metadata_updates: Option<HashMap<String, serde_json::Value>>,
        metrics: orchestrator::AgentExecutionMetrics,
        success: bool,
        error_message: &str,
        break_loop: bool,
    ) -> Result<()> {
        // Extract scalars before passing metrics by value (avoids clone)
        let llm_calls = metrics.llm_calls;
        let tool_calls = metrics.tool_calls;
        let tokens_in = metrics.tokens_in.unwrap_or(0);
        let tokens_out = metrics.tokens_out.unwrap_or(0);
        let duration_ms = metrics.duration_ms;

        // Record per-tool health from agent metrics
        for tool_result in &metrics.tool_results {
            self.tools.health.record_execution(&tool_result.name, tool_result.success, tool_result.latency_ms, tool_result.error_type.clone());
        }

        // Clone state_schema + output_key from orchestrator before mutable envelope borrow
        let state_schema = self.orchestrator.get_state_schema(process_id).cloned().unwrap_or_default();
        let output_key = self.orchestrator.get_stage_output_key(process_id, agent_name)
            .unwrap_or_else(|| agent_name.to_string());

        // Phase 1: Merge output into envelope + run orchestrator
        let effective_failed = {
            let envelope = self.process_envelopes.get_mut(process_id)
                .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", process_id)))?;

            // Build agent output map
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

            // State merge: write to state[output_key] per state_schema
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

            // Merge metadata updates from agent hooks
            if let Some(meta_updates) = metadata_updates {
                for (key, value) in meta_updates {
                    envelope.audit.metadata.insert(key, value);
                }
            }

            // Validate output against schema
            let mut agent_failed_override = false;
            {
                let schema = self.orchestrator.get_stage_output_schema(process_id, &envelope.pipeline.current_stage);
                if let Some(ref schema_val) = schema {
                    let output_value = serde_json::Value::Object(
                        envelope.outputs.get(agent_name)
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default()
                    );
                    if !jsonschema::is_valid(schema_val, &output_value) {
                        tracing::warn!(agent = %agent_name, "output_schema_validation_failed");
                        if let Some(agent_out) = envelope.outputs.get_mut(agent_name) {
                            agent_out.insert("_schema_validation_error".to_string(),
                                serde_json::Value::String("Output does not match declared output_schema".to_string()));
                        }
                        agent_failed_override = true;
                    }
                }
            }

            // Validate tool access
            if let Some(tool_calls_arr) = envelope.outputs.get(agent_name)
                .and_then(|out| out.get("tool_calls"))
                .and_then(|v| v.as_array())
            {
                for tc in tool_calls_arr {
                    if let Some(tool_name) = tc.get("name").or_else(|| tc.get("tool_name")).and_then(|v| v.as_str()) {
                        if !self.tools.access.check_access(agent_name, tool_name) {
                            tracing::warn!(agent = %agent_name, tool = %tool_name, "unauthorized_tool_call");
                            agent_failed_override = true;
                        }
                    }
                }
            }

            let effective_failed = !success || agent_failed_override;

            // Report to orchestrator (consumes metrics, adds to envelope, evaluates routing)
            self.orchestrator.report_agent_result(process_id, agent_name, metrics, envelope, effective_failed, break_loop)?;

            // Activate audit trail: record per-stage processing history
            let now = chrono::Utc::now();
            envelope.audit.processing_history.push(crate::envelope::ProcessingRecord {
                agent: agent_name.to_string(),
                stage_order: envelope.pipeline.current_stage_number,
                started_at: now - chrono::Duration::milliseconds(duration_ms),
                completed_at: Some(now),
                duration_ms: duration_ms as i32,
                status: if !effective_failed { crate::envelope::ProcessingStatus::Success } else { crate::envelope::ProcessingStatus::Error },
                error: if error_message.is_empty() { None } else { Some(error_message.to_string()) },
                llm_calls,
                tool_calls,
                tokens_in,
                tokens_out,
            });

            effective_failed
        }; // envelope borrow dropped

        let _ = effective_failed; // suppress unused warning

        // Phase 2: Apply SAME metrics delta directly to PCB
        {
            let user_id = self.lifecycle.get(process_id).map(|p| p.user_id.as_str().to_string());
            if let Some(uid) = user_id {
                self.record_usage(process_id, &uid, llm_calls, tool_calls, tokens_in, tokens_out);
            }
        }

        Ok(())
    }

    /// Get orchestration session state.
    pub fn get_orchestration_state(
        &self,
        process_id: &ProcessId,
    ) -> Result<orchestrator::SessionState> {
        let envelope = self.process_envelopes.get(process_id)
            .ok_or_else(|| Error::not_found(format!("Envelope not found for process: {}", process_id)))?;
        self.orchestrator.get_session_state(process_id, envelope)
    }

    // =============================================================================
    // Enrichment (private helpers)
    // =============================================================================

    /// Build the enrichment context for agent dispatch instructions.
    ///
    /// Reads the envelope, builds template_vars, constructs the agent_context JSON,
    /// reads stage config for max_context_tokens/context_overflow, and returns all three.
    fn build_enrichment_context(
        &self,
        process_id: &ProcessId,
    ) -> Option<(serde_json::Value, Option<i64>, Option<ContextOverflow>)> {
        let envelope = self.process_envelopes.get(process_id)?;

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
            .get_stage_config(process_id, &stage_name)
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

    // =============================================================================
    // Process Lifecycle (cross-domain coordination)
    // =============================================================================

    /// Create a new process.
    #[instrument(skip(self), fields(pid = %pid))]
    pub fn create_process(
        &mut self,
        pid: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        quota: Option<ResourceQuota>,
    ) -> Result<super::ProcessControlBlock> {
        self.lifecycle.create(pid, request_id, user_id, session_id, quota)
    }

    /// Check process quota.
    pub fn check_quota(&self, pid: &ProcessId) -> Result<()> {
        let pcb = self
            .lifecycle
            .get(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
        self.resources.check_quota(pcb)
    }

    /// Record resource usage.
    pub fn record_usage(
        &mut self,
        pid: &ProcessId,
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
    pub fn set_process_interrupt(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        // Register in interrupt manager (so resolve_interrupt can find it by ID)
        let interrupt_id = interrupt.id.clone();
        if let Some(env) = self.process_envelopes.get(pid) {
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
        let env = self.process_envelopes.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", pid)))?;
        env.set_interrupt(interrupt);
        Ok(())
    }

    /// Resolve a pending interrupt and stash the response for the next agent dispatch.
    pub fn resolve_process_interrupt(
        &mut self,
        pid: &ProcessId,
        interrupt_id: &str,
        response: crate::envelope::InterruptResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_value(&response).unwrap_or_default();
        if !self.interrupts.resolve(interrupt_id, response, None) {
            return Err(Error::not_found(format!("Interrupt {} not found", interrupt_id)));
        }

        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.audit.metadata.insert("_interrupt_response".to_string(), response_json);
            env.clear_interrupt();
        }
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.pending_interrupt = None;
        }
        Ok(())
    }

    /// Terminate a process and remove it from the kernel.
    pub fn terminate_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.terminate("Process terminated");
        }
        self.process_envelopes.remove(pid);
        self.orchestrator.cleanup_session(pid);
        Ok(())
    }

    /// Record a tool call for a process.
    pub fn record_tool_call(&mut self, pid: &ProcessId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_tool_call", pid)))?;
        pcb.usage.tool_calls += 1;
        Ok(())
    }

    /// Record an agent hop for a process.
    pub fn record_agent_hop(&mut self, pid: &ProcessId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_agent_hop", pid)))?;
        pcb.usage.agent_hops += 1;
        Ok(())
    }

    // =============================================================================
    // Cleanup (cross-domain coordination)
    // =============================================================================

    /// Cleanup stale orchestration sessions and their envelopes.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> usize {
        let removed_pids = self.orchestrator.cleanup_stale_sessions(max_age_seconds);
        let count = removed_pids.len();
        for pid in &removed_pids {
            self.process_envelopes.remove(pid);
        }
        count
    }

    /// Cleanup stale user usage entries.
    pub fn cleanup_stale_user_usage(&mut self, max_entries: usize) -> usize {
        let active_user_ids = self.lifecycle.get_active_user_ids();
        self.resources.cleanup_stale_users(&active_user_ids, max_entries)
    }

    // =============================================================================
    // System Status (cross-domain aggregation)
    // =============================================================================

    /// Get a full system status snapshot.
    pub fn get_system_status(&self) -> SystemStatus {
        let total = self.lifecycle.count();
        let mut by_state = HashMap::new();
        for state in &[ProcessState::Ready, ProcessState::Running, ProcessState::Terminated] {
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
    pub fn get_remaining_budget(&self, pid: &ProcessId) -> Option<RemainingBudget> {
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
    use crate::envelope::Envelope;
    use crate::kernel::orchestrator::{
        Instruction, PipelineConfig, PipelineStage,
    };
    use crate::kernel::Kernel;
    use crate::types::{ProcessId, RequestId, UserId, SessionId};

    fn test_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            ..PipelineStage::default()
        }
    }

    fn _test_config(stages: Vec<PipelineStage>) -> PipelineConfig {
        PipelineConfig::test_default("test", stages)
    }

    fn _setup_kernel_with_process(kernel: &mut Kernel, pid: &ProcessId) {
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
