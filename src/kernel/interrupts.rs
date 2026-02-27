//! Interrupt handling - unified interrupt service.
//!
//! Features:
//!   - Create and resolve interrupts
//!   - Pending interrupt queue per request/session
//!   - Interrupt lifecycle management
//!   - Config-driven behavior per interrupt kind

use crate::envelope::{FlowInterrupt, InterruptKind, InterruptResponse};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Interrupt Status
// =============================================================================

/// Status of an interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterruptStatus {
    /// Interrupt is awaiting response
    Pending,
    /// Interrupt has been resolved
    Resolved,
    /// Interrupt has expired
    Expired,
    /// Interrupt was cancelled
    Cancelled,
}

// =============================================================================
// Interrupt Config
// =============================================================================

/// Configuration for a specific interrupt kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptConfig {
    /// Default time-to-live for interrupts of this kind. None means no expiry.
    pub default_ttl: Option<Duration>,
    /// Determines if pending interrupts should auto-expire
    pub auto_expire: bool,
    /// Determines if a response is required to resolve
    pub require_response: bool,
}

impl InterruptConfig {
    /// Default configuration for an interrupt kind.
    pub fn default_for_kind(kind: InterruptKind) -> Self {
        match kind {
            InterruptKind::Clarification => Self {
                default_ttl: Some(Duration::hours(24)),
                auto_expire: true,
                require_response: true,
            },
            InterruptKind::Confirmation => Self {
                default_ttl: Some(Duration::hours(1)),
                auto_expire: true,
                require_response: true,
            },
            InterruptKind::AgentReview => Self {
                default_ttl: Some(Duration::minutes(30)),
                auto_expire: true,
                require_response: true,
            },
            InterruptKind::Checkpoint => Self {
                default_ttl: None,
                auto_expire: false,
                require_response: false,
            },
            InterruptKind::ResourceExhausted => Self {
                default_ttl: Some(Duration::minutes(5)),
                auto_expire: true,
                require_response: false,
            },
            InterruptKind::Timeout => Self {
                default_ttl: Some(Duration::minutes(5)),
                auto_expire: true,
                require_response: false,
            },
            InterruptKind::SystemError => Self {
                default_ttl: Some(Duration::hours(1)),
                auto_expire: true,
                require_response: false,
            },
        }
    }
}

// =============================================================================
// Kernel Interrupt (extended FlowInterrupt)
// =============================================================================

/// KernelInterrupt extends FlowInterrupt with kernel-level tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInterrupt {
    /// The underlying flow interrupt
    #[serde(flatten)]
    pub flow_interrupt: FlowInterrupt,

    // Kernel tracking
    /// Current status
    pub status: InterruptStatus,
    /// Request ID this interrupt belongs to
    pub request_id: String,
    /// User ID
    pub user_id: String,
    /// Session ID
    pub session_id: String,
    /// Envelope ID
    pub envelope_id: String,
    /// When the interrupt was resolved
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    /// Trace ID for distributed tracing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Span ID for distributed tracing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}

impl KernelInterrupt {
    /// Create a new kernel interrupt.
    pub fn new(
        kind: InterruptKind,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        ttl: Option<Duration>,
    ) -> Self {
        let now = Utc::now();
        let expires_at = ttl.map(|d| now + d);

        // Generate interrupt ID
        let id = format!("int_{}", &uuid::Uuid::new_v4().simple().to_string()[..16]);

        Self {
            flow_interrupt: FlowInterrupt {
                id,
                kind,
                created_at: now,
                expires_at,
                question: None,
                message: None,
                data: Some(HashMap::new()),
                response: None,
            },
            status: InterruptStatus::Pending,
            request_id,
            user_id,
            session_id,
            envelope_id,
            resolved_at: None,
            trace_id: None,
            span_id: None,
        }
    }

    /// Check if the interrupt has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.flow_interrupt.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if the interrupt is pending.
    pub fn is_pending(&self) -> bool {
        self.status == InterruptStatus::Pending
    }
}

// =============================================================================
// Interrupt Params
// =============================================================================

