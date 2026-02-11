//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes IPC commands via a single
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
pub mod rate_limiter;
pub mod recovery;
pub mod resources;
pub mod services;

// Re-export key types
pub use cleanup::{CleanupConfig, CleanupService, CleanupStats};
pub use interrupts::{InterruptConfig, InterruptService, InterruptStatus, KernelInterrupt};
pub use lifecycle::LifecycleManager;
pub use rate_limiter::{RateLimitConfig, RateLimiter};
pub use recovery::with_recovery;
pub use resources::ResourceTracker;
pub use services::{
    DispatchResult, DispatchTarget, RegistryStats, ServiceInfo, ServiceRegistry, ServiceStats,
    ServiceStatus,
};
pub use types::{
    ProcessControlBlock, ProcessState, QuotaViolation, ResourceQuota, ResourceUsage,
    SchedulingPriority,
};

use crate::envelope::{Envelope, FlowInterrupt, InterruptResponse};
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};

/// Kernel - the main orchestrator.
///
/// Owns all subsystems and provides unified interface for process management.
/// NOT an actor in the message-passing sense - called directly via &mut self.
#[derive(Debug)]
pub struct Kernel {
    /// Process lifecycle management
    pub lifecycle: LifecycleManager,

    /// Resource tracking and quota enforcement
    pub resources: ResourceTracker,

    /// Rate limiting per user
    pub rate_limiter: RateLimiter,

    /// Interrupt handling (human-in-the-loop)
    pub interrupts: interrupts::InterruptService,

    /// Service registry (IPC and dispatch)
    pub services: services::ServiceRegistry,

    /// Pipeline orchestration (kernel-driven execution)
    pub orchestrator: orchestrator::Orchestrator,

    /// Communication bus (kernel-mediated IPC)
    pub commbus: crate::commbus::CommBus,

