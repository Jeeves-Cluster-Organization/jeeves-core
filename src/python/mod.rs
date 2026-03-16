//! Python bindings for jeeves-core via PyO3.
//!
//! Exposes `PipelineRunner`, `EventIterator`, and the `@tool` decorator
//! as a Python-importable module: `from jeeves_core import PipelineRunner, tool`

use pyo3::prelude::*;

pub mod event_iter;
pub mod runner;
pub mod tool_bridge;

pub use event_iter::PyEventIterator;
pub use runner::PyPipelineRunner;

// =============================================================================
// @tool decorator
// =============================================================================

/// Decorator returned by `tool()`. Sets `_tool_name`, `_tool_description`,
/// and `_tool_parameters` attributes on the wrapped function.
#[pyclass(name = "_ToolDecorator")]
#[derive(Debug)]
struct ToolDecorator {
    name: String,
    description: String,
    parameters: Option<PyObject>,
    requires_confirmation: Option<PyObject>,
}

#[allow(clippy::useless_conversion)]
#[pymethods]
impl ToolDecorator {
    fn __call__(&self, py: Python<'_>, func: PyObject) -> PyResult<PyObject> {
        func.setattr(py, "_tool_name", &self.name)?;
        func.setattr(py, "_tool_description", &self.description)?;
        if let Some(ref params) = self.parameters {
            func.setattr(py, "_tool_parameters", params)?;
        }
        if let Some(ref confirm_fn) = self.requires_confirmation {
            func.setattr(py, "_requires_confirmation", confirm_fn)?;
        }
        Ok(func)
    }
}

/// Decorator factory for registering Python functions as pipeline tools.
///
/// Usage:
///     @tool(name="get_time", description="Get current time")
///     def get_time(params):
///         return {"time": "..."}
///
///     @tool(name="rm", description="Delete files", requires_confirmation=my_check_fn)
///     def rm(params):
///         ...
#[pyfunction]
#[pyo3(signature = (name, description, parameters=None, requires_confirmation=None))]
fn tool(
    _py: Python<'_>,
    name: String,
    description: String,
    parameters: Option<PyObject>,
    requires_confirmation: Option<PyObject>,
) -> ToolDecorator {
    ToolDecorator {
        name,
        description,
        parameters,
        requires_confirmation,
    }
}

// =============================================================================
// Module init
// =============================================================================

/// PyO3 module entry point. Registered as `jeeves_core` Python module.
#[pymodule]
fn jeeves_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPipelineRunner>()?;
    m.add_class::<PyEventIterator>()?;
    m.add_function(wrap_pyfunction!(tool, m)?)?;
    Ok(())
}