/// Parameters for creating a new interrupt.
#[derive(Debug, Clone)]
pub struct CreateInterruptParams {
    pub kind: InterruptKind,
    pub request_id: String,
    pub user_id: String,
    pub session_id: String,
    pub envelope_id: String,
    pub question: Option<String>,
    pub message: Option<String>,
    pub data: Option<HashMap<String, serde_json::Value>>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

// =============================================================================
// Interrupt Service
// =============================================================================

/// InterruptService manages interrupt lifecycle.
///
/// Single-actor implementation for creating, querying, and resolving interrupts.
#[derive(Debug)]
pub struct InterruptService {
    /// Configuration per interrupt kind
    configs: HashMap<InterruptKind, InterruptConfig>,
    /// In-memory store keyed by interrupt ID
    store: HashMap<String, KernelInterrupt>,
    /// Index by request ID for fast lookup
    by_request: HashMap<String, Vec<String>>,
    /// Index by session ID
    by_session: HashMap<String, Vec<String>>,
}

impl InterruptService {
    /// Create a new interrupt service with default configurations.
    pub fn new() -> Self {
        let mut configs = HashMap::new();

        // Initialize default configs for all interrupt kinds
        for kind in &[
            InterruptKind::Clarification,
            InterruptKind::Confirmation,
            InterruptKind::AgentReview,
            InterruptKind::Checkpoint,
            InterruptKind::ResourceExhausted,
            InterruptKind::Timeout,
            InterruptKind::SystemError,
        ] {
            configs.insert(*kind, InterruptConfig::default_for_kind(*kind));
        }

        Self {
            configs,
            store: HashMap::new(),
            by_request: HashMap::new(),
            by_session: HashMap::new(),
        }
    }

    /// Create with custom configurations.
    pub fn with_configs(custom_configs: HashMap<InterruptKind, InterruptConfig>) -> Self {
        let mut service = Self::new();
        // Override defaults with custom configs
        for (kind, config) in custom_configs {
            service.configs.insert(kind, config);
        }
        service
    }

    /// Get configuration for an interrupt kind.
    pub fn get_config(&self, kind: InterruptKind) -> InterruptConfig {
        self.configs
            .get(&kind)
            .cloned()
            .unwrap_or_else(|| InterruptConfig {
                default_ttl: Some(Duration::hours(1)),
                auto_expire: true,
                require_response: true,
            })
    }

    // =============================================================================
    // Create Interrupts
    // =============================================================================

    /// Create a new interrupt.
    pub fn create_interrupt(&mut self, params: CreateInterruptParams) -> KernelInterrupt {
        let config = self.get_config(params.kind);
        let mut interrupt = KernelInterrupt::new(
            params.kind,
            params.request_id.clone(),
            params.user_id,
            params.session_id.clone(),
            params.envelope_id,
            config.default_ttl,
        );

        // Apply options
        if let Some(q) = params.question {
            interrupt.flow_interrupt.question = Some(q);
        }
        if let Some(m) = params.message {
            interrupt.flow_interrupt.message = Some(m);
        }
        if let Some(d) = params.data {
            interrupt.flow_interrupt.data = Some(d);
        }
        interrupt.trace_id = params.trace_id;
        interrupt.span_id = params.span_id;

        // Store interrupt
        let interrupt_id = interrupt.flow_interrupt.id.clone();
        self.store.insert(interrupt_id.clone(), interrupt.clone());

        // Index by request
        self.by_request
            .entry(params.request_id)
            .or_default()
            .push(interrupt_id.clone());

        // Index by session
        self.by_session
            .entry(params.session_id)
            .or_default()
            .push(interrupt_id);

        interrupt
    }

    /// Convenience method to create a clarification interrupt.
    pub fn create_clarification(
        &mut self,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        question: String,
        context: Option<HashMap<String, serde_json::Value>>,
    ) -> KernelInterrupt {
        self.create_interrupt(CreateInterruptParams {
            kind: InterruptKind::Clarification,
            request_id,
            user_id,
            session_id,
            envelope_id,
            question: Some(question),
            message: None,
            data: context,
            trace_id: None,
            span_id: None,
        })
    }

