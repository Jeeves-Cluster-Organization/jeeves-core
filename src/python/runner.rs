//! PyO3 wrapper for PipelineRunner — thin delegation + GIL management.
//!
//! Owns a tokio Runtime (needed for Python's sync context) and delegates all
//! pipeline logic to `crate::worker::runner::PipelineRunner`.

use pyo3::prelude::*;
use std::sync::Arc;

use crate::commbus::types::{Event, Query};
use crate::kernel::routing::{RoutingContext, RoutingFn, RoutingResult};
use crate::worker::runner::PipelineRunner;
use crate::worker::tools::{ContentResolver, ToolExecutor, ToolInfo};

use super::commbus_bridge::PyEventSubscription;
use super::event_iter::PyEventIterator;
use super::tool_bridge::PyToolExecutor;

/// Bridge: wraps a Python callable as a RoutingFn.
///
/// The Python callable receives a dict and returns str, list[str], or None.
struct PyRoutingFn {
    py_fn: PyObject,
}

// Safety: PyObject is Send+Sync when only accessed inside GIL blocks.
unsafe impl Send for PyRoutingFn {}
unsafe impl Sync for PyRoutingFn {}

impl RoutingFn for PyRoutingFn {
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingResult {
        Python::with_gil(|py| {
            let ctx_dict = pyo3::types::PyDict::new_bound(py);
            let _ = ctx_dict.set_item("current_stage", ctx.current_stage);
            let _ = ctx_dict.set_item("agent_name", ctx.agent_name);
            let _ = ctx_dict.set_item("agent_failed", ctx.agent_failed);

            // Convert outputs/metadata/state to Python via JSON round-trip
            let Ok(json_mod) = py.import_bound("json") else {
                tracing::error!("failed to import Python json module in routing_fn");
                return RoutingResult::Terminate;
            };
            for (key, val) in [
                ("outputs", &serde_json::to_string(ctx.outputs).unwrap_or_default()),
                ("metadata", &serde_json::to_string(ctx.metadata).unwrap_or_default()),
                ("state", &serde_json::to_string(ctx.state).unwrap_or_default()),
            ] {
                if let Ok(parsed) = json_mod.call_method1("loads", (val.as_str(),)) {
                    let _ = ctx_dict.set_item(key, parsed);
                }
            }
            if let Some(ir) = ctx.interrupt_response {
                if let Ok(s) = serde_json::to_string(ir) {
                    if let Ok(parsed) = json_mod.call_method1("loads", (s.as_str(),)) {
                        let _ = ctx_dict.set_item("interrupt_response", parsed);
                    }
                }
            }

            match self.py_fn.call1(py, (ctx_dict,)) {
                Ok(result) => {
                    if result.is_none(py) {
                        return RoutingResult::Terminate;
                    }
                    // Try as list first (Fan)
                    if let Ok(list) = result.extract::<Vec<String>>(py) {
                        return RoutingResult::Fan(list);
                    }
                    // Try as string (Next)
                    if let Ok(s) = result.extract::<String>(py) {
                        return RoutingResult::Next(s);
                    }
                    tracing::warn!("routing_fn returned non-str/list/None, terminating");
                    RoutingResult::Terminate
                }
                Err(e) => {
                    tracing::error!("routing_fn raised exception: {e}");
                    RoutingResult::Terminate
                }
            }
        })
    }
}

/// Python-facing PipelineRunner. Thin wrapper over the Rust PipelineRunner
/// that adds a tokio runtime for blocking from Python and GIL management.
#[pyclass(name = "PipelineRunner")]
pub struct PyPipelineRunner {
    pub(crate) rt: tokio::runtime::Runtime,
    pub(crate) inner: PipelineRunner,
}

impl PyPipelineRunner {
    /// Access the underlying Rust PipelineRunner (for Rust crate consumers like airframe).
    pub fn runner(&self) -> &PipelineRunner {
        &self.inner
    }

    /// Access the tokio runtime handle (for Rust crate consumers that need to block_on).
    pub fn runtime(&self) -> &tokio::runtime::Runtime {
        &self.rt
    }
}

impl std::fmt::Debug for PyPipelineRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyPipelineRunner")
            .field("inner", &self.inner)
            .finish()
    }
}

