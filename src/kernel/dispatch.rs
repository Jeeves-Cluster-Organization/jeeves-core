//! Kernel orchestration methods — initialize, get_next_instruction, process_agent_result.

use std::collections::HashMap;

use tracing::instrument;

use crate::agent::policy::ContextOverflow;
use crate::run::{Run, FlowInterrupt};
use crate::types::{Error, RunId, RequestId, Result, SessionId, UserId};

use super::merge_state_field;
use super::orchestrator;
use super::{Kernel, RunStatus, RemainingBudget, ResourceQuota, SystemStatus};

impl Kernel {
    /// Stores `run` in `runs` and hands it to the orchestrator
    /// to seed the session. The orchestrator updates the run's workflow
    /// bounds in place; ownership stays with `runs`.
    #[instrument(skip(self, workflow, run), fields(run_id = %run_id))]
    pub fn initialize_orchestration(
        &mut self,
        run_id: RunId,
        workflow: orchestrator::Workflow,
        mut run: Run,
        force: bool,
    ) -> Result<orchestrator::RunSnapshot> {
        let state = self.orchestrator
            .initialize_session(run_id.clone(), workflow, &mut run, force)?;
        self.runs.insert(run_id, run);

        Ok(state)
    }

    /// Fetches and enriches the next instruction for `run_id`. The
    /// orchestrator may mutate the run on its way to a `Terminate`
    /// (bounds, errors); enrichment then layers in agent context, stage
    /// timeouts, response format, and any routing decision from the previous
    /// `report_agent_result`.
    #[instrument(skip(self), fields(run_id = %run_id))]
    pub fn get_next_instruction(
        &mut self,
        run_id: &RunId,
    ) -> Result<orchestrator::Instruction> {
        let run = self.runs.get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Run not found for run_id: {}", run_id)))?;
        let mut instruction = self.orchestrator.get_next_instruction(run_id, run)?;

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
                    .map(|e| e.current_stage.clone())
                    .unwrap_or_default();

                if let Some(sc) = self.orchestrator.get_stage_config(run_id, stage_name.as_str()) {
                    context.timeout_seconds = sc.timeout_seconds;
                    context.retry_policy = sc.retry_policy.clone();
                }

                context.response_format = self.orchestrator.get_stage_response_format(run_id, stage_name.as_str());
            }
            orchestrator::Instruction::Terminate { context, .. } => {
                if let Some(run) = self.runs.get(run_id) {
                    let total_duration_ms = (chrono::Utc::now() - run.audit.created_at)
                        .num_milliseconds();
                    context.agent_context = Some(serde_json::json!({
                        "outputs": &run.outputs,
                        "aggregate_metrics": {
                            "total_duration_ms": total_duration_ms,
                            "total_llm_calls": run.metrics.llm_calls,
                            "total_tool_calls": run.metrics.tool_calls,
                            "total_tokens_in": run.metrics.tokens_in,
                            "total_tokens_out": run.metrics.tokens_out,
                            "stages_executed": &run.stage_order,
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

    /// Merges an agent's output into the run, reports it to the
    /// orchestrator, and applies the metrics delta to the run record. The
    /// caller pulls the next instruction separately — the split is what
    /// keeps fork/parallel paths deadlock-free.
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

        // Lift state_schema + output_key out before the &mut run borrow.
        let state_schema = self.orchestrator.get_state_schema(run_id).cloned().unwrap_or_default();
        let output_key = self.orchestrator.get_stage_output_key(run_id, agent_name)
            .unwrap_or_else(|| agent_name.to_string());

        {
            let run = self.runs.get_mut(run_id)
                .ok_or_else(|| Error::not_found(format!("Run not found: {}", run_id)))?;

            let mut agent_output: std::collections::HashMap<crate::types::OutputKey, serde_json::Value> = std::collections::HashMap::new();
            if let serde_json::Value::Object(output_map) = output {
                for (key, value) in output_map {
                    agent_output.insert(key.as_str().into(), value);
                }
            }
            if !success {
                agent_output.insert("success".into(), serde_json::Value::Bool(false));
                if !error_message.is_empty() {
                    agent_output.insert("error".into(), serde_json::Value::String(error_message.to_string()));
                }
                run.audit.metadata.insert(
                    "last_agent_failure".to_string(),
                    serde_json::json!({
                        "agent_name": agent_name,
                        "error": error_message,
                    }),
                );
            }
            run.outputs.insert(agent_name.into(), agent_output);

            let mut state_matched = false;
            for field in &state_schema {
                if field.key == output_key {
                    let output_value = serde_json::Value::Object(
                        run.outputs.get(agent_name)
                            .map(|m| m.iter().map(|(k, v)| (k.as_str().to_string(), v.clone())).collect())
                            .unwrap_or_default()
                    );
                    merge_state_field(&mut run.state, &field.key, output_value, field.merge);
                    state_matched = true;
                    break;
                }
            }
            if !state_schema.is_empty() && !state_matched {
                tracing::debug!(output_key = %output_key, "output_key has no matching state_schema entry");
            }

            if let Some(meta_updates) = metadata_updates {
                for (key, value) in meta_updates {
                    run.audit.metadata.insert(key, value);
                }
            }

            let effective_failed = !success;
            self.orchestrator.report_agent_result(run_id, agent_name, metrics, run, effective_failed, break_loop)?;

            let now = chrono::Utc::now();
            run.audit.processing_history.push(crate::run::ProcessingRecord {
                agent: agent_name.to_string(),
                stage_order: run
                    .stage_order
                    .iter()
                    .position(|s| s == &run.current_stage)
                    .map(|p| (p + 1) as i32)
                    .unwrap_or(0),
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
            self.record_user_usage(&uid, llm_calls, tool_calls, tokens_in, tokens_out);
        }

        Ok(())
    }

    /// Get orchestration session state.
    pub fn get_orchestration_state(
        &self,
        run_id: &RunId,
    ) -> Result<orchestrator::RunSnapshot> {
        let run = self.runs.get(run_id)
            .ok_or_else(|| Error::not_found(format!("Run not found: {}", run_id)))?;
        self.orchestrator.get_session_state(run_id, run)
    }

    /// Reads the run and stage config, packs them into the JSON shape
    /// the worker expects, and returns it alongside the per-stage context-window
    /// bounds.
    fn build_enrichment_context(
        &self,
        run_id: &RunId,
    ) -> Option<(serde_json::Value, Option<i64>, Option<ContextOverflow>)> {
        let run = self.runs.get(run_id)?;

        let mut template_vars = serde_json::Map::new();
        for (agent_name, output) in &run.outputs {
            for (key, value) in output {
                template_vars.insert(format!("{}_{}", agent_name, key), value.clone());
            }
        }
        for (key, value) in &run.audit.metadata {
            template_vars.insert(key.clone(), value.clone());
        }

        let agent_context = serde_json::json!({
            "envelope_id": run.identity.envelope_id.as_str(),
            "request_id": run.identity.request_id.as_str(),
            "user_id": run.identity.user_id.as_str(),
            "session_id": run.identity.session_id.as_str(),
            "raw_input": &run.raw_input,
            "outputs": &run.outputs,
            "state": &run.state,
            "metadata": &run.audit.metadata,
            "template_vars": serde_json::Value::Object(template_vars),
            "llm_call_count": run.metrics.llm_calls,
            "agent_hop_count": run.metrics.agent_hops,
            "tokens_in": run.metrics.tokens_in,
            "tokens_out": run.metrics.tokens_out,
            "circuit_broken_tools": self.tools.health.get_circuit_broken_tools(),
        });

        let stage_name = run.current_stage.clone();
        let (max_context_tokens, context_overflow) = self.orchestrator
            .get_stage_config(run_id, stage_name.as_str())
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

    /// Create a new run record.
    #[instrument(skip(self), fields(run_id = %run_id))]
    pub fn create_run(
        &mut self,
        run_id: RunId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        quota: Option<ResourceQuota>,
    ) -> Result<super::RunRecord> {
        self.lifecycle.create(run_id, request_id, user_id, session_id, quota)
    }

    /// Check whether the run has exceeded its quota. Reads live counters from
    /// `Run.metrics` + `Run.iteration`, the wall-clock elapsed from
    /// `RunRecord.started_at`, and bounds from `RunRecord.quota` — one source
    /// of truth per dimension.
    pub fn check_quota(&self, run_id: &RunId) -> Result<()> {
        let record = self
            .lifecycle
            .get(run_id)
            .ok_or_else(|| Error::not_found(format!("Run {} not found", run_id)))?;
        let usage = self.usage_from_run(run_id, record);
        if let Some(violation) = usage.exceeds_quota(&record.quota) {
            return Err(Error::quota_exceeded(format!(
                "Run {} quota exceeded: {}",
                run_id, violation
            )));
        }
        Ok(())
    }

    /// Snapshot of usage built from `Run.metrics` + elapsed wall-clock. The
    /// kernel doesn't store this — it's derived on demand by `check_quota` and
    /// `get_remaining_budget`.
    fn usage_from_run(&self, run_id: &RunId, record: &super::RunRecord) -> super::ResourceUsage {
        let run = self.runs.get(run_id);
        super::ResourceUsage {
            llm_calls: run.map_or(0, |r| r.metrics.llm_calls),
            tool_calls: run.map_or(0, |r| r.metrics.tool_calls),
            agent_hops: run.map_or(0, |r| r.metrics.agent_hops),
            iterations: run.map_or(0, |r| r.iteration),
            tokens_in: run.map_or(0, |r| r.metrics.tokens_in),
            tokens_out: run.map_or(0, |r| r.metrics.tokens_out),
            elapsed_seconds: record.elapsed_seconds(),
        }
    }

    /// Accumulate per-user usage in the cross-run tracker. Per-run counters
    /// live on `Run.metrics` and are updated by `Orchestrator::report_agent_result`.
    pub fn record_user_usage(
        &mut self,
        user_id: &str,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    ) {
        self.resources
            .record_usage(user_id, llm_calls, tool_calls, tokens_in, tokens_out);
    }

    /// Set a tool-confirmation interrupt on a run. The workflow loop
    /// suspends the stage; the consumer resolves via `resolve_run_interrupt`.
    pub fn set_run_interrupt(&mut self, run_id: &RunId, interrupt: FlowInterrupt) -> Result<()> {
        // Register in interrupt manager (so resolve_interrupt can find it by ID)
        let interrupt_id = interrupt.id.clone();
        if let Some(run) = self.runs.get(run_id) {
            self.interrupts.register_flow_interrupt(
                interrupt.clone(),
                &run.identity.request_id,
                &run.identity.user_id,
                &run.identity.session_id,
                &run.identity.envelope_id,
            );
        }

        // Mark on the run record so resolve_interrupt can see it.
        if let Some(record) = self.lifecycle.get_mut(run_id) {
            record.pending_interrupt = Some(interrupt_id);
        }

        // Set on run (get_next_instruction will see it → WaitInterrupt)
        let run = self.runs.get_mut(run_id)
            .ok_or_else(|| Error::not_found(format!("Run not found: {}", run_id)))?;
        run.set_interrupt(interrupt);
        Ok(())
    }

    /// Resolve a pending interrupt and stash the response for the next agent dispatch.
    pub fn resolve_run_interrupt(
        &mut self,
        run_id: &RunId,
        interrupt_id: &str,
        response: crate::run::InterruptResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_value(&response).unwrap_or_default();
        if !self.interrupts.resolve(interrupt_id, response) {
            return Err(Error::not_found(format!("Interrupt {} not found", interrupt_id)));
        }

        if let Some(run) = self.runs.get_mut(run_id) {
            run.audit.metadata.insert("_interrupt_response".to_string(), response_json);
            run.clear_interrupt();
        }
        if let Some(record) = self.lifecycle.get_mut(run_id) {
            record.pending_interrupt = None;
        }
        Ok(())
    }

    /// Terminate a run and remove it from the kernel.
    pub fn terminate_run(&mut self, run_id: &RunId) -> Result<()> {
        self.lifecycle.terminate(run_id)?;
        if let Some(run) = self.runs.get_mut(run_id) {
            run.complete("Run terminated");
        }
        self.runs.remove(run_id);
        self.orchestrator.cleanup_session(run_id);
        Ok(())
    }

    /// Cleanup stale orchestration sessions and their runs.
    /// Returns the count of sessions removed.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> usize {
        let removed = self.orchestrator.cleanup_stale_sessions(max_age_seconds);
        let count = removed.len();
        for run_id in &removed {
            self.runs.remove(run_id);
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
            runs_total: total,
            runs_by_state: by_state,
            active_orchestration_sessions: orchestrator_sessions,
        }
    }

    /// Get remaining resource budget for a run.
    pub fn get_remaining_budget(&self, run_id: &RunId) -> Option<RemainingBudget> {
        let record = self.lifecycle.get(run_id)?;
        let usage = self.usage_from_run(run_id, record);
        Some(RemainingBudget {
            llm_calls_remaining: (record.quota.max_llm_calls - usage.llm_calls).max(0),
            iterations_remaining: (record.quota.max_iterations - usage.iterations).max(0),
            agent_hops_remaining: (record.quota.max_agent_hops - usage.agent_hops).max(0),
            tokens_in_remaining: (record.quota.max_input_tokens as i64 - usage.tokens_in).max(0),
            tokens_out_remaining: (record.quota.max_output_tokens as i64 - usage.tokens_out).max(0),
            time_remaining_seconds: if record.quota.timeout_seconds > 0 {
                (record.quota.timeout_seconds as f64 - usage.elapsed_seconds).max(0.0)
            } else {
                f64::MAX
            },
        })
    }
}

