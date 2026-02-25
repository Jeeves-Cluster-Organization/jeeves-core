//! TCP IPC server — accept loop and per-connection handler.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::ipc::codec::{
    read_frame, write_frame, MSG_ERROR, MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK, MSG_STREAM_END,
};
use crate::ipc::router::{self, DispatchResponse};
use crate::kernel::Kernel;
use crate::types::IpcConfig;

const SUPPORTED_PROTOCOL_MAJOR: u64 = 1;

fn required_str_field<'a>(request: &'a serde_json::Value, key: &str) -> std::io::Result<&'a str> {
    request
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Missing required request field: {}", key),
            )
        })
}

/// Encode a JSON value to msgpack. Logs and returns an error on failure
/// instead of silently producing an empty vec.
fn encode_msgpack(value: &serde_json::Value) -> std::io::Result<Vec<u8>> {
    rmp_serde::to_vec_named(value).map_err(|e| {
        tracing::error!("Msgpack encoding failed: {}", e);
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })
}

/// IPC server wrapping the kernel.
#[derive(Debug)]
pub struct IpcServer {
    kernel_tx: mpsc::Sender<KernelCommand>,
    addr: SocketAddr,
    cancel: CancellationToken,
    ipc_config: IpcConfig,
}

struct KernelCommand {
    service: String,
    method: String,
    body: serde_json::Value,
    response_tx: oneshot::Sender<crate::types::Result<DispatchResponse>>,
}

impl IpcServer {
    pub fn new(kernel: Kernel, addr: SocketAddr, ipc_config: IpcConfig) -> Self {
        let (kernel_tx, kernel_rx) = mpsc::channel(ipc_config.kernel_queue_capacity);
        let cancel = CancellationToken::new();
        tokio::spawn(run_kernel_actor(
            kernel,
            kernel_rx,
            cancel.clone(),
            ipc_config.clone(),
        ));

        Self {
            kernel_tx,
            addr,
            cancel,
            ipc_config,
        }
    }

    /// Run the server until cancelled or a fatal error occurs.
    pub async fn serve(&self) -> std::io::Result<()> {
        let listener = TcpListener::bind(self.addr).await?;
        let conn_semaphore = Arc::new(Semaphore::new(self.ipc_config.max_connections));
        tracing::info!(
            "IPC server listening on {} (max_connections={})",
            self.addr,
            self.ipc_config.max_connections,
        );

        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    tracing::info!("IPC server shutting down");
                    break;
                }
                accept = listener.accept() => {
                    let (stream, peer) = accept?;

                    // Acquire connection permit (backpressure when at capacity).
                    let permit = match conn_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            tracing::warn!(
                                "Connection from {} rejected: at max_connections ({})",
                                peer,
                                self.ipc_config.max_connections,
                            );
                            drop(stream);
                            continue;
                        }
                    };

                    tracing::debug!("IPC connection from {} (active={})",
                        peer,
                        self.ipc_config.max_connections - conn_semaphore.available_permits(),
                    );
                    let kernel_tx = self.kernel_tx.clone();
                    let cancel = self.cancel.clone();
                    let ipc_config = self.ipc_config.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, kernel_tx, cancel, ipc_config, permit).await {
                            tracing::warn!("Connection from {} error: {}", peer, e);
                        }
                        // permit is dropped here, releasing the connection slot
                    });
                }
            }
        }
        Ok(())
    }

    /// Request graceful shutdown.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

