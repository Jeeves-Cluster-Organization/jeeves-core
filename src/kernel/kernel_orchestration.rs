//! Kernel orchestration methods — initialize, get_next_instruction, process_agent_result.

use std::collections::HashMap;

use tracing::instrument;

use crate::envelope::{Envelope, FlowInterrupt};
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

use super::merge_state_field;
use super::orchestrator;
use super::orchestrator_types::{ContextOverflow, NodeKind};
use super::routing::evaluate_expr;
use super::{Kernel, ProcessState, RemainingBudget, ResourceQuota, SchedulingPriority, SystemStatus};

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

        // Wire CommBus subscriptions from pipeline config (before pipeline_config is moved)
        let subscription_types = pipeline_config.subscriptions.clone();

        let state = self.orchestrator
            .initialize_session(process_id.clone(), pipeline_config, &mut envelope, force)?;
        self.process_envelopes.insert(process_id.clone(), envelope);

        // Subscribe to CommBus event types if configured
        if !subscription_types.is_empty() {
            let subscriber_id = format!("pipeline:{}", process_id);
            match self.comm.bus.subscribe(subscriber_id, subscription_types) {
                Ok((subscription, receiver)) => {
                    self.comm.subscriptions
                        .entry(process_id)
                        .or_default()
                        .push((subscription, receiver));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to wire CommBus subscriptions");
                }
            }
        }

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
        // Phase 0: Drain CommBus subscription receivers into envelope.event_inbox
        const MAX_EVENT_INBOX: usize = 1024;
        if let Some(subs) = self.comm.subscriptions.get_mut(process_id) {
            if let Some(envelope) = self.process_envelopes.get_mut(process_id) {
                for (_subscription, receiver) in subs.iter_mut() {
                    while let Ok(event) = receiver.try_recv() {
                        if envelope.event_inbox.len() < MAX_EVENT_INBOX {
                            envelope.event_inbox.push(event);
                        } else {
                            tracing::warn!(process_id = %process_id, "event_inbox full, dropping events");
                            break;
                        }
                    }
                }
            }
        }

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

                // Router enrichment: evaluate when-conditions, filter targets, override output_schema
                if let Some(stage_config) = self.orchestrator.get_stage_config(process_id, &stage_name) {
                    if stage_config.node_kind == NodeKind::Router {
                        let envelope = self.process_envelopes.get(process_id);
                        let empty_outputs = HashMap::new();
                        let empty_meta = HashMap::new();
                        let empty_state = HashMap::new();

                        let (outputs, metadata, state) = envelope
                            .map(|e| (&e.outputs, &e.audit.metadata, &e.state))
                            .unwrap_or((&empty_outputs, &empty_meta, &empty_state));

                        let available: Vec<_> = stage_config.router_targets.iter()
                            .filter(|rt| {
                                rt.when.as_ref().map_or(true, |expr| {
                                    evaluate_expr(expr, outputs, &stage_config.agent, metadata, None, state)
                                })
                            })
                            .collect();

                        // Auto-generate output_schema with enum of available target names
                        let target_names: Vec<&str> = available.iter().map(|t| t.target.as_str()).collect();
                        context.output_schema = Some(serde_json::json!({
                            "type": "object",
                            "properties": {
                                "chosen_target": {
                                    "type": "string",
                                    "enum": target_names,
                                    "description": "The target to route to"
                                }
                            },
                            "required": ["chosen_target"],
                            "additionalProperties": false
                        }));

                        // Inject available_targets into metadata for prompt template vars
                        let targets_json: Vec<serde_json::Value> = available.iter()
                            .map(|rt| serde_json::json!({
                                "name": rt.target,
                                "description": rt.description,
                            }))
                            .collect();

                        if let Some(ref mut ctx_val) = context.agent_context {
                            if let Some(obj) = ctx_val.as_object_mut() {
                                if let Some(meta) = obj.get_mut("metadata") {
                                    if let Some(meta_obj) = meta.as_object_mut() {
                                        meta_obj.insert("available_targets".to_string(), serde_json::json!(targets_json));
                                    }
                                }
                            }
                        }
                    }
                }

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
            orchestrator::Instruction::Terminate { context, reason, .. } => {
                // Attach final outputs so the worker can return them
                if let Some(envelope) = self.process_envelopes.get(process_id) {
                    context.agent_context = Some(serde_json::json!({
                        "outputs": &envelope.outputs,
                    }));
                }

                // Publish completion events for each type in pipeline.publishes
                // Scope block: collect owned data, drop immutable borrows before &mut self
                let to_publish: Option<(Vec<String>, Vec<u8>)> = {
                    let publishes = self.orchestrator.get_pipeline_publishes(process_id);
                    match publishes {
                        Some(p) if !p.is_empty() => {
                            let types = p.to_vec();
                            let outputs_json = self.process_envelopes.get(process_id)
                                .map(|e| serde_json::to_value(&e.outputs).unwrap_or_default());
                            let payload = serde_json::json!({
                                "process_id": process_id.as_str(),
                                "terminal_reason": format!("{:?}", reason),
                                "outputs": outputs_json,
                            });
                            Some((types, serde_json::to_vec(&payload).unwrap_or_default()))
                        }
                        _ => None,
                    }
                };
                if let Some((types, payload_bytes)) = to_publish {
                    for event_type in types {
                        let event = crate::commbus::Event {
                            event_type,
                            payload: payload_bytes.clone(),
                            timestamp_ms: chrono::Utc::now().timestamp_millis(),
                            source: format!("pipeline:{}", process_id),
                        };
                        let _ = self.publish_event(event);
                    }
                }
            }
            _ => {}
        }

        // Phase 3: Clear event_inbox after enrichment (events now in agent_context)
        if let Some(envelope) = self.process_envelopes.get_mut(process_id) {
            envelope.event_inbox.clear();
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

        // Phase 3: Snapshot
        self.emit_envelope_snapshot(process_id, "agent_completed");
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
            "pending_events": &envelope.event_inbox,
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
        priority: SchedulingPriority,
        quota: Option<ResourceQuota>,
    ) -> Result<super::ProcessControlBlock> {
        self.rate_limiter.check_rate_limit(user_id.as_str())?;
        let pcb = self.lifecycle.submit(
            pid.clone(),
            request_id,
            user_id,
            session_id,
            priority,
            quota,
        )?;
        self.lifecycle.schedule(&pid)?;
        Ok(pcb)
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

    /// Wait a process (e.g., awaiting interrupt response).
    ///
    /// Requires lifecycle state = Running. For pipeline-loop interrupt flow
    /// (where lifecycle stays Ready), use `set_process_interrupt` instead.
    pub fn wait_process(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        self.lifecycle.wait(pid, interrupt.kind)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.set_interrupt(interrupt);
        }
        Ok(())
    }

    /// Set an interrupt on a process without lifecycle state transition.
    ///
    /// Used by the worker pipeline loop, which stays in Ready state throughout
    /// execution. Sets the interrupt on the envelope and registers it in the
    /// interrupt manager so `resolve_interrupt` can find it.
    pub fn set_process_interrupt(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        // Register in interrupt manager (so resolve_interrupt can find it by ID)
        if let Some(env) = self.process_envelopes.get(pid) {
            self.interrupts.register_flow_interrupt(
                interrupt.clone(),
                env.identity.request_id.as_str(),
                env.identity.user_id.as_str(),
                env.identity.session_id.as_str(),
                env.identity.envelope_id.as_str(),
            );
        }

        // Set on envelope (get_next_instruction will see it → WaitInterrupt)
        let env = self.process_envelopes.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Envelope not found: {}", pid)))?;
        env.set_interrupt(interrupt);
        Ok(())
    }

    /// Resume a process from waiting/blocked (lifecycle transition).
    ///
    /// For pipeline-loop processes (lifecycle stays Ready), use
    /// `clear_process_interrupt` instead.
    pub fn resume_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.resume(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.clear_interrupt();
        }
        Ok(())
    }

    /// Clear an interrupt on a process without lifecycle state transition.
    ///
    /// Counterpart to `set_process_interrupt`. Used when resolving interrupts
    /// for pipeline-loop processes that never leave Ready state.
    pub fn clear_process_interrupt(&mut self, pid: &ProcessId) -> Result<()> {
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.clear_interrupt();
        }
        Ok(())
    }

    /// Resolve an interrupt on a process, regardless of how it was created.
    ///
    /// Handles both lifecycle-managed interrupts (process in Waiting/Blocked state)
    /// and pipeline-loop interrupts (process stays in Ready state). Stores the
    /// interrupt response in envelope metadata for the next agent dispatch.
    pub fn resolve_process_interrupt(
        &mut self,
        pid: &ProcessId,
        interrupt_id: &str,
        response: crate::envelope::InterruptResponse,
    ) -> Result<()> {
        // 1. Resolve in interrupt manager
        let response_json = serde_json::to_value(&response).unwrap_or_default();
        if !self.interrupts.resolve(interrupt_id, response, None) {
            return Err(Error::not_found(format!("Interrupt {} not found", interrupt_id)));
        }

        // 2. Store response for the re-dispatched agent (extracted by get_next_instruction)
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.audit.metadata.insert("_interrupt_response".to_string(), response_json);
        }

        // 3. Clear interrupt + resume lifecycle if applicable
        let is_lifecycle_managed = self.lifecycle.get(pid)
            .map(|p| matches!(p.state, ProcessState::Waiting | ProcessState::Blocked))
            .unwrap_or(false);

        if is_lifecycle_managed {
            self.resume_process(pid)
        } else {
            self.clear_process_interrupt(pid)
        }
    }

    /// Resume a process with optional envelope updates.
    pub fn resume_process_with_update(
        &mut self,
        pid: &ProcessId,
        envelope_update: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<()> {
        self.lifecycle.resume(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.clear_interrupt();
            if let Some(updates) = envelope_update {
                env.merge_updates(updates);
            }
        }
        Ok(())
    }

    /// Terminate a process.
    pub fn terminate_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.terminate("Process terminated");
        }
        // Unsubscribe CommBus subscriptions and drop receivers
        if let Some(subs) = self.comm.subscriptions.remove(pid) {
            for (subscription, _receiver) in subs {
                self.comm.bus.unsubscribe(&subscription);
            }
        }
        Ok(())
    }

    /// Cleanup and remove a terminated process.
    pub fn cleanup_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.cleanup(pid)?;
        self.lifecycle.remove(pid)?;
        self.process_envelopes.remove(pid);
        self.orchestrator.cleanup_session(pid);
        // Cleanup any remaining subscriptions (belt-and-suspenders)
        if let Some(subs) = self.comm.subscriptions.remove(pid) {
            for (subscription, _receiver) in subs {
                self.comm.bus.unsubscribe(&subscription);
            }
        }
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
        for state in &[
            ProcessState::New,
            ProcessState::Ready,
            ProcessState::Running,
            ProcessState::Waiting,
            ProcessState::Blocked,
            ProcessState::Terminated,
            ProcessState::Zombie,
        ] {
            by_state.insert(*state, self.lifecycle.count_by_state(*state));
        }

        let service_stats = self.services.get_stats();
        let orchestrator_sessions = self.orchestrator.get_session_count();

        SystemStatus {
            processes_total: total,
            processes_by_state: by_state,
            services_healthy: service_stats.healthy_services,
            services_degraded: service_stats.degraded_services,
            services_unhealthy: service_stats.unhealthy_services,
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
    use crate::kernel::{Kernel, SchedulingPriority};
    use crate::types::{ProcessId, RequestId, UserId, SessionId};

    fn test_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            ..PipelineStage::default()
        }
    }

    fn test_config(stages: Vec<PipelineStage>) -> PipelineConfig {
        PipelineConfig {
            name: "test".to_string(),
            stages,
            max_iterations: 10,
            max_llm_calls: 10,
            max_agent_hops: 10,
            edge_limits: vec![],
            step_limit: None,
            state_schema: vec![],
            subscriptions: vec![],
            publishes: vec![],
        }
    }

    fn setup_kernel_with_process(kernel: &mut Kernel, pid: &ProcessId) {
        let _ = kernel.create_process(
            pid.clone(),
            RequestId::must("req-1"),
            UserId::must("user-1"),
            SessionId::must("sess-1"),
            SchedulingPriority::Normal,
            None,
        );
    }

    // =========================================================================
    // CommBus Subscription Wiring Tests
    // =========================================================================

    #[test]
    fn test_initialize_orchestration_wires_subscriptions() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-sub-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["npc.dialogue".to_string(), "game.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);

        let result = kernel.initialize_orchestration(pid.clone(), config, envelope, false);
        assert!(result.is_ok());

        // Subscriptions should be stored
        assert!(kernel.comm.subscriptions.contains_key(&pid));
        let subs = kernel.comm.subscriptions.get(&pid).unwrap();
        assert_eq!(subs.len(), 1); // One subscribe call with multiple event types
    }

    #[test]
    fn test_no_subscriptions_when_config_empty() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-nosub");
        setup_kernel_with_process(&mut kernel, &pid);

        let config = test_config(vec![test_stage("agent_a")]);
        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);

        let _state = kernel.initialize_orchestration(pid.clone(), config, envelope, false);

        // No subscriptions should be created
        assert!(!kernel.comm.subscriptions.contains_key(&pid));
    }

    #[test]
    fn test_event_inbox_drain_on_get_next_instruction() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-drain-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["test.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _state = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        // Publish an event to the CommBus (will be delivered to our subscription)
        let event = crate::commbus::Event {
            event_type: "test.event".to_string(),
            payload: b"{\"msg\":\"world\"}".to_vec(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            source: "external".to_string(),
        };
        kernel.comm.bus.publish(event).unwrap();

        // get_next_instruction should drain events into envelope.event_inbox
        let instruction = kernel.get_next_instruction(&pid).unwrap();
        assert!(matches!(instruction, Instruction::RunAgent { .. }));

        // Event inbox should be cleared after get_next_instruction
        let envelope = kernel.process_envelopes.get(&pid).unwrap();
        assert!(envelope.event_inbox.is_empty(), "event_inbox should be cleared after drain");
    }

    #[test]
    fn test_terminate_process_cleans_up_subscriptions() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-cleanup-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["test.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _state = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        assert!(kernel.comm.subscriptions.contains_key(&pid));

        // Terminate should clean up subscriptions
        kernel.terminate_process(&pid).unwrap();
        assert!(!kernel.comm.subscriptions.contains_key(&pid));
    }

    #[test]
    fn test_pending_events_in_agent_context() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-ctx-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["npc.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _state = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        // Publish two events
        for i in 0..2 {
            let event = crate::commbus::Event {
                event_type: "npc.event".to_string(),
                payload: format!("{{\"n\":{}}}", i).into_bytes(),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                source: "game".to_string(),
            };
            kernel.comm.bus.publish(event).unwrap();
        }

        let instruction = kernel.get_next_instruction(&pid).unwrap();
        // Agent context should include pending_events
        let context = match &instruction {
            Instruction::RunAgent { context, .. } => context,
            _ => panic!("Expected RunAgent"),
        };
        let ctx = context.agent_context.as_ref().unwrap();
        let pending = ctx.get("pending_events").unwrap().as_array().unwrap();
        assert_eq!(pending.len(), 2);
    }

    // =========================================================================
    // Pipeline Publishes Tests
    // =========================================================================

    #[test]
    fn test_pipeline_publishes_on_terminate() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-pub-1");
        setup_kernel_with_process(&mut kernel, &pid);

        // Subscribe to the event type before pipeline init
        let (_sub, mut rx) = kernel.comm.bus
            .subscribe("test_listener".to_string(), vec!["test.completed".to_string()])
            .unwrap();

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.publishes = vec!["test.completed".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _ = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        // First instruction: RunAgent
        let instr = kernel.get_next_instruction(&pid).unwrap();
        assert!(matches!(instr, Instruction::RunAgent { .. }));

        // Report agent result to advance to termination
        kernel.process_agent_result(
            &pid, "agent_a",
            serde_json::json!({"result": "done"}),
            None,
            crate::kernel::orchestrator_types::AgentExecutionMetrics::default(),
            true, "", false,
        ).unwrap();

        // Next instruction: Terminate (should publish completion event)
        let instr = kernel.get_next_instruction(&pid).unwrap();
        assert!(matches!(instr, Instruction::Terminate { .. }));

        // Verify the completion event was published
        let event = rx.try_recv().expect("should have received completion event");
        assert_eq!(event.event_type, "test.completed");
        assert!(event.source.contains("proc-pub-1"));

        let payload: serde_json::Value = serde_json::from_slice(&event.payload).unwrap();
        assert_eq!(payload["process_id"], "proc-pub-1");
        assert!(payload["terminal_reason"].as_str().unwrap().contains("Completed"));
    }

    #[test]
    fn test_no_publish_when_publishes_empty() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-nopub");
        setup_kernel_with_process(&mut kernel, &pid);

        // Subscribe to all events
        let (_sub, mut rx) = kernel.comm.bus
            .subscribe("test_listener".to_string(), vec!["test.completed".to_string()])
            .unwrap();

        // No publishes configured
        let config = test_config(vec![test_stage("agent_a")]);
        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        let _ = kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        let _instr = kernel.get_next_instruction(&pid).unwrap();
        kernel.process_agent_result(
            &pid, "agent_a",
            serde_json::json!({"result": "done"}),
            None,
            crate::kernel::orchestrator_types::AgentExecutionMetrics::default(),
            true, "", false,
        ).unwrap();
        let _instr = kernel.get_next_instruction(&pid).unwrap();

        // No completion event should be published (only envelope.snapshot events exist)
        assert!(rx.try_recv().is_err(), "should not receive completion event when publishes is empty");
    }

    // =========================================================================
    // AgentCard Tests
    // =========================================================================

    #[test]
    fn test_agent_card_registration_and_listing() {
        let mut kernel = Kernel::new();

        let card = crate::kernel::agent_card::AgentCard {
            name: "npc_dialogue".to_string(),
            description: "Handles NPC dialogue".to_string(),
            pipeline_name: Some("npc_pipeline".to_string()),
            capabilities: vec!["dialogue".to_string()],
            accepted_event_types: vec!["npc.speak".to_string()],
            published_event_types: vec!["npc.response".to_string()],
            input_schema: None,
            output_schema: None,
        };

        kernel.comm.agent_cards.register(card);

        let all = kernel.comm.agent_cards.list(None);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "npc_dialogue");

        // Filter by name substring
        let filtered = kernel.comm.agent_cards.list(Some("npc"));
        assert_eq!(filtered.len(), 1);

        let no_match = kernel.comm.agent_cards.list(Some("zzz_nonexistent"));
        assert_eq!(no_match.len(), 0);
    }

    #[test]
    fn test_agent_card_get_by_name() {
        let mut kernel = Kernel::new();

        let card = crate::kernel::agent_card::AgentCard {
            name: "search_agent".to_string(),
            description: "Searches things".to_string(),
            pipeline_name: None,
            capabilities: vec![],
            accepted_event_types: vec![],
            published_event_types: vec![],
            input_schema: None,
            output_schema: None,
        };

        kernel.comm.agent_cards.register(card);

        assert!(kernel.comm.agent_cards.get("search_agent").is_some());
        assert!(kernel.comm.agent_cards.get("nonexistent").is_none());
    }
}
