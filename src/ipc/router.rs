//! Top-level IPC router — routes by service, delegates to handlers.

use crate::ipc::handlers;
use crate::kernel::Kernel;
use crate::types::{Error, IpcConfig, Result};
use serde_json::Value;
use tokio::sync::mpsc;

/// Result from dispatching a request.
#[allow(missing_debug_implementations)]
pub enum DispatchResponse {
    /// Single response value (most endpoints).
    Single(Value),
    /// Streaming response — server writes each value as MSG_STREAM_CHUNK,
    /// then MSG_STREAM_END when the receiver closes.
    Stream(mpsc::Receiver<Value>),
}

/// Route an IPC request to the appropriate service handler.
pub async fn route_request(
    kernel: &mut Kernel,
    service: &str,
    method: &str,
    body: Value,
    ipc_config: &IpcConfig,
) -> Result<DispatchResponse> {
    match service {
        "kernel" => handlers::kernel::handle(kernel, method, body).await,
        "orchestration" => handlers::orchestration::handle(kernel, method, body).await,
        "commbus" => handlers::commbus::handle(kernel, method, body, ipc_config).await,
        "interrupt" => handlers::interrupt::handle(kernel, method, body).await,
        _ => Err(Error::not_found(format!("Unknown service: {}", service))),
    }
}

// =============================================================================
// Shared helpers — used by all handler modules
// =============================================================================

pub fn str_field(body: &Value, key: &str) -> Result<String> {
    body.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::validation(format!("Missing required field: {}", key)))
}