/// Handle a single TCP connection: read frames → dispatch → write responses.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    kernel_tx: mpsc::Sender<KernelCommand>,
    cancel: CancellationToken,
    ipc_config: IpcConfig,
    _permit: OwnedSemaphorePermit, // held for connection lifetime
) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let read_timeout = Duration::from_secs(ipc_config.read_timeout_secs);
    let write_timeout = Duration::from_secs(ipc_config.write_timeout_secs);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            frame_result = tokio::time::timeout(read_timeout, read_frame(&mut reader, ipc_config.max_frame_bytes)) => {
                let frame = match frame_result {
                    Err(_elapsed) => {
                        tracing::debug!("Read timeout ({}s), dropping connection", ipc_config.read_timeout_secs);
                        break;
                    }
                    Ok(result) => match result? {
                        Some(f) => f,
                        None => break, // clean EOF
                    },
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
                    let encoded = encode_msgpack(&err_payload)?;
                    timed_write(&mut writer, MSG_ERROR, &encoded, write_timeout).await?;
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
                        let encoded = encode_msgpack(&err_payload)?;
                        timed_write(&mut writer, MSG_ERROR, &encoded, write_timeout).await?;
                        continue;
                    }
                };

                let request_id = required_str_field(&request, "id")?.to_string();
                let service = required_str_field(&request, "service")?;
                let method = required_str_field(&request, "method")?;
                let body = request
                    .get("body")
                    .cloned()
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Missing required request field: body",
                        )
                    })?;

                let protocol_major = request
                    .get("protocol_version")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Missing required request field: protocol_version",
                        )
                    })?;
                if protocol_major != SUPPORTED_PROTOCOL_MAJOR {
                    let response = serde_json::json!({
                        "id": request_id,
                        "ok": false,
                        "error": {
                            "code": "INVALID_ARGUMENT",
                            "message": format!(
                                "Unsupported protocol_version {} (supported: {})",
                                protocol_major, SUPPORTED_PROTOCOL_MAJOR
                            ),
                            "supported_protocol_major": SUPPORTED_PROTOCOL_MAJOR,
                        }
                    });
                    let encoded = encode_msgpack(&response)?;
                    timed_write(&mut writer, MSG_ERROR, &encoded, write_timeout).await?;
                    continue;
                }

                let (response_tx, response_rx) = oneshot::channel();
                let command = KernelCommand {
                    service: service.to_string(),
                    method: method.to_string(),
                    body,
                    response_tx,
                };

                kernel_tx.try_send(command).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        format!("Kernel actor unavailable: {}", e),
                    )
                })?;

                let result = response_rx.await.map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Kernel actor terminated while request was in flight",
                    )
                })?;

                match result {
                    Ok(DispatchResponse::Single(response_body)) => {
                        let response = serde_json::json!({
                            "id": request_id,
                            "ok": true,
                            "body": response_body,
                        });
                        let encoded = encode_msgpack(&response)?;
                        timed_write(&mut writer, MSG_RESPONSE, &encoded, write_timeout).await?;
                    }
                    Ok(DispatchResponse::Stream(mut rx)) => {
                        // Stream chunks until the sender closes
                        while let Some(chunk) = rx.recv().await {
                            let frame = serde_json::json!({
                                "id": request_id,
                                "body": chunk,
                            });
                            let encoded = encode_msgpack(&frame)?;
                            timed_write(&mut writer, MSG_STREAM_CHUNK, &encoded, write_timeout).await?;
                        }
                        // End-of-stream sentinel
                        let end = serde_json::json!({ "id": request_id });
                        let encoded = encode_msgpack(&end)?;
                        timed_write(&mut writer, MSG_STREAM_END, &encoded, write_timeout).await?;
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
                        let encoded = encode_msgpack(&response)?;
                        timed_write(&mut writer, MSG_ERROR, &encoded, write_timeout).await?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_kernel_actor(
    mut kernel: Kernel,
    mut kernel_rx: mpsc::Receiver<KernelCommand>,
    cancel: CancellationToken,
    ipc_config: IpcConfig,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Kernel actor shutting down");
                break;
            }
            command = kernel_rx.recv() => {
                let Some(command) = command else {
                    tracing::error!("Kernel actor request channel closed unexpectedly");
                    break;
                };

                let result = router::route_request(
                    &mut kernel,
                    &command.service,
                    &command.method,
                    command.body,
                    &ipc_config,
                ).await;

                if command.response_tx.send(result).is_err() {
                    tracing::warn!("Dropped kernel actor response: requester closed channel");
                }
            }
        }
    }
}

/// Write a frame with a timeout. Returns an error if the write takes too long
/// (prevents slow consumers from holding connections indefinitely).
async fn timed_write<W: tokio::io::AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg_type: u8,
    payload: &[u8],
    timeout: Duration,
) -> std::io::Result<()> {
    tokio::time::timeout(timeout, write_frame(writer, msg_type, payload))
        .await
        .map_err(|_| {
            tracing::warn!(
                "Write timeout ({}s), dropping connection",
                timeout.as_secs()
            );
            std::io::Error::new(std::io::ErrorKind::TimedOut, "write timeout")
        })?
}
