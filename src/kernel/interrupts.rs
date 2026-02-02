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
    pub resolved_at: Option<DateTime<Utc>>,
    /// Trace ID for distributed tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    /// Span ID for distributed tracing
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let id = format!("int_{}", uuid::Uuid::new_v4().simple().to_string()[..16].to_string());

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
    pub fn create_interrupt(
        &mut self,
        kind: InterruptKind,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        question: Option<String>,
        message: Option<String>,
        data: Option<HashMap<String, serde_json::Value>>,
        trace_id: Option<String>,
        span_id: Option<String>,
    ) -> KernelInterrupt {
        let config = self.get_config(kind);
        let mut interrupt = KernelInterrupt::new(
            kind,
            request_id.clone(),
            user_id,
            session_id.clone(),
            envelope_id,
            config.default_ttl,
        );

        // Apply options
        if let Some(q) = question {
            interrupt.flow_interrupt.question = Some(q);
        }
        if let Some(m) = message {
            interrupt.flow_interrupt.message = Some(m);
        }
        if let Some(d) = data {
            interrupt.flow_interrupt.data = Some(d);
        }
        interrupt.trace_id = trace_id;
        interrupt.span_id = span_id;

        // Store interrupt
        let interrupt_id = interrupt.flow_interrupt.id.clone();
        self.store.insert(interrupt_id.clone(), interrupt.clone());

        // Index by request
        self.by_request
            .entry(request_id)
            .or_insert_with(Vec::new)
            .push(interrupt_id.clone());

        // Index by session
        self.by_session
            .entry(session_id)
            .or_insert_with(Vec::new)
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
        self.create_interrupt(
            InterruptKind::Clarification,
            request_id,
            user_id,
            session_id,
            envelope_id,
            Some(question),
            None,
            context,
            None,
            None,
        )
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
        self.create_interrupt(
            InterruptKind::Confirmation,
            request_id,
            user_id,
            session_id,
            envelope_id,
            None,
            Some(message),
            action_data,
            None,
            None,
        )
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

        self.create_interrupt(
            InterruptKind::ResourceExhausted,
            request_id,
            user_id,
            session_id,
            envelope_id,
            None,
            Some(format!("Resource exhausted: {}", resource_type)),
            Some(data),
            None,
            None,
        )
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
            *stats.get_mut(key).unwrap() += 1;
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
