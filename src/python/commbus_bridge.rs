//! PyEventSubscription — CommBus event subscription as a Python iterator.
//!
//! Same GIL-release pattern as `PyEventIterator`: blocks on channel recv
//! while the GIL is released so other Python threads can proceed.

use pyo3::prelude::*;
use tokio::sync::mpsc;

use crate::commbus::types::{Event, Subscription};

/// Python iterator over CommBus events for a subscription.
///
/// Each `__next__` call blocks (GIL-released) until the next event arrives
/// or the channel closes (subscription dropped → StopIteration).
///
/// **Important:** This is a lossy channel (256-item buffer, fire-and-forget).
/// If Python drains slower than publishers produce, events are silently dropped.
/// Consumers should wrap this with application-level overflow policy.
#[pyclass(name = "EventSubscription")]
pub struct PyEventSubscription {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) rx: mpsc::Receiver<Event>,
    pub(crate) subscription: Subscription,
}

impl std::fmt::Debug for PyEventSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyEventSubscription")
            .field("id", &self.subscription.id)
            .field("event_types", &self.subscription.event_types)
            .finish()
    }
}

#[pymethods]
impl PyEventSubscription {
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

    /// The subscription ID.
    #[getter]
    fn subscriber_id(&self) -> &str {
        &self.subscription.id
    }

    /// The event types this subscription listens to.
    #[getter]
    fn event_types(&self) -> Vec<String> {
        self.subscription.event_types.clone()
    }
}

/// Convert a CommBus Event to a Python dict.
fn event_to_pyobject(py: Python<'_>, event: &Event) -> PyResult<PyObject> {
    let dict = pyo3::types::PyDict::new_bound(py);
    dict.set_item("event_type", &event.event_type)?;
    dict.set_item("source", &event.source)?;
    dict.set_item("timestamp_ms", event.timestamp_ms)?;

    // Decode payload bytes → Python dict via JSON
    let payload_str = String::from_utf8(event.payload.clone())
        .unwrap_or_else(|_| "{}".to_string());
    let json_mod = py.import_bound("json")?;
    let py_payload = json_mod.call_method1("loads", (payload_str,))?;
    dict.set_item("payload", py_payload)?;

    Ok(dict.into())
}