#[allow(clippy::useless_conversion)]
#[pymethods]
impl PyPipelineRunner {
    /// Create a PipelineRunner from a JSON pipeline config file.
    ///
    /// Args:
    ///     pipeline_path: Path to the pipeline JSON config file.
    ///     prompts_dir: Directory containing prompt template .txt files.
    ///     model: LLM model name (optional, defaults to gpt-4o-mini).
    ///            Model prefix determines provider: gpt-* → OpenAI, claude-* → Anthropic, etc.
    ///            API keys are read from env vars: OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.
    #[staticmethod]
    #[pyo3(signature = (pipeline_path, prompts_dir="prompts/", model=None))]
    fn from_json(
        pipeline_path: &str,
        prompts_dir: &str,
        model: Option<&str>,
    ) -> PyResult<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to create tokio runtime: {e}"
                ))
            })?;

        let inner = rt
            .block_on(PipelineRunner::from_json(pipeline_path, prompts_dir, model))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to create PipelineRunner: {e}"
                ))
            })?;

        Ok(Self { rt, inner })
    }

    /// Register a Python callable as a tool.
    ///
    /// The callable must have `_tool_name` and `_tool_description` attributes
    /// (set by the `@tool` decorator). Rebuilds agent registry after registration.
    fn register_tool(&mut self, py: Python<'_>, py_fn: PyObject) -> PyResult<()> {
        let name: String = py_fn
            .getattr(py, "_tool_name")
            .and_then(|v| v.extract(py))
            .map_err(|_| {
                pyo3::exceptions::PyAttributeError::new_err(
                    "Tool function must have _tool_name attribute. Use @tool decorator.",
                )
            })?;
        let description: String = py_fn
            .getattr(py, "_tool_description")
            .and_then(|v| v.extract(py))
            .map_err(|_| {
                pyo3::exceptions::PyAttributeError::new_err(
                    "Tool function must have _tool_description attribute. Use @tool decorator.",
                )
            })?;
        let parameters: serde_json::Value = py_fn
            .getattr(py, "_tool_parameters")
            .ok()
            .and_then(|v| {
                if v.is_none(py) {
                    return None;
                }
                let json_mod = py.import_bound("json").ok()?;
                let json_str: String = json_mod
                    .call_method1("dumps", (v,))
                    .ok()?
                    .extract()
                    .ok()?;
                serde_json::from_str(&json_str).ok()
            })
            .unwrap_or(serde_json::json!({
                "type": "object",
                "properties": {}
            }));

        let info = ToolInfo {
            name: name.clone(),
            description,
            parameters,
        };

        let mut py_executor = PyToolExecutor::new(py_fn.clone_ref(py), info);

        if let Ok(confirm_fn) = py_fn.getattr(py, "_requires_confirmation") {
            if !confirm_fn.is_none(py) {
                py_executor = py_executor.with_confirmation_fn(confirm_fn);
            }
        }

        let executor: Arc<dyn ToolExecutor> = Arc::new(py_executor);
        self.inner.register_tool_executor(name, executor);

        Ok(())
    }

    /// Register an additional pipeline config for cross-pipeline coordination.
    fn register_pipeline(&mut self, name: &str, config_json: &str) -> PyResult<()> {
        let config: crate::kernel::orchestrator_types::PipelineConfig =
            serde_json::from_str(config_json).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid pipeline JSON: {e}"))
            })?;
        self.inner.register_pipeline(name.to_string(), config).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid pipeline config: {e}"))
        })?;
        Ok(())
    }

    /// Register a Python callable as a routing function.
    ///
    /// The callable receives a dict with keys: current_stage, agent_name, agent_failed,
    /// outputs, metadata, interrupt_response, state.
    /// It must return: a stage name (str), a list of stage names (list[str] for Fork fan-out),
    /// or None (terminate pipeline).
    fn register_routing_fn(&mut self, py: Python<'_>, name: String, py_fn: PyObject) -> PyResult<()> {
        let routing_fn = Arc::new(PyRoutingFn { py_fn: py_fn.clone_ref(py) });
        self.rt.block_on(async {
            self.inner.register_routing_fn(name, routing_fn).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to register routing fn: {e}"))
        })?;
        Ok(())
    }

    /// Run a pipeline to completion (buffered). Blocks until done.
    #[pyo3(signature = (input, user_id="user", session_id=None, pipeline_name=None, metadata=None))]
    fn run(
        &self,
        py: Python<'_>,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
        metadata: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let meta_json = py_obj_to_json_value(py, metadata)?;
        let input = input.to_string();
        let user_id = user_id.to_string();
        let session_id = session_id.map(String::from);
        let pipeline_name = pipeline_name.map(String::from);

        let result = py.allow_threads(|| {
            let rt_handle = self.rt.handle();
            // Handle nested calls (from Python tools inside a pipeline)
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::block_in_place(|| {
                    rt_handle.block_on(self.inner.run(
                        &input,
                        &user_id,
                        session_id.as_deref(),
                        pipeline_name.as_deref(),
                        meta_json,
                    ))
                })
            } else {
                rt_handle.block_on(self.inner.run(
                    &input,
                    &user_id,
                    session_id.as_deref(),
                    pipeline_name.as_deref(),
                    meta_json,
                ))
            }
        })
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Pipeline error: {e}"))
        })?;

        worker_result_to_pyobject(py, &result)
    }

    /// Run a pipeline with streaming events. Returns an EventIterator.
    #[pyo3(signature = (input, user_id="user", session_id=None, pipeline_name=None, metadata=None))]
    fn stream(
        &self,
        py: Python<'_>,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
        metadata: Option<PyObject>,
    ) -> PyResult<PyEventIterator> {
        let meta_json = py_obj_to_json_value(py, metadata)?;

        let (jh, rx) = py
            .allow_threads(|| {
                self.rt.block_on(self.inner.stream(
                    input,
                    user_id,
                    session_id,
                    pipeline_name,
                    meta_json,
                ))
            })
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Pipeline stream error: {e}"))
            })?;

        Ok(PyEventIterator {
            rt_handle: self.rt.handle().clone(),
            rx,
            _jh: jh,
        })
    }

    /// Get the state of a running/completed pipeline session.
    #[pyo3(signature = (process_id))]
    fn get_pipeline_state(&self, py: Python<'_>, process_id: &str) -> PyResult<PyObject> {
        let pid = crate::types::ProcessId::must(process_id);

        let state = py
            .allow_threads(|| self.rt.block_on(self.inner.get_session_state(&pid)))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to get pipeline state: {e}"
                ))
            })?;

        json_to_pyobject(py, &state)
    }

    /// Capture a checkpoint of a running pipeline process.
    #[pyo3(signature = (process_id))]
    fn checkpoint(&self, py: Python<'_>, process_id: &str) -> PyResult<PyObject> {
        let pid = crate::types::ProcessId::must(process_id);

        let snapshot = py
            .allow_threads(|| self.rt.block_on(self.inner.checkpoint(&pid)))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to checkpoint: {e}"))
            })?;

        json_to_pyobject(py, &snapshot)
    }

    /// Resume a pipeline process from a checkpoint snapshot.
    #[pyo3(signature = (snapshot_json, pipeline_name=None))]
    fn resume_from_checkpoint(
        &self,
        py: Python<'_>,
        snapshot_json: &str,
        pipeline_name: Option<&str>,
    ) -> PyResult<String> {
        let snapshot: crate::kernel::checkpoint::CheckpointSnapshot =
            serde_json::from_str(snapshot_json).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to parse checkpoint snapshot: {e}"
                ))
            })?;

        let pid = py
            .allow_threads(|| {
                self.rt
                    .block_on(self.inner.resume_from_checkpoint(snapshot, pipeline_name))
            })
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to resume from checkpoint: {e}"
                ))
            })?;

        Ok(pid.to_string())
    }

    /// Describe a pipeline's structure.
    #[pyo3(signature = (pipeline_name=None))]
    fn describe_pipeline(
        &self,
        py: Python<'_>,
        pipeline_name: Option<&str>,
    ) -> PyResult<PyObject> {
        let config = self.inner.describe_pipeline(pipeline_name).ok_or_else(|| {
            pyo3::exceptions::PyKeyError::new_err(format!(
                "Pipeline '{}' not found",
                pipeline_name.unwrap_or(self.inner.default_pipeline())
            ))
        })?;
        json_to_pyobject(py, config)
    }

    /// List all registered tools.
    fn list_tools(&self, py: Python<'_>) -> PyResult<PyObject> {
        let tools: Vec<serde_json::Value> = self
            .inner
            .list_tools()
            .into_iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                })
            })
            .collect();
        json_to_pyobject(py, &tools)
    }

    /// Get JSON Schema for PipelineConfig.
    #[staticmethod]
    fn get_schema(py: Python<'_>) -> PyResult<PyObject> {
        let schema = crate::kernel::orchestrator_types::pipeline_config_json_schema();
        json_to_pyobject(py, &schema)
    }

    // =========================================================================
    // CommBus Federation (P0a)
    // =========================================================================

    /// Publish an event to CommBus subscribers.
    ///
    /// Args:
    ///     event_type: Event type string (e.g. "perception.detections").
    ///     payload: JSON-serializable dict payload.
    ///     source: Identity of the publisher (e.g. "perception_cv", "session:abc").
    ///
    /// Returns: Number of subscribers the event was delivered to.
    fn publish_event(&self, py: Python<'_>, event_type: String, payload: PyObject, source: String) -> PyResult<usize> {
        let payload_bytes = py_obj_to_json_bytes(py, payload)?;
        let event = Event {
            event_type,
            payload: payload_bytes,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            source,
        };

        py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().publish_event(event))
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("publish_event failed: {e}")))
    }

    /// Subscribe to CommBus events. Returns an EventSubscription iterator.
    ///
    /// Args:
    ///     subscriber_id: Unique ID for this subscription.
    ///     event_types: List of event type strings to subscribe to.
    ///
    /// The returned iterator blocks (GIL-released) on each `next()` call.
    /// Channel is 256-item, lossy (fire-and-forget) — wrap with overflow policy in Python.
    fn subscribe(&self, py: Python<'_>, subscriber_id: String, event_types: Vec<String>) -> PyResult<PyEventSubscription> {
        let (subscription, rx) = py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().subscribe(subscriber_id, event_types))
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("subscribe failed: {e}")))?;

        Ok(PyEventSubscription {
            rt_handle: self.rt.handle().clone(),
            rx,
            subscription,
        })
    }

    /// Unsubscribe from CommBus events.
    fn unsubscribe(&self, py: Python<'_>, subscription: &PyEventSubscription) -> PyResult<()> {
        let sub = subscription.subscription.clone();
        py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().unsubscribe(sub));
        });
        Ok(())
    }

    /// Execute a CommBus query (request-response with timeout).
    ///
    /// Args:
    ///     query_type: Query type string.
    ///     payload: JSON-serializable dict payload.
    ///     timeout_ms: Timeout in milliseconds.
    ///     source: Identity of the querier (e.g. "mission_planner", "session:abc").
    ///
    /// Returns: Response dict with 'result' (dict) and optional 'error' (str).
    /// Note: Blocks the Python thread for the full timeout duration if no handler responds.
    fn commbus_query(&self, py: Python<'_>, query_type: String, payload: PyObject, timeout_ms: u64, source: String) -> PyResult<PyObject> {
        let payload_bytes = py_obj_to_json_bytes(py, payload)?;
        let query = Query {
            query_type,
            payload: payload_bytes,
            timeout_ms,
            source,
        };

        let response = py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().commbus_query(query))
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("commbus_query failed: {e}")))?;

        // Build response dict
        let dict = pyo3::types::PyDict::new_bound(py);
        let json_mod = py.import_bound("json")?;
        let result_str = String::from_utf8(response.result).unwrap_or_else(|_| "{}".to_string());
        let py_result = json_mod.call_method1("loads", (result_str,))?;
        dict.set_item("result", py_result)?;
        if let Some(ref err) = response.error {
            dict.set_item("error", err)?;
        }
        Ok(dict.into())
    }

    // =========================================================================
    // Agent Discovery + System Status (P0b)
    // =========================================================================

    /// List registered agent cards for discovery.
    #[pyo3(signature = (filter=None))]
    fn list_agent_cards(&self, py: Python<'_>, filter: Option<String>) -> PyResult<PyObject> {
        let cards = py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().list_agent_cards(filter))
        });
        json_to_pyobject(py, &cards)
    }

    /// Get system status (active processes, subscriber counts, etc.).
    fn get_system_status(&self, py: Python<'_>) -> PyResult<PyObject> {
        let status = py.allow_threads(|| {
            self.rt.block_on(self.inner.handle().get_system_status())
        });
        json_to_pyobject(py, &status)
    }

    // =========================================================================
    // Content Resolver (P1c)
    // =========================================================================

    /// Register a Python callable as a content resolver for lazy Ref resolution.
    ///
    /// The callable receives (ref_id: str, content_type: str) and returns bytes or None.
    /// Called at LLM-send time when a tool returns ContentPart::Ref.
    fn register_content_resolver(&mut self, py: Python<'_>, py_fn: PyObject) -> PyResult<()> {
        let resolver = Arc::new(PyContentResolver {
            py_fn: py_fn.clone_ref(py),
        });
        self.inner.set_content_resolver(resolver);
        Ok(())
    }

    /// Shut down the kernel actor.
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _exc_tb: Option<PyObject>,
    ) {
        self.inner.shutdown();
    }
}

