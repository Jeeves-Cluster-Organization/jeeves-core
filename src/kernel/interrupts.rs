//! Tool-confirmation interrupt service.
//!
//! Tracks pending `FlowInterrupt`s by id and the consumer-supplied responses.
//! The kernel uses this to suspend a stage when an agent requests
//! confirmation and to thread the response back into the next agent dispatch.

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::run::{FlowInterrupt, InterruptResponse};
use crate::types::{EnvelopeId, InterruptId, RequestId, SessionId, UserId};

/// Lightweight bookkeeping for a pending interrupt.
#[derive(Debug, Clone)]
pub struct PendingInterrupt {
    pub interrupt: FlowInterrupt,
    pub request_id: RequestId,
    pub user_id: UserId,
    pub session_id: SessionId,
    pub envelope_id: EnvelopeId,
    pub registered_at: DateTime<Utc>,
}

/// Lightweight registry: pending interrupts by id + resolved responses.
///
/// Held by `Kernel` and accessed via `&mut self`. No state machine, no TTL,
/// no kind discrimination — `FlowInterrupt` self-describes via its
/// `message` / `question` / `data` fields.
#[derive(Debug, Default)]
pub struct InterruptService {
    pending: HashMap<InterruptId, PendingInterrupt>,
    resolved: HashMap<InterruptId, InterruptResponse>,
}

impl InterruptService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a `FlowInterrupt` so it can be looked up + resolved by id.
    pub fn register_flow_interrupt(
        &mut self,
        interrupt: FlowInterrupt,
        request_id: &RequestId,
        user_id: &UserId,
        session_id: &SessionId,
        envelope_id: &EnvelopeId,
    ) {
        let id = interrupt.id.clone();
        self.pending.insert(
            id,
            PendingInterrupt {
                interrupt,
                request_id: request_id.clone(),
                user_id: user_id.clone(),
                session_id: session_id.clone(),
                envelope_id: envelope_id.clone(),
                registered_at: Utc::now(),
            },
        );
    }

    /// Resolve a pending interrupt with the consumer's response.
    /// Returns true if `interrupt_id` was registered.
    pub fn resolve(
        &mut self,
        interrupt_id: &str,
        response: InterruptResponse,
    ) -> bool {
        if self.pending.remove(interrupt_id).is_some() {
            self.resolved.insert(InterruptId::must(interrupt_id), response);
            true
        } else {
            false
        }
    }

    /// Look up a pending interrupt by id.
    pub fn get_pending(&self, interrupt_id: &str) -> Option<&PendingInterrupt> {
        self.pending.get(interrupt_id)
    }

    /// Look up a resolved response by id.
    pub fn get_response(&self, interrupt_id: &str) -> Option<&InterruptResponse> {
        self.resolved.get(interrupt_id)
    }

    /// Number of currently pending interrupts.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_interrupt() -> FlowInterrupt {
        FlowInterrupt::new().with_message("Approve destructive op?".into())
    }

    fn make_response() -> InterruptResponse {
        InterruptResponse {
            text: None,
            approved: Some(true),
            decision: None,
            data: None,
            received_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn register_and_resolve_round_trip() {
        let mut svc = InterruptService::new();
        let interrupt = make_interrupt();
        let id = interrupt.id.clone();

        svc.register_flow_interrupt(
            interrupt,
            &RequestId::must("req"),
            &UserId::must("user"),
            &SessionId::must("sess"),
            &EnvelopeId::must("env"),
        );
        assert_eq!(svc.pending_count(), 1);
        assert!(svc.get_pending(id.as_str()).is_some());

        assert!(svc.resolve(id.as_str(), make_response()));
        assert_eq!(svc.pending_count(), 0);
        assert!(svc.get_response(id.as_str()).is_some());
    }

    #[test]
    fn resolve_unknown_returns_false() {
        let mut svc = InterruptService::new();
        assert!(!svc.resolve("nonexistent", make_response()));
    }
}