    /// Convenience method to create a confirmation interrupt.
    pub fn create_confirmation(
        &mut self,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        message: String,
        action_data: Option<HashMap<String, serde_json::Value>>,
    ) -> KernelInterrupt {
        self.create_interrupt(CreateInterruptParams {
            kind: InterruptKind::Confirmation,
            request_id,
            user_id,
            session_id,
            envelope_id,
            question: None,
            message: Some(message),
            data: action_data,
            trace_id: None,
            span_id: None,
        })
    }

    /// Convenience method to create a resource exhausted interrupt.
    pub fn create_resource_exhausted(
        &mut self,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        resource_type: String,
        retry_after_seconds: f64,
    ) -> KernelInterrupt {
        let mut data = HashMap::new();
        data.insert(
            "resource_type".to_string(),
            serde_json::Value::String(resource_type.clone()),
        );
        data.insert(
            "retry_after_seconds".to_string(),
            serde_json::json!(retry_after_seconds),
        );

        self.create_interrupt(CreateInterruptParams {
            kind: InterruptKind::ResourceExhausted,
            request_id,
            user_id,
            session_id,
            envelope_id,
            question: None,
            message: Some(format!("Resource exhausted: {}", resource_type)),
            data: Some(data),
            trace_id: None,
            span_id: None,
        })
    }

    // =============================================================================
    // Query Interrupts
    // =============================================================================

    /// Get an interrupt by ID.
    pub fn get_interrupt(&self, interrupt_id: &str) -> Option<&KernelInterrupt> {
        self.store.get(interrupt_id)
    }

    /// Get the most recent pending interrupt for a request.
    pub fn get_pending_for_request(&self, request_id: &str) -> Option<&KernelInterrupt> {
        let interrupt_ids = self.by_request.get(request_id)?;

        // Return most recent pending (iterate backwards)
        for interrupt_id in interrupt_ids.iter().rev() {
            if let Some(interrupt) = self.store.get(interrupt_id) {
                if interrupt.is_pending() && !interrupt.is_expired() {
                    return Some(interrupt);
                }
            }
        }
        None
    }

    /// Get all pending interrupts for a session, optionally filtered by kind.
    pub fn get_pending_for_session(
        &self,
        session_id: &str,
        kinds: Option<&[InterruptKind]>,
    ) -> Vec<&KernelInterrupt> {
        let interrupt_ids = match self.by_session.get(session_id) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let kind_set: Option<std::collections::HashSet<InterruptKind>> =
            kinds.map(|ks| ks.iter().copied().collect());

        interrupt_ids
            .iter()
            .filter_map(|id| self.store.get(id))
            .filter(|interrupt| interrupt.is_pending() && !interrupt.is_expired())
            .filter(|interrupt| {
                kind_set
                    .as_ref()
                    .map(|set| set.contains(&interrupt.flow_interrupt.kind))
                    .unwrap_or(true)
            })
            .collect()
    }

    // =============================================================================
    // Resolve Interrupts
    // =============================================================================

    /// Resolve an interrupt with a response.
    /// Returns true if successful, false if not found or invalid.
    pub fn resolve(
        &mut self,
        interrupt_id: &str,
        response: InterruptResponse,
        user_id: Option<&str>,
    ) -> bool {
        let interrupt = match self.store.get_mut(interrupt_id) {
            Some(i) => i,
            None => return false,
        };

        if interrupt.status != InterruptStatus::Pending {
            return false;
        }

        // Validate user if provided
        if let Some(uid) = user_id {
            if interrupt.user_id != uid {
                return false;
            }
        }

        // Update interrupt
        let now = Utc::now();
        let mut updated_response = response;
        updated_response.received_at = now;
        interrupt.flow_interrupt.response = Some(updated_response);
        interrupt.status = InterruptStatus::Resolved;
        interrupt.resolved_at = Some(now);

        true
    }