// -- helpers --

/// Convert an optional Python object to serde_json::Value.
fn py_obj_to_json_value(
    py: Python<'_>,
    obj: Option<PyObject>,
) -> PyResult<Option<serde_json::Value>> {
    match obj {
        None => Ok(None),
        Some(o) if o.is_none(py) => Ok(None),
        Some(o) => {
            let json_mod = py.import_bound("json")?;
            let json_str: String = json_mod.call_method1("dumps", (o,))?.extract()?;
            let value: serde_json::Value =
                serde_json::from_str(&json_str).map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Failed to parse metadata as JSON: {e}"
                    ))
                })?;
            Ok(Some(value))
        }
    }
}

/// Serialize any serde value to a Python object via JSON.
fn json_to_pyobject(py: Python<'_>, value: &impl serde::Serialize) -> PyResult<PyObject> {
    let json_str = serde_json::to_string(value)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let json_mod = py.import_bound("json")?;
    let py_obj = json_mod.call_method1("loads", (json_str,))?;
    Ok(py_obj.into())
}

/// Convert a Python object to JSON-encoded bytes.
fn py_obj_to_json_bytes(py: Python<'_>, obj: PyObject) -> PyResult<Vec<u8>> {
    let json_mod = py.import_bound("json")?;
    let json_str: String = json_mod.call_method1("dumps", (obj,))?.extract()?;
    Ok(json_str.into_bytes())
}

