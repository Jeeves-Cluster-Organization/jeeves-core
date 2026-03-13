//! Kernel event emission — CommBus envelope snapshots and pub/sub.

use crate::types::{ProcessId, Result};

use super::Kernel;

impl Kernel {
    /// Subscribe to envelope snapshot events (call before spawning actor).
    /// Returns receiver that yields every envelope mutation event.
    pub fn subscribe_snapshots(&mut self) -> Result<tokio::sync::mpsc::UnboundedReceiver<crate::commbus::Event>> {
        let sub_id = format!("snap_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let (_sub, rx) = self.commbus.subscribe(sub_id, vec!["envelope.snapshot".to_string()])?;
        Ok(rx)
    }

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

    /// Publish an event to subscribers.
    pub fn publish_event(&mut self, event: crate::commbus::Event) -> Result<usize> {
        self.commbus.publish(event)
    }
}