    /// Envelope storage (envelope_id -> envelope)
    envelopes: HashMap<String, Envelope>,
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
            envelopes: HashMap::new(),
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
            envelopes: HashMap::new(),
        }
    }

    /// Create a new process.
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

    /// Store envelope.
    pub fn store_envelope(&mut self, envelope: Envelope) {
        self.envelopes
            .insert(envelope.identity.envelope_id.to_string(), envelope);
    }

    /// Get envelope by ID.
    pub fn get_envelope(&self, envelope_id: &str) -> Option<&Envelope> {
        self.envelopes.get(envelope_id)
    }

    /// Get mutable envelope by ID.
    pub fn get_envelope_mut(&mut self, envelope_id: &str) -> Option<&mut Envelope> {
        self.envelopes.get_mut(envelope_id)
    }

    /// Remove envelope.
    pub fn remove_envelope(&mut self, envelope_id: &str) -> Option<Envelope> {
        self.envelopes.remove(envelope_id)
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
        user_id: &str,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    ) {
        self.resources
            .record_usage(user_id, llm_calls, tool_calls, tokens_in, tokens_out);
    }

    /// Get next runnable process.
    pub fn get_next_runnable(&mut self) -> Option<ProcessControlBlock> {
        self.lifecycle.get_next_runnable()
    }

    /// Start a process.
    pub fn start_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.start(pid)
    }

    /// Block a process (e.g., resource exhausted).
    pub fn block_process(&mut self, pid: &ProcessId, reason: String) -> Result<()> {
        self.lifecycle.block(pid, reason)
    }

    /// Wait a process (e.g., awaiting interrupt response).
    pub fn wait_process(&mut self, pid: &ProcessId, interrupt: FlowInterrupt) -> Result<()> {
        self.lifecycle.wait(pid, interrupt.kind)?;
        // Also set interrupt on envelope
        if let Some(env) = self.envelopes.get_mut(pid.as_str()) {
            env.set_interrupt(interrupt);
        }
        Ok(())
    }

    /// Resume a process from waiting/blocked.
    pub fn resume_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.resume(pid)?;
        // Clear interrupt on envelope
        if let Some(env) = self.envelopes.get_mut(pid.as_str()) {
            env.clear_interrupt();
        }
        Ok(())
    }

    /// Terminate a process.
    pub fn terminate_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        // Terminate envelope
        if let Some(env) = self.envelopes.get_mut(pid.as_str()) {
            env.terminate("Process terminated");
        }
        Ok(())
    }

    /// Cleanup and remove a terminated process.
    pub fn cleanup_process(&mut self, pid: &ProcessId) -> Result<()> {
        self.lifecycle.cleanup(pid)?;
        self.lifecycle.remove(pid)?;
        self.envelopes.remove(pid.as_str());
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

    /// Dispatch a request to a service.
    pub fn dispatch(
        &mut self,
        target: &services::DispatchTarget,
        data: &HashMap<String, serde_json::Value>,
    ) -> Result<services::DispatchResult> {
        self.services
            .dispatch(target, data)
            .map_err(Error::internal)
    }

    // =============================================================================
    // Orchestrator Methods (Delegation to Orchestrator)
    // =============================================================================

    /// Initialize an orchestration session.
    pub fn initialize_orchestration(
        &mut self,
        process_id: ProcessId,
        pipeline_config: orchestrator::PipelineConfig,
        envelope: Envelope,
        force: bool,
    ) -> Result<orchestrator::SessionState> {
        self.orchestrator
            .initialize_session(process_id, pipeline_config, envelope, force)
    }

    /// Get the next instruction for a process.
    pub fn get_next_instruction(
        &mut self,
        process_id: &ProcessId,
    ) -> Result<orchestrator::Instruction> {
        self.orchestrator
            .get_next_instruction(process_id)
    }

    /// Report agent execution result.
    pub fn report_agent_result(
        &mut self,
        process_id: &ProcessId,
        metrics: orchestrator::AgentExecutionMetrics,
        updated_envelope: Envelope,
    ) -> Result<()> {
        self.orchestrator
            .report_agent_result(process_id, metrics, updated_envelope)
    }

    /// Get orchestration session state.
    pub fn get_orchestration_state(&self, process_id: &ProcessId) -> Result<orchestrator::SessionState> {
        self.orchestrator
            .get_session_state(process_id)
    }

    // =============================================================================
    // Resource Tracking (Additional Methods)
    // =============================================================================

    /// Record a tool call for a process (PCB is the single source of truth).
    pub fn record_tool_call(&mut self, pid: &ProcessId) -> Result<()> {
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.usage.tool_calls += 1;
        }
        Ok(())
    }

    /// Record an agent hop for a process (PCB is the single source of truth).
    pub fn record_agent_hop(&mut self, pid: &ProcessId) -> Result<()> {
        if let Some(pcb) = self.lifecycle.get_mut(pid) {
            pcb.usage.agent_hops += 1;
        }
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
    // System Status Aggregation
    // =============================================================================

    /// Get a full system status snapshot (sync parts).
    /// CommBus stats are gathered separately in the IPC handler (async).
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

        kernel.lifecycle.submit(
            pid1.clone(),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();
        kernel.lifecycle.submit(
            pid2.clone(),
            RequestId::must("req2"),
            UserId::must("user2"),
            SessionId::must("sess2"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();
        kernel.lifecycle.submit(
            pid3.clone(),
            RequestId::must("req3"),
            UserId::must("user3"),
            SessionId::must("sess3"),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        // Schedule 2 of them (New → Ready)
        kernel.lifecycle.schedule(&pid1).unwrap();
        kernel.lifecycle.schedule(&pid2).unwrap();

        // Transition 1 to Running (Ready → Running)
        kernel.lifecycle.start(&pid1).unwrap();

        let status = kernel.get_system_status();

        assert_eq!(status.processes_total, 3);
        assert_eq!(*status.processes_by_state.get(&ProcessState::New).unwrap(), 1);
        assert_eq!(*status.processes_by_state.get(&ProcessState::Ready).unwrap(), 1);
        assert_eq!(*status.processes_by_state.get(&ProcessState::Running).unwrap(), 1);
    }
}
