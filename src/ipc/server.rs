//! TCP IPC server — accept loop and per-connection handler.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWrite;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::ipc::codec::{
    read_frame, write_frame, MSG_ERROR, MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK, MSG_STREAM_END,
};
use crate::ipc::router::{self, DispatchResponse};
use crate::kernel::Kernel;
use crate::types::IpcConfig;

fn required_str_field<'a>(
    request: &'a serde_json::Value,
    key: &str,
) -> std::result::Result<&'a str, String> {
    request
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing required request field: {}", key))
}

struct ParsedRequest {
    request_id: String,
    service: String,
    method: String,
    body: serde_json::Value,
}

struct RequestValidationError {
    request_id: String,
    message: String,
}

fn parse_request(request: &serde_json::Value) -> std::result::Result<ParsedRequest, RequestValidationError> {
    let request_id_hint = request
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let request_id = required_str_field(request, "id")
        .map(str::to_string)
        .map_err(|message| RequestValidationError {
            request_id: request_id_hint.clone(),
            message,
        })?;

    let service = required_str_field(request, "service")
        .map(str::to_string)
        .map_err(|message| RequestValidationError {
            request_id: request_id.clone(),
            message,
        })?;

    let method = required_str_field(request, "method")
        .map(str::to_string)
        .map_err(|message| RequestValidationError {
            request_id: request_id.clone(),
            message,
        })?;

    let body = request
        .get("body")
        .cloned()
        .ok_or_else(|| RequestValidationError {
            request_id: request_id.clone(),
            message: "Missing required request field: body".to_string(),
        })?;

    Ok(ParsedRequest {
        request_id,
        service,
        method,
        body,
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

async fn send_error_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &serde_json::Value,
    timeout: Duration,
) -> std::io::Result<()> {
    let encoded = encode_msgpack(response)?;
    timed_write(writer, MSG_ERROR, &encoded, timeout).await
}

async fn send_error<W: AsyncWrite + Unpin>(
    writer: &mut W,
    request_id: &str,
    code: &str,
    message: impl Into<String>,
    timeout: Duration,
) -> std::io::Result<()> {
    let response = serde_json::json!({
        "id": request_id,
        "ok": false,
        "error": {
            "code": code,
            "message": message.into(),
        }
    });
    send_error_response(writer, &response, timeout).await
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
                    send_error(
                        &mut writer,
                        "",
                        "INVALID_ARGUMENT",
                        format!("Unexpected message type: 0x{:02X}", msg_type),
                        write_timeout,
                    )
                    .await?;
                    continue;
                }

                // Decode msgpack request
                let request: serde_json::Value = match rmp_serde::from_slice(&payload_bytes) {
                    Ok(v) => v,
                    Err(e) => {
                        send_error(
                            &mut writer,
                            "",
                            "INVALID_ARGUMENT",
                            format!("Invalid msgpack: {}", e),
                            write_timeout,
                        )
                        .await?;
                        continue;
                    }
                };

                let ParsedRequest {
                    request_id,
                    service,
                    method,
                    body,
                } = match parse_request(&request) {
                    Ok(parsed) => parsed,
                    Err(validation_error) => {
                        send_error(
                            &mut writer,
                            &validation_error.request_id,
                            "INVALID_ARGUMENT",
                            validation_error.message,
                            write_timeout,
                        )
                        .await?;
                        continue;
                    }
                };

                let (response_tx, response_rx) = oneshot::channel();
                let command = KernelCommand {
                    service,
                    method,
                    body,
                    response_tx,
                };

                match kernel_tx.try_send(command) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        let response = serde_json::json!({
                            "id": request_id,
                            "ok": false,
                            "error": {
                                "code": "RESOURCE_EXHAUSTED",
                                "message": "Kernel request queue is full; retry shortly",
                                "retryable": true,
                                "kernel_queue_capacity": ipc_config.kernel_queue_capacity,
                            }
                        });
                        send_error_response(&mut writer, &response, write_timeout).await?;
                        continue;
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        send_error(
                            &mut writer,
                            &request_id,
                            "UNAVAILABLE",
                            "Kernel actor is unavailable",
                            write_timeout,
                        )
                        .await?;
                        continue;
                    }
                }

                let result = match response_rx.await {
                    Ok(result) => result,
                    Err(_) => {
                        send_error(
                            &mut writer,
                            &request_id,
                            "UNAVAILABLE",
                            "Kernel actor terminated while request was in flight",
                            write_timeout,
                        )
                        .await?;
                        continue;
                    }
                };

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
                        send_error(
                            &mut writer,
                            &request_id,
                            e.to_ipc_error_code(),
                            e.to_string(),
                            write_timeout,
                        )
                        .await?;
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
async fn timed_write<W: AsyncWrite + Unpin>(
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
