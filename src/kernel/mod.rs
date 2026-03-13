//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes commands via a single
//! message channel. Subsystems (lifecycle, resources, interrupts, rate limiter)
//! are plain structs owned by the Kernel, not separate actors.

use std::collections::HashMap;

// Core types
pub mod types;

// Subsystem modules
pub mod cleanup;
pub mod interrupts;
pub mod lifecycle;
pub mod orchestrator;
pub mod orchestrator_types;
pub mod rate_limiter;
pub mod resources;
pub mod routing;
pub mod services;

// Re-export key types
pub use cleanup::{CleanupConfig, CleanupService, CleanupStats};
pub use interrupts::{InterruptConfig, InterruptService, InterruptStatus, KernelInterrupt};
pub use lifecycle::LifecycleManager;
pub use rate_limiter::{RateLimitConfig, RateLimiter};
pub use resources::ResourceTracker;
pub use services::{
    RegistryStats, ServiceInfo, ServiceRegistry, ServiceStats, ServiceStatus, SERVICE_TYPE_MCP,
};
pub use types::{
    ProcessControlBlock, ProcessState, QuotaViolation, ResourceQuota, ResourceUsage,
    SchedulingPriority,
};

use crate::envelope::{Envelope, FlowInterrupt, InterruptResponse};
use crate::kernel::orchestrator_types::MergeStrategy;
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};
use tracing::instrument;

/// Merge a value into envelope.state according to the configured strategy.
fn merge_state_field(
    state: &mut HashMap<String, serde_json::Value>,
    key: &str,
    val: serde_json::Value,
    strategy: MergeStrategy,
) {
    match strategy {
        MergeStrategy::Replace => {
            state.insert(key.to_string(), val);
        }
        MergeStrategy::Append => {
            let arr = state.entry(key.to_string()).or_insert_with(|| serde_json::json!([]));
            if let Some(existing) = arr.as_array_mut() {
                existing.push(val);
            } else {
                // Not an array — replace with [existing, val]
                let old = arr.clone();
                *arr = serde_json::json!([old, val]);
            }
        }
        MergeStrategy::MergeDict => {
            if let serde_json::Value::Object(new_map) = val {
                let existing = state.entry(key.to_string()).or_insert_with(|| serde_json::json!({}));
                if let Some(existing_map) = existing.as_object_mut() {
                    for (k, v) in new_map {
                        existing_map.insert(k, v);
                    }
                } else {
                    *existing = serde_json::Value::Object(new_map);
                }
            } else {
                // Not an object — fall back to Replace
                state.insert(key.to_string(), val);
            }
        }
    }
}

/// Kernel - the main orchestrator.
///
/// Owns all subsystems and provides unified interface for process management.
/// NOT an actor in the message-passing sense - called directly via &mut self.
#[derive(Debug)]
pub struct Kernel {
    /// Process lifecycle management
    lifecycle: LifecycleManager,

    /// Resource tracking and quota enforcement
    resources: ResourceTracker,

    /// Rate limiting per user
    rate_limiter: RateLimiter,

    /// Interrupt handling (human-in-the-loop)
    interrupts: interrupts::InterruptService,

    /// Service registry (dispatch)
    services: services::ServiceRegistry,

    /// Pipeline orchestration (kernel-driven execution)
    orchestrator: orchestrator::Orchestrator,

    /// Communication bus (kernel-mediated inter-process communication)
    commbus: crate::commbus::CommBus,

    /// Process envelope storage (process_id -> envelope).
    /// Single source of truth for all envelopes. Keyed by ProcessId so that
    /// lifecycle methods, orchestrator, and actor dispatch all use the same key.
    process_envelopes: HashMap<ProcessId, Envelope>,

    /// Tool catalog — metadata, validation, prompt generation.
    tool_catalog: crate::tools::ToolCatalog,

    /// Tool access policy — agent-scoped permissions.
    tool_access: crate::tools::ToolAccessPolicy,

