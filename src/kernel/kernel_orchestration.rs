//! Kernel orchestration methods — initialize, get_next_instruction, process_agent_result.

use std::collections::HashMap;

use tracing::instrument;

use crate::envelope::Envelope;
use crate::types::{Error, ProcessId, Result};

use super::merge_state_field;
use super::orchestrator;
use super::Kernel;

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
                self.tool_access.grant_many(&stage.agent, tools);
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
            match self.commbus.subscribe(subscriber_id, subscription_types) {
                Ok((subscription, receiver)) => {
                    self.process_subscriptions
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
        if let Some(subs) = self.process_subscriptions.get_mut(process_id) {
            if let Some(envelope) = self.process_envelopes.get_mut(process_id) {
                for (_subscription, receiver) in subs.iter_mut() {
                    while let Ok(event) = receiver.try_recv() {
                        envelope.event_inbox.push(event);
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
        match instruction.kind {
            orchestrator::InstructionKind::RunAgent
            | orchestrator::InstructionKind::RunAgents => {
                if let Some(envelope) = self.process_envelopes.get(process_id) {
                    // Build agent context from envelope
                    let mut prompt_context = serde_json::Map::new();
                    for (agent_name, output) in &envelope.outputs {
                        for (key, value) in output {
                            prompt_context.insert(format!("{}_{}", agent_name, key), value.clone());
                        }
                    }
                    for (key, value) in &envelope.audit.metadata {
                        prompt_context.insert(key.clone(), value.clone());
                    }

                    instruction.agent_context = Some(serde_json::json!({
                        "envelope_id": envelope.identity.envelope_id.as_str(),
                        "request_id": envelope.identity.request_id.as_str(),
                        "user_id": envelope.identity.user_id.as_str(),
                        "session_id": envelope.identity.session_id.as_str(),
                        "raw_input": &envelope.raw_input,
                        "outputs": &envelope.outputs,
                        "state": &envelope.state,
                        "metadata": &envelope.audit.metadata,
                        "prompt_context": serde_json::Value::Object(prompt_context),
                        "llm_call_count": envelope.bounds.llm_call_count,
                        "agent_hop_count": envelope.bounds.agent_hop_count,
                        "tokens_in": envelope.bounds.tokens_in,
                        "tokens_out": envelope.bounds.tokens_out,
                        "circuit_broken_tools": self.tool_health.get_circuit_broken_tools(),
                        "pending_events": &envelope.event_inbox,
                    }));
                }

                // Look up stage-level config
                if let Some(agent_name) = instruction.agents.first() {
                    let stage_name = self.process_envelopes.get(process_id)
                        .map(|e| e.pipeline.current_stage.clone())
                        .unwrap_or_default();
                    instruction.output_schema = self.orchestrator.get_stage_output_schema(process_id, &stage_name);

                    let allowed = self.tool_access.tools_for_agent(agent_name);
                    if !allowed.is_empty() {
                        instruction.allowed_tools = Some(allowed);
                    }
                }
            }
            orchestrator::InstructionKind::Terminate => {
                // Attach final outputs so the worker can return them
                if let Some(envelope) = self.process_envelopes.get(process_id) {
                    instruction.agent_context = Some(serde_json::json!({
                        "outputs": &envelope.outputs,
                    }));
                }
            }
            _ => {}
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
        for tr in &metrics.tool_results {
            self.tool_health.record_execution(&tr.name, tr.success, tr.latency_ms, tr.error_type.clone());
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
            for field in &state_schema {
                if field.key == output_key {
                    let output_value = serde_json::Value::Object(
                        envelope.outputs.get(agent_name)
                            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                            .unwrap_or_default()
                    );
                    merge_state_field(&mut envelope.state, &field.key, output_value, field.merge);
                    break;
                }
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
                        if !self.tool_access.check_access(agent_name, tool_name) {
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
}

#[cfg(test)]
mod tests {
    use crate::envelope::Envelope;
    use crate::kernel::orchestrator::{
        InstructionKind, NodeKind, PipelineConfig, PipelineStage, JoinStrategy,
    };
    use crate::kernel::{Kernel, SchedulingPriority};
    use crate::types::{ProcessId, RequestId, UserId, SessionId};

    fn test_stage(name: &str) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            routing: vec![],
            default_next: None,
            error_next: None,
            max_visits: None,
            node_kind: NodeKind::Agent,
            output_key: None,
            join_strategy: JoinStrategy::WaitAll,
            has_llm: false,
            prompt_key: None,
            temperature: None,
            max_tokens: None,
            model_role: None,
            allowed_tools: None,
            output_schema: None,
            child_pipeline: None,
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
        assert!(kernel.process_subscriptions.contains_key(&pid));
        let subs = kernel.process_subscriptions.get(&pid).unwrap();
        assert_eq!(subs.len(), 1); // One subscribe call with multiple event types
    }

    #[test]
    fn test_no_subscriptions_when_config_empty() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-nosub");
        setup_kernel_with_process(&mut kernel, &pid);

        let config = test_config(vec![test_stage("agent_a")]);
        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);

        let _ = kernel.initialize_orchestration(pid.clone(), config, envelope, false);

        // No subscriptions should be created
        assert!(!kernel.process_subscriptions.contains_key(&pid));
    }

    #[test]
    fn test_event_inbox_drain_on_get_next_instruction() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-drain-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["test.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        // Publish an event to the CommBus (will be delivered to our subscription)
        let event = crate::commbus::Event {
            event_type: "test.event".to_string(),
            payload: b"{\"msg\":\"world\"}".to_vec(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            source: "external".to_string(),
        };
        kernel.commbus.publish(event).unwrap();

        // get_next_instruction should drain events into envelope.event_inbox
        let instruction = kernel.get_next_instruction(&pid).unwrap();
        assert_eq!(instruction.kind, InstructionKind::RunAgent);

        // Check that the event was drained into the envelope
        let envelope = kernel.process_envelopes.get(&pid).unwrap();
        assert_eq!(envelope.event_inbox.len(), 1);
        assert_eq!(envelope.event_inbox[0].event_type, "test.event");
        assert_eq!(envelope.event_inbox[0].source, "external");
    }

    #[test]
    fn test_terminate_process_cleans_up_subscriptions() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-cleanup-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["test.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        assert!(kernel.process_subscriptions.contains_key(&pid));

        // Terminate should clean up subscriptions
        kernel.terminate_process(&pid).unwrap();
        assert!(!kernel.process_subscriptions.contains_key(&pid));
    }

    #[test]
    fn test_pending_events_in_agent_context() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("proc-ctx-1");
        setup_kernel_with_process(&mut kernel, &pid);

        let mut config = test_config(vec![test_stage("agent_a")]);
        config.subscriptions = vec!["npc.event".to_string()];

        let envelope = Envelope::new_minimal("user-1", "sess-1", "hello", None);
        kernel.initialize_orchestration(pid.clone(), config, envelope, false).unwrap();

        // Publish two events
        for i in 0..2 {
            let event = crate::commbus::Event {
                event_type: "npc.event".to_string(),
                payload: format!("{{\"n\":{}}}", i).into_bytes(),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                source: "game".to_string(),
            };
            kernel.commbus.publish(event).unwrap();
        }

        let instruction = kernel.get_next_instruction(&pid).unwrap();
        // Agent context should include pending_events
        let ctx = instruction.agent_context.unwrap();
        let pending = ctx.get("pending_events").unwrap().as_array().unwrap();
        assert_eq!(pending.len(), 2);
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

        kernel.agent_cards.register(card);

        let all = kernel.agent_cards.list(None);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "npc_dialogue");

        // Filter by name substring
        let filtered = kernel.agent_cards.list(Some("npc"));
        assert_eq!(filtered.len(), 1);

        let no_match = kernel.agent_cards.list(Some("zzz_nonexistent"));
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

        kernel.agent_cards.register(card);

        assert!(kernel.agent_cards.get("search_agent").is_some());
        assert!(kernel.agent_cards.get("nonexistent").is_none());
    }
}
