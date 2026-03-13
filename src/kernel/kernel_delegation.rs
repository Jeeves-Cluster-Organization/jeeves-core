//! Kernel delegation methods — process lifecycle, envelope store, quota,
//! interrupts, services, rate limiting, resource tracking, tool catalog,
//! cleanup, and system status.

use std::collections::HashMap;

use tracing::instrument;

use crate::envelope::{Envelope, FlowInterrupt, InterruptResponse};
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

use super::{
    interrupts, services, Kernel, ProcessState, RemainingBudget, ResourceQuota,
    SchedulingPriority, SystemStatus,
};

impl Kernel {
    // =============================================================================
    // Process Lifecycle
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

    /// Get process by PID.
    pub fn get_process(&self, pid: &ProcessId) -> Option<&super::ProcessControlBlock> {
        self.lifecycle.get(pid)
    }

    // =============================================================================
    // Process Envelope Store (Single Source of Truth)
    // =============================================================================

    /// Store an envelope for a process.
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
    #[deprecated(note = "Use get_process_envelope_mut() with field-level borrow splitting")]
    pub fn take_process_envelope(&mut self, pid: &ProcessId) -> Option<Envelope> {
        self.process_envelopes.remove(pid)
    }

    // =============================================================================
    // Quota & Usage
    // =============================================================================

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

    /// Get next runnable process.
    pub fn get_next_runnable(&mut self) -> Option<super::ProcessControlBlock> {
        self.lifecycle.get_next_runnable()
    }

    /// Schedule a process from NEW/WAITING/BLOCKED into READY.
    pub fn schedule_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.schedule(pid)
    }

    /// Start a process.
    pub fn start_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.start(pid)
    }

    /// Block a process.
    pub fn block_process(&mut self, pid: &ProcessId, reason: String) -> Result<()> {
        self.lifecycle.block(pid, reason)
    }

    /// Wait a process (e.g., awaiting interrupt response).
    pub fn wait_process(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        self.lifecycle.wait(pid, interrupt.kind)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.set_interrupt(interrupt);
        }
        Ok(())
    }

    /// Resume a process from waiting/blocked.
    pub fn resume_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.resume(pid)?;
        if let Some(env) = self.process_envelopes.get_mut(pid) {
            env.clear_interrupt();
        }
        Ok(())
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
        if let Some(subs) = self.process_subscriptions.remove(pid) {
            for (subscription, _receiver) in subs {
                self.commbus.unsubscribe(&subscription);
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
        if let Some(subs) = self.process_subscriptions.remove(pid) {
            for (subscription, _receiver) in subs {
                self.commbus.unsubscribe(&subscription);
            }
        }
        Ok(())
    }

    /// List all processes.
    pub fn list_processes(&self) -> Vec<super::ProcessControlBlock> {
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
    // Interrupt Methods
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

    /// Get all pending interrupts for a session.
    pub fn get_pending_for_session(
        &self,
        session_id: &str,
        kinds: Option<&[crate::envelope::InterruptKind]>,
    ) -> Vec<&interrupts::KernelInterrupt> {
        self.interrupts.get_pending_for_session(session_id, kinds)
    }

    // =============================================================================
    // Service Methods
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
    // Resource Tracking
    // =============================================================================

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
    // Quota Defaults
    // =============================================================================

    /// Get the kernel's default quota.
    pub fn get_default_quota(&self) -> &ResourceQuota {
        self.lifecycle.get_default_quota()
    }

    /// Merge overrides into the default quota.
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
    // Cleanup
    // =============================================================================

    /// Cleanup zombie processes older than `max_age_seconds`.
    pub fn cleanup_zombie_processes(&mut self, max_age_seconds: i64) -> usize {
        self.lifecycle.cleanup_zombies(max_age_seconds)
    }

    /// Cleanup stale orchestration sessions and their envelopes.
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

    /// Cleanup stale user usage entries.
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
