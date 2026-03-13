//! Consumer client for the Jeeves HTTP API.
//!
//! Provides typed, ergonomic access to the kernel's HTTP gateway.
//!
//! ```ignore
//! let client = JeevesClient::new("http://localhost:8080");
//! let response = client.run_pipeline(&config, "Hello", None).await?;
//! println!("{:?}", response.terminal_reason);
//! ```

use crate::envelope::InterruptResponse;
use crate::kernel::orchestrator_types::PipelineConfig;
use crate::types::{Error, Result};
use crate::worker::gateway_types::{AgentCard, ChatResponse, TaskStatus, ToolCardEntry};
use crate::worker::llm::PipelineEvent;

/// Typed HTTP client for the Jeeves kernel API.
#[derive(Debug, Clone)]
pub struct JeevesClient {
    base_url: String,
    http: reqwest::Client,
}

impl JeevesClient {
    /// Create a new client pointing at the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Run a pipeline to completion (buffered).
    pub async fn run_pipeline(
        &self,
        config: &PipelineConfig,
        input: &str,
        user_id: Option<&str>,
    ) -> Result<ChatResponse> {
        let url = format!("{}/api/v1/chat/messages", self.base_url);
        let body = serde_json::json!({
            "pipeline_config": config,
            "input": input,
            "user_id": user_id.unwrap_or("anonymous"),
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        resp.json::<ChatResponse>()
            .await
            .map_err(|e| Error::internal(format!("Response parse error: {e}")))
    }

    /// Get task status by process ID.
    pub async fn get_task_status(&self, process_id: &str) -> Result<TaskStatus> {
        let url = format!("{}/api/v1/tasks/{}", self.base_url, process_id);

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        resp.json::<TaskStatus>()
            .await
            .map_err(|e| Error::internal(format!("Response parse error: {e}")))
    }

    /// Run a pipeline with SSE streaming. Returns a receiver of pipeline events.
    ///
    /// Events are parsed from the server's SSE stream. The receiver closes
    /// when the stream ends (after a `Done` or `Error` event).
    pub async fn stream_pipeline(
        &self,
        config: &PipelineConfig,
        input: &str,
        user_id: Option<&str>,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<PipelineEvent>>> {
        let url = format!("{}/api/v1/chat/stream", self.base_url);
        let body = serde_json::json!({
            "pipeline_config": config,
            "input": input,
            "user_id": user_id.unwrap_or("anonymous"),
        });

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(64);

        // Spawn a task to parse the SSE byte stream into PipelineEvents
        tokio::spawn(async move {
            let mut buf = String::new();
            let mut event_type = String::new();
            let mut data = String::new();

            use futures::StreamExt;
            let mut byte_stream = resp.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(Err(Error::internal(format!("Stream read error: {e}")))).await;
                        break;
                    }
                };

                buf.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete SSE blocks (delimited by blank lines)
                while let Some(block_end) = buf.find("\n\n") {
                    let block = buf[..block_end].to_string();
                    buf = buf[block_end + 2..].to_string();

                    event_type.clear();
                    data.clear();

                    for line in block.lines() {
                        if let Some(val) = line.strip_prefix("event:") {
                            event_type = val.trim().to_string();
                        } else if let Some(val) = line.strip_prefix("data:") {
                            data = val.trim().to_string();
                        }
                    }

                    if data.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<PipelineEvent>(&data) {
                        Ok(event) => {
                            if tx.send(Ok(event)).await.is_err() {
                                return; // receiver dropped
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(Error::internal(
                                format!("SSE parse error (event={}): {e}", event_type),
                            ))).await;
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    /// List registered agents.
    pub async fn list_agents(&self) -> Result<Vec<AgentCard>> {
        let url = format!("{}/api/v1/agents", self.base_url);

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        resp.json::<Vec<AgentCard>>()
            .await
            .map_err(|e| Error::internal(format!("Response parse error: {e}")))
    }

    /// List registered tools.
    pub async fn list_tools(&self) -> Result<Vec<ToolCardEntry>> {
        let url = format!("{}/api/v1/tools", self.base_url);

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        resp.json::<Vec<ToolCardEntry>>()
            .await
            .map_err(|e| Error::internal(format!("Response parse error: {e}")))
    }

    /// Resolve an interrupt.
    pub async fn resolve_interrupt(
        &self,
        process_id: &str,
        interrupt_id: &str,
        response: &InterruptResponse,
    ) -> Result<()> {
        let url = format!(
            "{}/api/v1/interrupts/{}/{}/resolve",
            self.base_url, process_id, interrupt_id
        );

        let resp = self
            .http
            .post(&url)
            .json(response)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), &error_body));
        }

        Ok(())
    }

    /// Health check.
    pub async fn health(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::internal(format!("HTTP error: {e}")))?;

        Ok(resp.status().is_success())
    }
}

/// Map HTTP status code + body to typed Error.
fn map_http_error(status: u16, body: &str) -> Error {
    // Try to parse structured error response
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) {
        let code = parsed.get("code").and_then(|v| v.as_str()).unwrap_or("UNKNOWN");
        let message = parsed.get("error").and_then(|v| v.as_str()).unwrap_or(body);
        return match ErrorCode::from(code) {
            ErrorCode::RateLimited => Error::quota_exceeded(message),
            ErrorCode::InvalidArgument => Error::validation(message),
            ErrorCode::NotFound => Error::not_found(message),
            ErrorCode::FailedPrecondition => Error::state_transition(message),
            ErrorCode::Timeout => Error::timeout(message),
            ErrorCode::Cancelled => Error::cancelled(message),
            ErrorCode::Internal | ErrorCode::Unknown(_) => Error::internal(message),
        };
    }

    match status {
        400 => Error::validation(body),
        404 => Error::not_found(body),
        429 => Error::quota_exceeded(body),
        408 | 504 => Error::timeout(body),
        _ => Error::internal(format!("HTTP {status}: {body}")),
    }
}

/// Structured error codes from the gateway.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    RateLimited,
    InvalidArgument,
    NotFound,
    FailedPrecondition,
    Timeout,
    Cancelled,
    Internal,
    Unknown(String),
}