    /// Cancel an interrupt.
    pub fn cancel(&mut self, interrupt_id: &str, reason: String) -> bool {
        let interrupt = match self.store.get_mut(interrupt_id) {
            Some(i) => i,
            None => return false,
        };

        if interrupt.status != InterruptStatus::Pending {
            return false;
        }

        interrupt.status = InterruptStatus::Cancelled;

        // Ensure data exists
        if interrupt.flow_interrupt.data.is_none() {
            interrupt.flow_interrupt.data = Some(HashMap::new());
        }
        if let Some(data) = &mut interrupt.flow_interrupt.data {
            data.insert("cancel_reason".to_string(), serde_json::json!(reason));
        }

        true
    }

    /// Expire all pending interrupts that have passed their expiry time.
    /// Returns the number of interrupts expired.
    pub fn expire_pending(&mut self) -> usize {
        let mut count = 0;

        for interrupt in self.store.values_mut() {
            if interrupt.is_pending() && interrupt.is_expired() {
                interrupt.status = InterruptStatus::Expired;
                count += 1;
            }
        }

        count
    }

    // =============================================================================
    // Cleanup
    // =============================================================================

    /// Remove resolved/expired/cancelled interrupts older than the given duration.
    /// Returns the number of interrupts cleaned up.
    pub fn cleanup_resolved(&mut self, older_than: Duration) -> usize {
        let cutoff = Utc::now() - older_than;
        let mut to_delete = Vec::new();

        for (id, interrupt) in &self.store {
            if interrupt.status != InterruptStatus::Pending
                && interrupt.flow_interrupt.created_at < cutoff
            {
                to_delete.push(id.clone());
            }
        }

        let count = to_delete.len();
        for id in to_delete {
            if let Some(interrupt) = self.store.remove(&id) {
                // Remove from indices
                Self::remove_from_index(&mut self.by_request, &interrupt.request_id, &id);
                Self::remove_from_index(&mut self.by_session, &interrupt.session_id, &id);
            }
        }

        count
    }

    /// Remove an interrupt from an index slice.
    fn remove_from_index(index: &mut HashMap<String, Vec<String>>, key: &str, interrupt_id: &str) {
        if let Some(interrupts) = index.get_mut(key) {
            interrupts.retain(|id| id != interrupt_id);
            if interrupts.is_empty() {
                index.remove(key);
            }
        }
    }

    // =============================================================================
    // Statistics
    // =============================================================================

    /// Get statistics about interrupts.
    pub fn get_stats(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        stats.insert("total".to_string(), self.store.len());
        stats.insert("pending".to_string(), 0);
        stats.insert("resolved".to_string(), 0);
        stats.insert("expired".to_string(), 0);
        stats.insert("cancelled".to_string(), 0);

        for interrupt in self.store.values() {
            let key = match interrupt.status {
                InterruptStatus::Pending => "pending",
                InterruptStatus::Resolved => "resolved",
                InterruptStatus::Expired => "expired",
                InterruptStatus::Cancelled => "cancelled",
            };
            if let Some(count) = stats.get_mut(key) {
                *count += 1;
            }
        }

        stats
    }

    /// Get the number of pending interrupts.
    pub fn get_pending_count(&self) -> usize {
        self.store
            .values()
            .filter(|i| i.is_pending())
            .count()
    }
}

impl Default for InterruptService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_clarification_interrupt() {
        let mut service = InterruptService::new();
        let interrupt = service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "What should I do next?".to_string(),
            None,
        );

