//! PyToolExecutor — wraps a Python callable as a Rust ToolExecutor.
//!
//! Bridge: Python tool function → ToolExecutor trait → McpDelegatingAgent.
//! JSON serialization for all data crossing the boundary (safe, no complex conversion).

use async_trait::async_trait;
use pyo3::prelude::*;

use crate::worker::tools::{ConfirmationRequest, ToolExecutor, ToolInfo};

/// Wraps a Python callable (`Py<PyAny>`) as a Rust `ToolExecutor`.
///
/// `Py<PyAny>` is Send+Sync in PyO3 0.22. GIL is acquired per-call via
/// `Python::with_gil()`, which is safe from any thread (including tokio workers).
pub struct PyToolExecutor {
    py_fn: Py<PyAny>,
    info: ToolInfo,
    /// Optional Python callable for confirmation checks: (name, params) -> dict|None
    py_confirmation_fn: Option<Py<PyAny>>,
}

impl std::fmt::Debug for PyToolExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyToolExecutor")
            .field("tool_name", &self.info.name)
            .finish()
    }
}

impl PyToolExecutor {
    pub fn new(py_fn: Py<PyAny>, info: ToolInfo) -> Self {
        Self { py_fn, info, py_confirmation_fn: None }
    }

    pub fn with_confirmation_fn(mut self, py_fn: Py<PyAny>) -> Self {
        self.py_confirmation_fn = Some(py_fn);
        self
    }
}

#[async_trait]
impl ToolExecutor for PyToolExecutor {
    async fn execute(
        &self,
        _name: &str,
        params: serde_json::Value,
    ) -> crate::types::Result<serde_json::Value> {
        let result = Python::with_gil(|py| -> PyResult<serde_json::Value> {
            let json_mod = py.import_bound("json")?;

            // serde_json::Value → Python str → Python dict
            let params_str = serde_json::to_string(&params)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
            let py_params = json_mod.call_method1("loads", (params_str,))?;

            // Call the Python tool function
            let py_result = self.py_fn.call1(py, (py_params,))?;

            // Python result → Python str → serde_json::Value
            let result_str: String =
                json_mod.call_method1("dumps", (py_result,))?.extract()?;
            serde_json::from_str(&result_str)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })
        .map_err(|e| crate::types::Error::internal(format!("Python tool error: {e}")))?;

        Ok(result)
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![self.info.clone()]
    }

    fn requires_confirmation(&self, name: &str, params: &serde_json::Value) -> Option<ConfirmationRequest> {
        let py_fn = self.py_confirmation_fn.as_ref()?;
        Python::with_gil(|py| -> Option<ConfirmationRequest> {
            let json_mod = py.import_bound("json").ok()?;
            let params_str = serde_json::to_string(params).ok()?;
            let py_params = json_mod.call_method1("loads", (params_str,)).ok()?;
            let py_result = py_fn.call1(py, (name, py_params)).ok()?;
            if py_result.is_none(py) {
                return None;
            }
            let result_str: String = json_mod.call_method1("dumps", (py_result,)).ok()?.extract().ok()?;
            serde_json::from_str(&result_str).ok()
        })
    }
}