    /// Tool health tracking — sliding-window metrics and circuit breaking.
    tool_health: crate::tools::ToolHealthTracker,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            lifecycle: LifecycleManager::default(),
            resources: ResourceTracker::default(),
            rate_limiter: RateLimiter::default(),
            interrupts: interrupts::InterruptService::new(),
            services: services::ServiceRegistry::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            commbus: crate::commbus::CommBus::new(),
            process_envelopes: HashMap::new(),
            tool_catalog: crate::tools::ToolCatalog::new(),
            tool_access: crate::tools::ToolAccessPolicy::new(),
            tool_health: crate::tools::ToolHealthTracker::default(),
        }
    }

    /// Create a Kernel wired from a Config struct.
    pub fn from_config(config: &crate::Config) -> Self {
        let default_quota = ResourceQuota {
            max_llm_calls: config.defaults.max_llm_calls,
            max_tool_calls: config.defaults.max_tool_calls,
            max_agent_hops: config.defaults.max_agent_hops,
            max_iterations: config.defaults.max_iterations,
            timeout_seconds: config.defaults.process_timeout.as_secs() as i32,
            ..ResourceQuota::default()
        };
        let rate_limit_config = RateLimitConfig {
            requests_per_minute: config.rate_limit.requests_per_minute,
            requests_per_hour: config.rate_limit.requests_per_hour,
            burst_size: config.rate_limit.burst_size,
        };
        Self {
            lifecycle: LifecycleManager::new(Some(default_quota)),
            resources: ResourceTracker::new(),
            rate_limiter: RateLimiter::new(Some(rate_limit_config)),
            interrupts: interrupts::InterruptService::new(),
            services: services::ServiceRegistry::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            commbus: crate::commbus::CommBus::new(),
            process_envelopes: HashMap::new(),
            tool_catalog: crate::tools::ToolCatalog::new(),
            tool_access: crate::tools::ToolAccessPolicy::new(),
            tool_health: crate::tools::ToolHealthTracker::default(),
        }
    }

    pub fn with_config(
        default_quota: Option<ResourceQuota>,
        rate_limit_config: Option<RateLimitConfig>,
    ) -> Self {
        Self {
            lifecycle: LifecycleManager::new(default_quota),
            resources: ResourceTracker::new(),
            rate_limiter: RateLimiter::new(rate_limit_config),
            interrupts: interrupts::InterruptService::new(),
            services: services::ServiceRegistry::new(),
            orchestrator: orchestrator::Orchestrator::new(),
            commbus: crate::commbus::CommBus::new(),
            process_envelopes: HashMap::new(),
            tool_catalog: crate::tools::ToolCatalog::new(),
            tool_access: crate::tools::ToolAccessPolicy::new(),
            tool_health: crate::tools::ToolHealthTracker::default(),
        }
    }

    /// Subscribe to envelope snapshot events (call before spawning actor).
    /// Returns receiver that yields every envelope mutation event.
    pub fn subscribe_snapshots(&mut self) -> Result<tokio::sync::mpsc::UnboundedReceiver<crate::commbus::Event>> {
        let sub_id = format!("snap_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let (_sub, rx) = self.commbus.subscribe(sub_id, vec!["envelope.snapshot".to_string()])?;
        Ok(rx)
    }

    /// Create a new process.
    ///
    /// # Errors
    ///
    /// Returns error if rate limit exceeded or process submission/scheduling fails.
    #[instrument(skip(self), fields(pid = %pid))]
    pub fn create_process(
        &mut self,
        pid: ProcessId,
        request_id: RequestId,
        user_id: UserId,
        session_id: SessionId,
        priority: SchedulingPriority,
        quota: Option<ResourceQuota>,
    ) -> Result<ProcessControlBlock> {
        // Check rate limit
        self.rate_limiter.check_rate_limit(user_id.as_str())?;

        // Submit process to lifecycle manager
        let pcb = self.lifecycle.submit(
            pid.clone(),
            request_id,
            user_id,
            session_id,
            priority,
            quota,
        )?;

        // Schedule it
        self.lifecycle.schedule(&pid)?;

        Ok(pcb)
    }

    /// Get process by PID.
    pub fn get_process(&self, pid: &ProcessId) -> Option<&ProcessControlBlock> {
        self.lifecycle.get(pid)
    }

    // =============================================================================
    // Process Envelope Store (Single Source of Truth)
    // =============================================================================

    /// Store an envelope for a process. Kernel-owned, keyed by ProcessId.
    pub fn store_process_envelope(&mut self, pid: ProcessId, envelope: Envelope) {
        self.process_envelopes.insert(pid, envelope);
    }

    /// Get a reference to a process envelope.
    pub fn get_process_envelope(&self, pid: &ProcessId) -> Option<&Envelope> {
        self.process_envelopes.get(pid)
    }

    /// Get a mutable reference to a process envelope.
    pub fn get_process_envelope_mut(&mut self, pid: &ProcessId) -> Option<&mut Envelope> {
        self.process_envelopes.get_mut(pid)
    }

    /// Take (remove and return) a process envelope.
    ///
    /// Prefer `get_process_envelope_mut()` + field-level borrow splitting instead.
    /// This method removes the envelope from the store — if the caller fails to
    /// re-insert it, the envelope is silently lost.
    #[deprecated(note = "Use get_process_envelope_mut() with field-level borrow splitting")]
    pub fn take_process_envelope(&mut self, pid: &ProcessId) -> Option<Envelope> {
        self.process_envelopes.remove(pid)
    }

    /// Check process quota.
    ///
    /// # Errors
    ///
    /// Returns error if process not found or quota exceeded.
    pub fn check_quota(&self, pid: &ProcessId) -> Result<()> {
        let pcb = self
            .lifecycle
            .get(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
        self.resources.check_quota(pcb)
    }

    /// Record resource usage (updates both per-user aggregate and per-process PCB).
    pub fn record_usage(
        &mut self,
        pid: &ProcessId,
        user_id: &str,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    ) {
        // Per-user aggregate (billing/rate-limit secondary index)
        self.resources
            .record_usage(user_id, llm_calls, tool_calls, tokens_in, tokens_out);

        // Per-process PCB (single source of truth for quota checks)
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.usage.llm_calls += llm_calls;
            pcb.usage.tool_calls += tool_calls;
            pcb.usage.tokens_in += tokens_in;
            pcb.usage.tokens_out += tokens_out;
        }
    }

    /// Get next runnable process.
    pub fn get_next_runnable(&mut self) -> Option<ProcessControlBlock> {
        self.lifecycle.get_next_runnable()
    }

    /// Schedule a process from NEW/WAITING/BLOCKED into READY.
    pub fn schedule_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.schedule(pid)
    }

    /// Start a process.
    ///
    /// # Errors
    ///
    /// Returns error if process not found or invalid state transition.
    pub fn start_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.start(pid)
    }

    /// Block a process (e.g., resource exhausted).
    ///
    /// # Errors
    ///
    /// Returns error if process not found or invalid state transition.
    pub fn block_process(&mut self, pid: &ProcessId, reason: String) -> Result<()> {
        self.lifecycle.block(pid, reason)
    }

    /// Wait a process (e.g., awaiting interrupt response).
    ///
    /// # Errors
    ///
    /// Returns error if process not found or invalid state transition.
    pub fn wait_process(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        self.lifecycle.wait(pid, interrupt.kind)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.set_interrupt(interrupt);
        }
        Ok(())
    }

    /// Resume a process from waiting/blocked.
    ///
    /// # Errors
    ///
    /// Returns error if process not found or invalid state transition.
    pub fn resume_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.resume(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.clear_interrupt();
        }
        Ok(())
    }

    /// Resume a process with optional envelope updates.
    ///
    /// Clears the interrupt and merges any provided updates into the envelope.
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
    ///
    /// # Errors
    ///
    /// Returns error if process not found or invalid state transition.
    pub fn terminate_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.terminate("Process terminated");
        }
        Ok(())
    }

    /// Cleanup and remove a terminated process.
    ///
    /// Removes the PCB, the process envelope, and the pipeline session.
    ///
    /// # Errors
    ///
    /// Returns error if process not found or not in terminal state.
    pub fn cleanup_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.cleanup(pid)?;
        self.lifecycle.remove(pid)?;
        self.process_envelopes.remove(pid);
        self.orchestrator.cleanup_session(pid);
        Ok(())
    }

    /// List all processes.
    pub fn list_processes(&self) -> Vec<ProcessControlBlock> {
        self.lifecycle.list()
    }

    /// Count processes.
    pub fn process_count(&self) -> usize {
        self.lifecycle.count()
    }

    /// Count processes by state.
    pub fn process_count_by_state(&self, state: ProcessState) -> usize {
        self.lifecycle.count_by_state(state)
    }

    // =============================================================================
    // Interrupt Methods (Delegation to InterruptService)
    // =============================================================================

    /// Create a new interrupt for a process.
    pub fn create_interrupt(
        &mut self,
        params: interrupts::CreateInterruptParams,
    ) -> interrupts::KernelInterrupt {
        self.interrupts.create_interrupt(params)
    }

    /// Resolve an interrupt with a response.
    pub fn resolve_interrupt(
        &mut self,
        interrupt_id: &str,
        response: InterruptResponse,
        user_id: Option<&str>,
    ) -> bool {
        self.interrupts.resolve(interrupt_id, response, user_id)
    }

    /// Get the most recent pending interrupt for a request.
    pub fn get_pending_interrupt(&self, request_id: &str) -> Option<&interrupts::KernelInterrupt> {
        self.interrupts.get_pending_for_request(request_id)
    }

    /// Cancel an interrupt.
    pub fn cancel_interrupt(&mut self, interrupt_id: &str, reason: String) -> bool {
        self.interrupts.cancel(interrupt_id, reason)
    }

    /// Get an interrupt by ID.
    pub fn get_interrupt(&self, interrupt_id: &str) -> Option<&interrupts::KernelInterrupt> {
        self.interrupts.get_interrupt(interrupt_id)
    }

    /// Get all pending interrupts for a session, optionally filtered by kind.
    pub fn get_pending_for_session(
        &self,
        session_id: &str,
        kinds: Option<&[crate::envelope::InterruptKind]>,
    ) -> Vec<&interrupts::KernelInterrupt> {
        self.interrupts.get_pending_for_session(session_id, kinds)
    }

    // =============================================================================
    // Service Methods (Delegation to ServiceRegistry)
    // =============================================================================

    /// Register a service.
    pub fn register_service(&mut self, info: services::ServiceInfo) -> bool {
        self.services.register_service(info)
    }

    /// Unregister a service.
    pub fn unregister_service(&mut self, service_name: &str) -> bool {
        self.services.unregister_service(service_name)
    }

    /// Get a service by name.
    pub fn get_service(&self, name: &str) -> Option<services::ServiceInfo> {
        self.services.get_service(name)
    }

    /// List services matching criteria.
    pub fn list_services(
        &self,
        service_type: Option<&str>,
        healthy_only: bool,
    ) -> Vec<services::ServiceInfo> {
        self.services.list_services(service_type, healthy_only)
    }

    /// Get all service names.
    pub fn get_service_names(&self) -> Vec<String> {
        self.services.get_service_names()
    }

    /// Increment load for a service.
    pub fn increment_service_load(&mut self, name: &str) -> bool {
        self.services.increment_load(name)
    }

    /// Decrement load for a service.
    pub fn decrement_service_load(&mut self, name: &str) -> bool {
        self.services.decrement_load(name)
    }

    /// Get current load for a service.
    pub fn get_service_load(&self, name: &str) -> i32 {
        self.services.get_load(name)
    }

    /// Update health status for a service.
    pub fn update_service_health(
        &mut self,
        name: &str,
        status: services::ServiceStatus,
    ) -> bool {
        self.services.update_health(name, status)
    }

    /// Get statistics for a specific service.
    pub fn get_service_stats(&self, name: &str) -> Option<services::ServiceStats> {
        self.services.get_service_stats(name)
    }

    /// Get overall registry statistics.
    pub fn get_registry_stats(&self) -> services::RegistryStats {
        self.services.get_stats()
    }

    // =============================================================================
    // Orchestrator Methods (Microkernel: kernel owns envelope, orchestrator processes)
    // =============================================================================

    /// Initialize an orchestration session.
    ///
    /// Stores the envelope in `process_envelopes`, then initializes the pipeline
    /// session in the orchestrator. The orchestrator sets pipeline bounds on the
    /// envelope but does not own it.
    ///
    /// # Errors
    ///
    /// Returns error if session already exists (unless `force`) or config is invalid.
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

        let state = self.orchestrator
            .initialize_session(process_id.clone(), pipeline_config, &mut envelope, force)?;
        self.process_envelopes.insert(process_id, envelope);
        Ok(state)
    }

    /// Get the next instruction for a process.
    ///
    /// Extracts the envelope from `process_envelopes`, passes it to the orchestrator
    /// (which may mutate it for bounds termination), then re-stores it.
    ///
    /// # Errors
    ///
    /// Returns error if no orchestration session or envelope exists for this process.
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
        if instruction.kind == orchestrator::InstructionKind::RunAgent
            || instruction.kind == orchestrator::InstructionKind::RunAgents
        {
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

        Ok(instruction)
    }

    /// Process a complete agent result: merge output, report to orchestrator,
    /// sync PCB counters, and emit snapshot.
    ///
    /// Mutation only — caller fetches the next instruction separately via
    /// `get_next_instruction()`. This decoupling prevents fork/parallel deadlocks.
    ///
    /// `metrics` is the delta — applied directly to both envelope (via orchestrator)
    /// and PCB (via record_usage), eliminating backwards reconciliation arithmetic.
    ///
    /// # Errors
    ///
    /// Returns error if no orchestration session or envelope exists for this process.
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

        // Clone state_schema + output_key from orchestrator before mutable envelope borrow (C9)
        let state_schema = self.orchestrator.get_state_schema(process_id).cloned().unwrap_or_default();
        let output_key = self.orchestrator.get_stage_output_key(process_id, agent_name)
            .unwrap_or_else(|| agent_name.to_string());

        // Phase 1: Merge output into envelope + run orchestrator
        // Scoped block so &mut envelope borrow drops before Phase 2 calls &mut self
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

        // Phase 2: Apply SAME metrics delta directly to PCB — no reconciliation!
        {
            let user_id = self.lifecycle.get(process_id).map(|p| p.user_id.as_str().to_string());
            if let Some(uid) = user_id {
                self.record_usage(process_id, &uid, llm_calls, tool_calls, tokens_in, tokens_out);
            }
        }

        // Phase 3: Snapshot (mutation complete — caller fetches next instruction separately)
        self.emit_envelope_snapshot(process_id, "agent_completed");
        Ok(())
    }

    /// Get orchestration session state.
    ///
    /// # Errors
    ///
    /// Returns error if no orchestration session or envelope exists for this process.
    pub fn get_orchestration_state(
        &self,
        process_id: &ProcessId,
    ) -> Result<orchestrator::SessionState> {
        let envelope = self.process_envelopes.get(process_id)
            .ok_or_else(|| Error::not_found(format!("Envelope not found for process: {}", process_id)))?;
        self.orchestrator.get_session_state(process_id, envelope)
    }

    // =============================================================================
    // Rate Limiting
    // =============================================================================

    /// Check and optionally record rate-limit usage for a user.
    pub fn check_rate_limit(&mut self, user_id: &str) -> Result<()> {
        self.rate_limiter.check_rate_limit(user_id)
    }

    /// Current request count in the minute window.
    pub fn get_current_rate(&mut self, user_id: &str) -> usize {
        self.rate_limiter.get_current_rate(user_id)
    }

    // =============================================================================
    // Envelope Snapshot Emission
    // =============================================================================

    /// Emit an envelope snapshot to CommBus subscribers.
    ///
    /// Fired at mutation points so capabilities can subscribe and handle
    /// envelope state changes (persist, forward to UI, etc.).
    pub fn emit_envelope_snapshot(&mut self, pid: &ProcessId, trigger: &str) {
        let Some(envelope) = self.process_envelopes.get(pid) else { return; };
        let payload = serde_json::json!({
            "pid": pid.as_str(),
            "trigger": trigger,
            "envelope": envelope,
        });
        let Ok(payload_bytes) = serde_json::to_vec(&payload) else { return; };
        let event = crate::commbus::Event {
            event_type: "envelope.snapshot".to_string(),
            payload: payload_bytes,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            source: "kernel".to_string(),
        };
        let _ = self.publish_event(event);
    }

    // =============================================================================
    // CommBus Methods (Delegation to CommBus)
    // =============================================================================

    /// Publish an event to subscribers.
    pub fn publish_event(&mut self, event: crate::commbus::Event) -> Result<usize> {
        self.commbus.publish(event)
    }

    // =============================================================================
    // Resource Tracking (Additional Methods)
    // =============================================================================

    /// Record a tool call for a process (PCB is the single source of truth).
    pub fn record_tool_call(&mut self, pid: &ProcessId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_tool_call", pid)))?;
        pcb.usage.tool_calls += 1;
        Ok(())
    }

    /// Record an agent hop for a process (PCB is the single source of truth).
    pub fn record_agent_hop(&mut self, pid: &ProcessId) -> Result<()> {
        let pcb = self.lifecycle.get_mut(pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found in record_agent_hop", pid)))?;
        pcb.usage.agent_hops += 1;
        Ok(())
    }

    // =============================================================================
    // Quota Defaults (Single Source of Truth)
    // =============================================================================

    /// Get the kernel's default quota.
    pub fn get_default_quota(&self) -> &ResourceQuota {
        self.lifecycle.get_default_quota()
    }

    /// Merge overrides into the default quota (non-zero fields overwrite).
    pub fn set_default_quota(&mut self, overrides: &ResourceQuota) {
        self.lifecycle.set_default_quota(overrides);
    }

    // =============================================================================
    // Tool Catalog & Access Control
    // =============================================================================

    /// Get a reference to the tool catalog.
    pub fn tool_catalog(&self) -> &crate::tools::ToolCatalog {
        &self.tool_catalog
    }

    /// Get a mutable reference to the tool catalog.
    pub fn tool_catalog_mut(&mut self) -> &mut crate::tools::ToolCatalog {
        &mut self.tool_catalog
    }

    /// Get a reference to the tool access policy.
    pub fn tool_access(&self) -> &crate::tools::ToolAccessPolicy {
        &self.tool_access
    }

    /// Get a mutable reference to the tool access policy.
    pub fn tool_access_mut(&mut self) -> &mut crate::tools::ToolAccessPolicy {
        &mut self.tool_access
    }

    /// Get a reference to the tool health tracker.
    pub fn tool_health(&self) -> &crate::tools::ToolHealthTracker {
        &self.tool_health
    }

    /// Get a mutable reference to the tool health tracker.
    pub fn tool_health_mut(&mut self) -> &mut crate::tools::ToolHealthTracker {
        &mut self.tool_health
    }

    // =============================================================================
    // Cleanup Facade Methods
    // =============================================================================

    /// Cleanup zombie processes older than `max_age_seconds`.
    pub fn cleanup_zombie_processes(&mut self, max_age_seconds: i64) -> usize {
        self.lifecycle.cleanup_zombies(max_age_seconds)
    }

    /// Cleanup stale orchestration sessions and their envelopes.
    /// Returns the number of sessions removed.
    pub fn cleanup_stale_sessions(&mut self, max_age_seconds: i64) -> usize {
        let removed_pids = self.orchestrator.cleanup_stale_sessions(max_age_seconds);
        let count = removed_pids.len();
        for pid in &removed_pids {
            self.process_envelopes.remove(pid);
        }
        count
    }

    /// Cleanup resolved interrupts older than the given duration.
    pub fn cleanup_resolved_interrupts(&mut self, max_age: chrono::Duration) -> usize {
        self.interrupts.cleanup_resolved(max_age)
    }

    /// Cleanup expired rate limit windows.
    pub fn cleanup_expired_rate_windows(&mut self) {
        self.rate_limiter.cleanup_expired();
    }

    /// Cleanup stale user usage entries (users with no active processes).
    pub fn cleanup_stale_user_usage(&mut self, max_entries: usize) -> usize {
        let active_user_ids = self.lifecycle.get_active_user_ids();
        self.resources.cleanup_stale_users(&active_user_ids, max_entries)
    }

    /// Remove a process envelope by PID (used during cleanup).
    pub fn remove_process_envelope(&mut self, pid: &ProcessId) -> Option<Envelope> {
        self.process_envelopes.remove(pid)
    }

    // =============================================================================
    // System Status Aggregation
    // =============================================================================

    /// Get a full system status snapshot (sync parts).
    /// CommBus stats can be gathered separately (async).
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

/// Remaining resource budget for a process.
#[derive(Debug, Clone)]
pub struct RemainingBudget {
    pub llm_calls_remaining: i32,
    pub iterations_remaining: i32,
    pub agent_hops_remaining: i32,
    pub tokens_in_remaining: i64,
    pub tokens_out_remaining: i64,
    pub time_remaining_seconds: f64,
}

/// Full system status snapshot returned by `Kernel::get_system_status()`.
#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub processes_total: usize,
    pub processes_by_state: HashMap<ProcessState, usize>,
    pub services_healthy: usize,
    pub services_degraded: usize,
    pub services_unhealthy: usize,
    pub active_orchestration_sessions: usize,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_status_empty_kernel() {
        let kernel = Kernel::new();
        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 0);
        assert_eq!(status.services_healthy, 0);
        assert_eq!(status.services_degraded, 0);
        assert_eq!(status.services_unhealthy, 0);
        assert_eq!(status.active_orchestration_sessions, 0);

        for (_state, count) in &status.processes_by_state {
            assert_eq!(*count, 0);
        }
    }

    #[test]
    fn test_get_system_status_with_processes() {
        let mut kernel = Kernel::new();

        // Submit 3 processes (all start in New)
        let pid1 = ProcessId::must("pid1");
        let pid2 = ProcessId::must("pid2");
        let pid3 = ProcessId::must("pid3");

        kernel
            .lifecycle
            .submit(
                pid1.clone(),
                RequestId::must("req1"),
                UserId::must("user1"),
                SessionId::must("sess1"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();
        kernel
            .lifecycle
            .submit(
                pid2.clone(),
                RequestId::must("req2"),
                UserId::must("user2"),
                SessionId::must("sess2"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();
        kernel
            .lifecycle
            .submit(
                pid3.clone(),
                RequestId::must("req3"),
                UserId::must("user3"),
                SessionId::must("sess3"),
                SchedulingPriority::Normal,
                None,
            )
            .unwrap();

        // Schedule 2 of them (New → Ready)
        kernel.lifecycle.schedule(&pid1).unwrap();
        kernel.lifecycle.schedule(&pid2).unwrap();

        // Transition 1 to Running (Ready → Running)
        kernel.lifecycle.start(&pid1).unwrap();

        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 3);
        assert_eq!(
            *status.processes_by_state.get(&ProcessState::New).unwrap(),
            1
        );
        assert_eq!(
            *status.processes_by_state.get(&ProcessState::Ready).unwrap(),
            1
        );
        assert_eq!(
            *status
                .processes_by_state
                .get(&ProcessState::Running)
                .unwrap(),
            1
        );
    }

    #[test]
    fn test_record_tool_call_missing_pid_returns_err() {
        let mut kernel = Kernel::new();
        let result = kernel.record_tool_call(&ProcessId::must("nonexistent"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_record_agent_hop_missing_pid_returns_err() {
        let mut kernel = Kernel::new();
        let result = kernel.record_agent_hop(&ProcessId::must("nonexistent"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_pcb_sync_after_agent_result() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        // Create process
        kernel.create_process(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        // Record usage
        kernel.record_usage(&pid, "user1", 3, 5, 1000, 500);

        // Verify PCB counters
        let pcb = kernel.get_process(&pid).unwrap();
        assert_eq!(pcb.usage.llm_calls, 3);
        assert_eq!(pcb.usage.tool_calls, 5);
        assert_eq!(pcb.usage.tokens_in, 1000);
        assert_eq!(pcb.usage.tokens_out, 500);
    }

    #[test]
    fn test_process_lifecycle_create_and_destroy() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        // Create process
        kernel.create_process(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        // Verify it exists
        assert!(kernel.get_process(&pid).is_some());
        assert_eq!(kernel.process_count(), 1);

        // Start → Terminate → Cleanup
        kernel.start_process(&pid).unwrap();
        kernel.terminate_process(&pid).unwrap();
        kernel.cleanup_process(&pid).unwrap();

        // Verify it's gone
        assert!(kernel.get_process(&pid).is_none());
        assert_eq!(kernel.process_count(), 0);
    }

    #[test]
    fn test_record_tool_call_increments_counter() {
        let mut kernel = Kernel::new();
        let pid = ProcessId::must("p1");

        kernel.create_process(
            pid.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        // Record tool calls
        kernel.record_tool_call(&pid).unwrap();
        kernel.record_tool_call(&pid).unwrap();
        kernel.record_tool_call(&pid).unwrap();

        let pcb = kernel.get_process(&pid).unwrap();
        assert_eq!(pcb.usage.tool_calls, 3);
    }
}