        assert_eq!(interrupt.flow_interrupt.kind, InterruptKind::Clarification);
        assert_eq!(interrupt.status, InterruptStatus::Pending);
        assert_eq!(interrupt.request_id, "req1");
        assert_eq!(interrupt.user_id, "user1");
        assert_eq!(interrupt.session_id, "sess1");
        assert!(interrupt.flow_interrupt.question.is_some());
        assert_eq!(service.get_pending_count(), 1);
    }

    #[test]
    fn test_create_confirmation_interrupt() {
        let mut service = InterruptService::new();
        let interrupt = service.create_confirmation(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Should I delete this file?".to_string(),
            None,
        );

        assert_eq!(interrupt.flow_interrupt.kind, InterruptKind::Confirmation);
        assert_eq!(interrupt.status, InterruptStatus::Pending);
        assert!(interrupt.flow_interrupt.message.is_some());
        assert_eq!(service.get_pending_count(), 1);
    }

    #[test]
    fn test_resolve_interrupt_with_approval() {
        let mut service = InterruptService::new();
        let interrupt = service.create_confirmation(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Proceed?".to_string(),
            None,
        );

        let interrupt_id = interrupt.flow_interrupt.id.clone();
        let response = InterruptResponse {
            text: Some("Yes, proceed".to_string()),
            approved: Some(true),
            decision: None,
            data: None,
            received_at: Utc::now(),
        };

        let success = service.resolve(&interrupt_id, response, Some("user1"));
        assert!(success);

        let resolved = service.get_interrupt(&interrupt_id).unwrap();
        assert_eq!(resolved.status, InterruptStatus::Resolved);
        assert!(resolved.flow_interrupt.response.is_some());
        assert_eq!(resolved.flow_interrupt.response.as_ref().unwrap().approved, Some(true));
    }

    #[test]
    fn test_resolve_wrong_user_fails() {
        let mut service = InterruptService::new();
        let interrupt = service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Question?".to_string(),
            None,
        );

        let interrupt_id = interrupt.flow_interrupt.id.clone();
        let response = InterruptResponse {
            text: Some("Answer".to_string()),
            approved: Some(true),
            decision: None,
            data: None,
            received_at: Utc::now(),
        };

        // Try to resolve with wrong user
        let success = service.resolve(&interrupt_id, response, Some("user2"));
        assert!(!success);

        // Interrupt should still be pending
        let still_pending = service.get_interrupt(&interrupt_id).unwrap();
        assert_eq!(still_pending.status, InterruptStatus::Pending);
    }

    #[test]
    fn test_cancel_interrupt() {
        let mut service = InterruptService::new();
        let interrupt = service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Question?".to_string(),
            None,
        );

        let interrupt_id = interrupt.flow_interrupt.id.clone();
        let success = service.cancel(&interrupt_id, "User cancelled operation".to_string());
        assert!(success);

        let cancelled = service.get_interrupt(&interrupt_id).unwrap();
        assert_eq!(cancelled.status, InterruptStatus::Cancelled);
        assert!(cancelled.flow_interrupt.data.is_some());
        assert!(cancelled.flow_interrupt.data.as_ref().unwrap().contains_key("cancel_reason"));
    }

    #[test]
    fn test_expire_pending_interrupts() {
        let mut service = InterruptService::new();

        // Create interrupt with very short TTL (1 second)
        let interrupt = service.create_interrupt(CreateInterruptParams {
            kind: InterruptKind::Confirmation,
            request_id: "req1".to_string(),
            user_id: "user1".to_string(),
            session_id: "sess1".to_string(),
            envelope_id: "env1".to_string(),
            question: Some("Quick question?".to_string()),
            message: None,
            data: None,
            trace_id: None,
            span_id: None,
        });

        // Manually set expiry to past
        let interrupt_id = interrupt.flow_interrupt.id.clone();
        if let Some(int) = service.store.get_mut(&interrupt_id) {
            int.flow_interrupt.expires_at = Some(Utc::now() - Duration::minutes(1));
        }

        let expired_count = service.expire_pending();
        assert_eq!(expired_count, 1);

        let expired = service.get_interrupt(&interrupt_id).unwrap();
        assert_eq!(expired.status, InterruptStatus::Expired);
    }

    #[test]
    fn test_get_pending_for_request() {
        let mut service = InterruptService::new();

        // Create multiple interrupts for same request
        service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "First question?".to_string(),
            None,
        );

        let second = service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env2".to_string(),
            "Second question?".to_string(),
            None,
        );

        // Should return most recent pending
        let pending = service.get_pending_for_request("req1");
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().flow_interrupt.id, second.flow_interrupt.id);
    }

    #[test]
    fn test_get_pending_for_session_filtered() {
        let mut service = InterruptService::new();

        // Create interrupts of different kinds
        service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Clarification?".to_string(),
            None,
        );

        service.create_confirmation(
            "req2".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env2".to_string(),
            "Confirmation?".to_string(),
            None,
        );

        service.create_clarification(
            "req3".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env3".to_string(),
            "Another clarification?".to_string(),
            None,
        );

        // Get all pending for session
        let all_pending = service.get_pending_for_session("sess1", None);
        assert_eq!(all_pending.len(), 3);

        // Filter by clarification kind only
        let clarifications = service.get_pending_for_session(
            "sess1",
            Some(&[InterruptKind::Clarification]),
        );
        assert_eq!(clarifications.len(), 2);

        // Filter by confirmation kind only
        let confirmations = service.get_pending_for_session(
            "sess1",
            Some(&[InterruptKind::Confirmation]),
        );
        assert_eq!(confirmations.len(), 1);
    }

    #[test]
    fn test_cleanup_resolved() {
        let mut service = InterruptService::new();

        // Create and resolve an interrupt
        let interrupt = service.create_confirmation(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Old interrupt".to_string(),
            None,
        );

        let interrupt_id = interrupt.flow_interrupt.id.clone();
        let response = InterruptResponse {
            text: None,
            approved: Some(true),
            decision: None,
            data: None,
            received_at: Utc::now(),
        };
        service.resolve(&interrupt_id, response, None);

        // Create a pending interrupt (should not be cleaned)
        service.create_clarification(
            "req2".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env2".to_string(),
            "New interrupt".to_string(),
            None,
        );

        assert_eq!(service.store.len(), 2);

        // Cleanup resolved interrupts older than 0 seconds (should remove resolved)
        let cleaned = service.cleanup_resolved(Duration::seconds(0));
        assert_eq!(cleaned, 1);
        assert_eq!(service.store.len(), 1);
    }

    #[test]
    fn test_interrupt_ttl_config() {
        let service = InterruptService::new();

        // Check default TTLs for different kinds
        let clarification_config = service.get_config(InterruptKind::Clarification);
        assert!(clarification_config.default_ttl.is_some());
        assert_eq!(clarification_config.default_ttl.unwrap().num_hours(), 24);

        let confirmation_config = service.get_config(InterruptKind::Confirmation);
        assert!(confirmation_config.default_ttl.is_some());
        assert_eq!(confirmation_config.default_ttl.unwrap().num_hours(), 1);

        let checkpoint_config = service.get_config(InterruptKind::Checkpoint);
        assert!(checkpoint_config.default_ttl.is_none());
    }

    #[test]
    fn test_interrupt_stats() {
        let mut service = InterruptService::new();

        // Create various interrupts
        let int1 = service.create_clarification(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "Q1?".to_string(),
            None,
        );

        let int2 = service.create_confirmation(
            "req2".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env2".to_string(),
            "Q2?".to_string(),
            None,
        );

        // Resolve one
        let response = InterruptResponse {
            text: None,
            approved: Some(true),
            decision: None,
            data: None,
            received_at: Utc::now(),
        };
        service.resolve(&int1.flow_interrupt.id, response, None);

        // Cancel one
        service.cancel(&int2.flow_interrupt.id, "Cancelled".to_string());

        // Create one more pending
        service.create_clarification(
            "req3".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env3".to_string(),
            "Q3?".to_string(),
            None,
        );

        let stats = service.get_stats();
        assert_eq!(stats.get("total"), Some(&3));
        assert_eq!(stats.get("pending"), Some(&1));
        assert_eq!(stats.get("resolved"), Some(&1));
        assert_eq!(stats.get("cancelled"), Some(&1));
        assert_eq!(stats.get("expired"), Some(&0));
    }

    #[test]
    fn test_resource_exhausted_interrupt() {
        let mut service = InterruptService::new();
        let interrupt = service.create_resource_exhausted(
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            "llm_calls".to_string(),
            60.0,
        );

        assert_eq!(interrupt.flow_interrupt.kind, InterruptKind::ResourceExhausted);
        assert!(interrupt.flow_interrupt.message.is_some());
        assert!(interrupt.flow_interrupt.data.is_some());

        let data = interrupt.flow_interrupt.data.as_ref().unwrap();
        assert_eq!(
            data.get("resource_type").unwrap().as_str().unwrap(),
            "llm_calls"
        );
    }
}