// =============================================================================
// PyContentResolver — bridges Python callable to ContentResolver trait
// =============================================================================

/// Wraps a Python callable `(ref_id, content_type) -> bytes | None` as ContentResolver.
struct PyContentResolver {
    py_fn: PyObject,
}

// Safety: PyObject is Send+Sync when only accessed inside GIL blocks.
unsafe impl Send for PyContentResolver {}
unsafe impl Sync for PyContentResolver {}

impl std::fmt::Debug for PyContentResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyContentResolver").finish()
    }
}

impl ContentResolver for PyContentResolver {
    fn resolve(&self, ref_id: &str, content_type: &str) -> Option<Vec<u8>> {
        Python::with_gil(|py| {
            let result = self.py_fn.call1(py, (ref_id, content_type)).ok()?;
            if result.is_none(py) {
                return None;
            }
            result.extract::<Vec<u8>>(py).ok()
        })
    }
}

/// Convert a WorkerResult to a Python dict.
fn worker_result_to_pyobject(
    py: Python<'_>,
    result: &crate::worker::WorkerResult,
) -> PyResult<PyObject> {
    let outputs_json = serde_json::to_string(&result.outputs)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

    let json_mod = py.import_bound("json")?;
    let py_outputs = json_mod.call_method1("loads", (outputs_json,))?;

    let dict = pyo3::types::PyDict::new_bound(py);
    dict.set_item("process_id", result.process_id.as_str())?;
    dict.set_item("terminated", result.terminated())?;
    dict.set_item(
        "terminal_reason",
        result
            .terminal_reason()
            .as_ref()
            .map(|r| format!("{r:?}")),
    )?;
    dict.set_item("outputs", py_outputs)?;

    Ok(dict.into())
}
