//! PyEventIterator — streaming pipeline events as a Python iterator.
//!
//! Wraps a `tokio::sync::mpsc::Receiver<PipelineEvent>` as a Python `__iter__`/`__next__`
//! protocol. Releases the GIL while blocking on channel recv so other Python threads
//! can proceed.

use pyo3::prelude::*;
use tokio::sync::mpsc;

use crate::worker::llm::PipelineEvent;
use crate::worker::WorkerResult;

/// Python iterator over pipeline streaming events.
///
/// Each `__next__` call blocks (GIL-released) until the next event arrives
/// or the channel closes (pipeline done → StopIteration).
#[pyclass(name = "EventIterator")]
pub struct PyEventIterator {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) rx: mpsc::Receiver<PipelineEvent>,
    pub(crate) _jh: tokio::task::JoinHandle<crate::types::Result<WorkerResult>>,
}

impl std::fmt::Debug for PyEventIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyEventIterator").finish()
    }
}

#[pymethods]
impl PyEventIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let handle = self.rt_handle.clone();
        let rx = &mut self.rx;

        // Release GIL while blocking on channel recv
        let event = py.allow_threads(|| handle.block_on(rx.recv()));

        match event {
            Some(evt) => Ok(Some(event_to_pyobject(py, &evt)?)),
            None => Ok(None), // Channel closed → StopIteration
        }
    }
}

/// Convert a PipelineEvent to a Python dict.
fn event_to_pyobject(py: Python<'_>, event: &PipelineEvent) -> PyResult<PyObject> {
    // Serialize to JSON string, then parse into Python dict via json.loads
    // This is simple and avoids complex manual dict construction.
    let json_str = serde_json::to_string(event)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let json_mod = py.import_bound("json")?;
    let py_dict = json_mod.call_method1("loads", (json_str,))?;
    Ok(py_dict.into())
}
