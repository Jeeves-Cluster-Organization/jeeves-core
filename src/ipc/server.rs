//! TCP IPC server — accept loop and per-connection handler.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ipc::codec::{
    read_frame, write_frame, MSG_ERROR, MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK, MSG_STREAM_END,
};
use crate::ipc::dispatch::{self, DispatchResponse};
use crate::kernel::Kernel;
use crate::types::IpcConfig;

/// IPC server wrapping the kernel.
#[derive(Debug)]
pub struct IpcServer {
    kernel: Arc<Mutex<Kernel>>,
    addr: SocketAddr,
    cancel: CancellationToken,
    ipc_config: IpcConfig,
}

impl IpcServer {
    pub fn new(kernel: Arc<Mutex<Kernel>>, addr: SocketAddr, ipc_config: IpcConfig) -> Self {
        Self {
            kernel,
            addr,
            cancel: CancellationToken::new(),
            ipc_config,
        }
    }

    /// Run the server until cancelled or a fatal error occurs.
    pub async fn serve(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(self.addr).await?;
        tracing::info!("IPC server listening on {}", self.addr);

        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    tracing::info!("IPC server shutting down");
                    break;
                }
                accept = listener.accept() => {
                    let (stream, peer) = accept?;
                    tracing::debug!("IPC connection from {}", peer);
                    let kernel = self.kernel.clone();
                    let cancel = self.cancel.clone();
                    let ipc_config = self.ipc_config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, kernel, cancel, ipc_config).await {
                            tracing::warn!("Connection from {} error: {}", peer, e);
                        }
                    });
                }
            }
        }
        Ok(())
    }

    /// Request graceful shutdown.
    #[allow(dead_code)]
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Handle a single TCP connection: read frames → dispatch → write responses.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    kernel: Arc<Mutex<Kernel>>,
    cancel: CancellationToken,
    ipc_config: IpcConfig,
) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            frame = read_frame(&mut reader, ipc_config.max_frame_bytes) => {
                let frame = match frame? {
                    Some(f) => f,
                    None => break, // clean EOF
                };

                let (msg_type, payload_bytes) = frame;

                if msg_type != MSG_REQUEST {
                    let err_payload = serde_json::json!({
                        "id": "",
                        "ok": false,
                        "error": {
                            "code": "INVALID_ARGUMENT",
                            "message": format!("Unexpected message type: 0x{:02X}", msg_type),
                        }
                    });
                    let encoded = rmp_serde::to_vec_named(&err_payload)
                        .unwrap_or_default();
                    write_frame(&mut writer, MSG_ERROR, &encoded).await?;
                    continue;
                }

                // Decode msgpack request
                let request: serde_json::Value = match rmp_serde::from_slice(&payload_bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_payload = serde_json::json!({
                            "id": "",
                            "ok": false,
                            "error": {
                                "code": "INVALID_ARGUMENT",
                                "message": format!("Invalid msgpack: {}", e),
                            }
                        });
                        let encoded = rmp_serde::to_vec_named(&err_payload)
                            .unwrap_or_default();
                        write_frame(&mut writer, MSG_ERROR, &encoded).await?;
                        continue;
                    }
                };

                let request_id = request.get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let service = request.get("service")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let method = request.get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let body = request.get("body")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                // Dispatch (kernel lock released before response writing)
                let mut kernel_guard = kernel.lock().await;
                let result = dispatch::dispatch(&mut kernel_guard, service, method, body, &ipc_config).await;
                drop(kernel_guard);

                match result {
                    Ok(DispatchResponse::Single(response_body)) => {
                        let response = serde_json::json!({
                            "id": request_id,
                            "ok": true,
                            "body": response_body,
                        });
                        let encoded = rmp_serde::to_vec_named(&response)
                            .unwrap_or_default();
                        write_frame(&mut writer, MSG_RESPONSE, &encoded).await?;
                    }
                    Ok(DispatchResponse::Stream(mut rx)) => {
                        // Stream chunks until the sender closes
                        while let Some(chunk) = rx.recv().await {
                            let frame = serde_json::json!({
                                "id": request_id,
                                "body": chunk,
                            });
                            let encoded = rmp_serde::to_vec_named(&frame)
                                .unwrap_or_default();
                            write_frame(&mut writer, MSG_STREAM_CHUNK, &encoded).await?;
                        }
                        // End-of-stream sentinel
                        let end = serde_json::json!({ "id": request_id });
                        let encoded = rmp_serde::to_vec_named(&end)
                            .unwrap_or_default();
                        write_frame(&mut writer, MSG_STREAM_END, &encoded).await?;
                    }
                    Err(e) => {
                        let response = serde_json::json!({
                            "id": request_id,
                            "ok": false,
                            "error": {
                                "code": e.to_ipc_error_code(),
                                "message": e.to_string(),
                            }
                        });
                        let encoded = rmp_serde::to_vec_named(&response)
                            .unwrap_or_default();
                        write_frame(&mut writer, MSG_ERROR, &encoded).await?;
                    }
                }
            }
        }
    }

    Ok(())
}
