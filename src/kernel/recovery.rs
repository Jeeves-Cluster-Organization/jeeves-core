//! Panic recovery utilities for kernel operations.
//!
//! These utilities ensure that panics in kernel operations don't crash
//! the entire system but are instead gracefully handled and logged.
//!
//! Critical for production: a single agent panic shouldn't bring down
//! the entire agentic OS kernel.

use crate::types::{Error, Result};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Execute a function with panic recovery.
///
/// If the function panics, the panic is captured, logged, and converted
/// to an error. This prevents panics from crashing the kernel.
///
/// # Example
/// ```
/// use jeeves_core::kernel::with_recovery;
///
/// let result = with_recovery(|| {
///     // Potentially panicking code
///     Ok(())
/// }, "my_operation");
/// ```
pub fn with_recovery<F, T>(operation: F, operation_name: &str) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(panic_payload) => {
            let panic_msg = extract_panic_message(&panic_payload);
            tracing::error!(
                "panic_recovered: operation={}, panic={}",
                operation_name,
                panic_msg
            );

            Err(Error::internal(format!(
                "Panic in {}: {}",
                operation_name, panic_msg
            )))
        }
    }
}

/// Execute an async function with panic recovery.
///
/// Similar to `with_recovery` but for async operations.
pub async fn with_recovery_async<F, Fut, T>(operation: F, operation_name: &str) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let future = operation();

    match catch_unwind(AssertUnwindSafe(|| future)) {
        Ok(fut) => fut.await,
        Err(panic_payload) => {
            let panic_msg = extract_panic_message(&panic_payload);
            tracing::error!(
                "async_panic_recovered: operation={}, panic={}",
                operation_name,
                panic_msg
            );

            Err(Error::internal(format!(
                "Async panic in {}: {}",
                operation_name, panic_msg
            )))
        }
    }
}

/// Extract panic message from panic payload.
fn extract_panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic (no message)".to_string()
    }
}

/// Macro for wrapping operations with recovery and context.
///
/// # Example
/// ```ignore
/// recover_with_context!("create_process", {
///     kernel.create_process(...)
/// })
/// ```
#[macro_export]
macro_rules! recover_with_context {
    ($operation_name:expr, $body:expr) => {
        $crate::kernel::with_recovery(|| $body, $operation_name)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_recovery_success() {
        let result = with_recovery(|| Ok(42), "test_operation");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_with_recovery_error() {
        let result: Result<()> = with_recovery(
            || Err(Error::validation("test error".to_string())),
            "test_operation",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_with_recovery_panic_str() {
        let result: Result<()> = with_recovery(
            || {
                panic!("test panic");
            },
            "test_operation",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("Panic in test_operation"));
        assert!(err_msg.contains("test panic"));
    }

    #[test]
    fn test_with_recovery_panic_string() {
        let result: Result<()> = with_recovery(
            || {
                panic!("{}", "dynamic panic message");
            },
            "test_operation",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("Panic"));
        assert!(err_msg.contains("dynamic panic message"));
    }

    #[test]
    fn test_extract_panic_message_str() {
        let panic_result = std::panic::catch_unwind(|| {
            panic!("test message");
        });

        match panic_result {
            Err(payload) => {
                let msg = extract_panic_message(&payload);
                assert_eq!(msg, "test message");
            }
            Ok(_) => panic!("Expected panic"),
        }
    }

    #[test]
    fn test_extract_panic_message_string() {
        let panic_result = std::panic::catch_unwind(|| {
            panic!("{}", "formatted message");
        });

        match panic_result {
            Err(payload) => {
                let msg = extract_panic_message(&payload);
                assert!(msg.contains("formatted message"));
            }
            Ok(_) => panic!("Expected panic"),
        }
    }

    #[tokio::test]
    async fn test_with_recovery_async_success() {
        let result = with_recovery_async(|| async { Ok(42) }, "async_test").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_recovery_async_error() {
        let result: Result<()> = with_recovery_async(
            || async { Err(Error::validation("async error".to_string())) },
            "async_test",
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_preserves_error_details() {
        let original_error = Error::validation("specific validation error".to_string());
        let result: Result<()> = with_recovery(|| Err(original_error), "test_operation");

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("specific validation error"));
    }
}
