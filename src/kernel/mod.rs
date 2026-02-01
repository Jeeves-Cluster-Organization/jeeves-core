//! Kernel - the main orchestration actor.
//!
//! The Kernel owns all mutable state and processes gRPC commands via a single
//! message channel. Subsystems (lifecycle, resources, interrupts, rate limiter)
//! are plain structs owned by the Kernel, not separate actors.

use std::collections::HashMap;

// Core types
pub mod types;

// Subsystem modules
pub mod interrupts;
pub mod lifecycle;
pub mod orchestrator;
pub mod rate_limiter;
pub mod resources;

// Re-export key types
pub use lifecycle::LifecycleManager;
pub use rate_limiter::{RateLimitConfig, RateLimiter};
pub use resources::ResourceTracker;
pub use types::{
    ProcessControlBlock, ProcessState, ResourceQuota, ResourceUsage, SchedulingPriority,
};

use crate::envelope::{Envelope, FlowInterrupt};
use crate::types::{Error, Result};

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

    /// Envelope storage (pid -> envelope)
    envelopes: HashMap<String, Envelope>,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            lifecycle: LifecycleManager::default(),
            resources: ResourceTracker::default(),
            rate_limiter: RateLimiter::default(),
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
            envelopes: HashMap::new(),
        }
    }

    /// Create a new process for an envelope.
    pub fn create_process(
        &mut self,
        envelope_id: String,
        request_id: String,
        user_id: String,
        session_id: String,
        priority: SchedulingPriority,
        quota: Option<ResourceQuota>,
    ) -> Result<ProcessControlBlock> {
        // Check rate limit
        self.rate_limiter.check_rate_limit(&user_id)?;

        // Submit process to lifecycle manager
        let pcb = self.lifecycle.submit(
            envelope_id.clone(),
            request_id,
            user_id,
            session_id,
            priority,
            quota,
        )?;

        // Schedule it
        self.lifecycle.schedule(&envelope_id)?;

        Ok(pcb)
    }

    /// Get process by PID.
    pub fn get_process(&self, pid: &str) -> Option<&ProcessControlBlock> {
        self.lifecycle.get(pid)
    }

    /// Store envelope.
    pub fn store_envelope(&mut self, envelope: Envelope) {
        self.envelopes
            .insert(envelope.envelope_id.clone(), envelope);
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
    pub fn check_quota(&self, pid: &str) -> Result<()> {
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
    pub fn start_process(&mut self, pid: &str) -> Result<()> {
        self.lifecycle.start(pid)
    }

    /// Block a process (e.g., resource exhausted).
    pub fn block_process(&mut self, pid: &str, reason: String) -> Result<()> {
        self.lifecycle.block(pid, reason)
    }

    /// Wait a process (e.g., awaiting interrupt response).
    pub fn wait_process(&mut self, pid: &str, interrupt: FlowInterrupt) -> Result<()> {
        self.lifecycle.wait(pid, interrupt.kind)?;
        // Also set interrupt on envelope
        if let Some(env) = self.envelopes.get_mut(pid) {
            env.set_interrupt(interrupt);
        }
        Ok(())
    }

    /// Resume a process from waiting/blocked.
    pub fn resume_process(&mut self, pid: &str) -> Result<()> {
        self.lifecycle.resume(pid)?;
        // Clear interrupt on envelope
        if let Some(env) = self.envelopes.get_mut(pid) {
            env.clear_interrupt();
        }
        Ok(())
    }

    /// Terminate a process.
    pub fn terminate_process(&mut self, pid: &str) -> Result<()> {
        self.lifecycle.terminate(pid)?;
        // Terminate envelope
        if let Some(env) = self.envelopes.get_mut(pid) {
            env.terminate("Process terminated");
        }
        Ok(())
    }

    /// Cleanup and remove a terminated process.
    pub fn cleanup_process(&mut self, pid: &str) -> Result<()> {
        self.lifecycle.cleanup(pid)?;
        self.lifecycle.remove(pid)?;
        self.envelopes.remove(pid);
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
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}