impl From<&str> for ErrorCode {
    fn from(s: &str) -> Self {
        match s {
            "RATE_LIMITED" => Self::RateLimited,
            "INVALID_ARGUMENT" => Self::InvalidArgument,
            "NOT_FOUND" => Self::NotFound,
            "FAILED_PRECONDITION" => Self::FailedPrecondition,
            "TIMEOUT" => Self::Timeout,
            "CANCELLED" => Self::Cancelled,
            "INTERNAL" | "PIPELINE_FAILED" | "INIT_FAILED" => Self::Internal,
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_parsing() {
        assert_eq!(ErrorCode::from("RATE_LIMITED"), ErrorCode::RateLimited);
        assert_eq!(ErrorCode::from("INVALID_ARGUMENT"), ErrorCode::InvalidArgument);
        assert_eq!(ErrorCode::from("NOT_FOUND"), ErrorCode::NotFound);
        assert_eq!(ErrorCode::from("FAILED_PRECONDITION"), ErrorCode::FailedPrecondition);
        assert_eq!(ErrorCode::from("TIMEOUT"), ErrorCode::Timeout);
        assert_eq!(ErrorCode::from("CANCELLED"), ErrorCode::Cancelled);
        assert_eq!(ErrorCode::from("INTERNAL"), ErrorCode::Internal);
        assert_eq!(ErrorCode::from("PIPELINE_FAILED"), ErrorCode::Internal);
        assert_eq!(ErrorCode::from("UNKNOWN_CODE"), ErrorCode::Unknown("UNKNOWN_CODE".to_string()));
    }

    #[test]
    fn test_map_http_error_structured() {
        let body = r#"{"error":"Too many requests","code":"RATE_LIMITED"}"#;
        let err = map_http_error(429, body);
        assert!(matches!(err, Error::QuotaExceeded(_)));
    }

    #[test]
    fn test_map_http_error_unstructured() {
        let err = map_http_error(404, "not found");
        assert!(matches!(err, Error::NotFound(_)));
    }

    #[test]
    fn test_client_creation() {
        let client = JeevesClient::new("http://localhost:8080/");
        assert_eq!(client.base_url, "http://localhost:8080");
    }
}
