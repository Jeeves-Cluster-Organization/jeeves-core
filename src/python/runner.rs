//! PyPipelineRunner — high-level Python facade for the Rust kernel.
//!
//! Owns a tokio Runtime + Kernel actor. Provides `run()` (buffered) and
//! `stream()` (iterator) methods. Tools are registered as Python callables
//! and bridged to the kernel's ToolRegistry via PyToolExecutor.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::PipelineConfig;
use crate::kernel::Kernel;
use crate::types::ProcessId;
use crate::worker::actor::spawn_kernel;
use crate::worker::agent::AgentRegistry;
use crate::worker::handle::KernelHandle;
use crate::worker::llm::openai::OpenAiProvider;
use crate::worker::llm::LlmProvider;
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::{ToolExecutor, ToolInfo, ToolRegistry};

use super::event_iter::PyEventIterator;
use super::tool_bridge::PyToolExecutor;

/// High-level facade for running pipelines from Python.
///
/// Owns a tokio runtime, kernel actor, LLM provider, and tool/agent registries.
/// Thread-safe: `run()` and `stream()` take `&self` and use Arc-wrapped registries.
#[pyclass(name = "PipelineRunner")]
pub struct PyPipelineRunner {
    rt: tokio::runtime::Runtime,
    handle: KernelHandle,
    cancel: CancellationToken,
    pipeline_configs: HashMap<String, PipelineConfig>,
    default_pipeline: String,
    prompts: Arc<PromptRegistry>,
    llm: Arc<dyn LlmProvider>,
    // Registered Python tool executors (accumulated via register_tool)
    tool_executors: Vec<(String, Arc<dyn ToolExecutor>)>,
    // Built registries (rebuilt on register_tool)
    tools: Arc<ToolRegistry>,
    agents: Arc<AgentRegistry>,
}

impl std::fmt::Debug for PyPipelineRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyPipelineRunner")
            .field("default_pipeline", &self.default_pipeline)
            .field("pipelines", &self.pipeline_configs.keys().collect::<Vec<_>>())
            .field("tools", &self.tool_executors.iter().map(|(n, _)| n).collect::<Vec<_>>())
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
    ///     openai_api_key: OpenAI API key (optional, falls back to OPENAI_API_KEY env).
    ///     openai_model: OpenAI model name (optional, defaults to gpt-4o-mini).
    ///     openai_base_url: OpenAI base URL override (optional).
    #[staticmethod]
    #[pyo3(signature = (pipeline_path, prompts_dir="prompts/", openai_api_key=None, openai_model=None, openai_base_url=None))]
    fn from_json(
        pipeline_path: &str,
        prompts_dir: &str,
        openai_api_key: Option<&str>,
        openai_model: Option<&str>,
        openai_base_url: Option<&str>,
    ) -> PyResult<Self> {
        // Read and parse pipeline config
        let config_str = std::fs::read_to_string(pipeline_path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to read pipeline config '{pipeline_path}': {e}"
            ))
        })?;
        let pipeline_config: PipelineConfig =
            serde_json::from_str(&config_str).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to parse pipeline config: {e}"
                ))
            })?;
        pipeline_config.validate().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid pipeline config: {e}"
            ))
        })?;

        let pipeline_name = pipeline_config.name.clone();

        // Build tokio runtime
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to create tokio runtime: {e}"
                ))
            })?;

        // Create and spawn kernel actor
        let kernel = Kernel::new();
        let cancel = CancellationToken::new();
        let handle = rt.block_on(async { spawn_kernel(kernel, cancel.clone()) });

        // Load prompts
        let prompts = Arc::new(PromptRegistry::from_dir(prompts_dir));

        // Build LLM provider
        let api_key = openai_api_key
            .map(String::from)
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();
        let model = openai_model
            .map(String::from)
            .or_else(|| std::env::var("OPENAI_MODEL").ok())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        let mut provider = OpenAiProvider::new(api_key, model);
        if let Some(base_url) = openai_base_url
            .map(String::from)
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
        {
            provider = provider.with_base_url(base_url);
        }
        let llm: Arc<dyn LlmProvider> = Arc::new(provider);

        // Initialize empty registries
        let tools = Arc::new(ToolRegistry::new());

        let mut pipeline_configs = HashMap::new();
        pipeline_configs.insert(pipeline_name.clone(), pipeline_config);

        // Build agents from pipeline config (no tools registered yet)
        let agents = crate::worker::agent_factory::AgentFactoryBuilder::new(
            llm.clone(),
            prompts.clone(),
            tools.clone(),
            handle.clone(),
        )
        .add_pipelines(pipeline_configs.values().cloned())
        .build();

        Ok(Self {
            rt,
            handle,
            cancel,
            pipeline_configs,
            default_pipeline: pipeline_name,
            prompts,
            llm,
            tool_executors: Vec::new(),
            tools,
            agents,
        })
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

        let executor: Arc<dyn ToolExecutor> = Arc::new(PyToolExecutor::new(py_fn.clone_ref(py), info));
        self.tool_executors.push((name, executor));

        // Rebuild tool + agent registries
        self.rebuild_registries();

        Ok(())
    }

    /// Register an additional pipeline config for cross-pipeline coordination.
    fn register_pipeline(&mut self, name: &str, config_json: &str) -> PyResult<()> {
        let config: PipelineConfig = serde_json::from_str(config_json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid pipeline JSON: {e}"))
        })?;
        config.validate().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid pipeline config: {e}"))
        })?;
        self.pipeline_configs.insert(name.to_string(), config);
        // Rebuild agents to include new pipeline's stages
        self.rebuild_registries();
        Ok(())
    }

    /// Run a pipeline to completion (buffered). Blocks until done.
    ///
    /// Returns a dict with keys: outputs, terminated, terminal_reason, process_id.
    ///
    /// Args:
    ///     input: Raw input string.
    ///     user_id: User identifier.
    ///     session_id: Optional session identifier.
    ///     pipeline_name: Optional pipeline name (defaults to the first registered).
    ///     metadata: Optional dict of metadata to pass through to agents/tools.
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
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let meta_json = py_obj_to_json_value(py, metadata)?;

        let handle = self.handle.clone();
        let agents = self.agents.clone();
        let input_owned = input.to_string();
        let user_id_owned = user_id.to_string();

        // Release GIL while running the pipeline
        let result = py.allow_threads(|| {
            self.run_inner(
                handle,
                process_id,
                config,
                &input_owned,
                &user_id_owned,
                &sid,
                agents,
                meta_json,
            )
        });

        let worker_result = result.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Pipeline error: {e}"))
        })?;

        // Convert WorkerResult to Python dict
        worker_result_to_pyobject(py, &worker_result)
    }

    /// Run a pipeline with streaming events. Returns an EventIterator.
    ///
    /// Each iteration yields a dict with a "type" key (delta, stage_started, etc.).
    ///
    /// Args:
    ///     input: Raw input string.
    ///     user_id: User identifier.
    ///     session_id: Optional session identifier.
    ///     pipeline_name: Optional pipeline name (defaults to the first registered).
    ///     metadata: Optional dict of metadata to pass through to agents/tools.
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
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let meta_json = py_obj_to_json_value(py, metadata)?;

        let handle = self.handle.clone();
        let agents = self.agents.clone();
        let envelope = Envelope::new_minimal(user_id, &sid, input, meta_json);

        let (jh, rx) = py.allow_threads(|| {
            self.rt.block_on(async {
                crate::worker::run_pipeline_streaming(
                    handle,
                    process_id,
                    config,
                    envelope,
                    agents,
                )
                .await
            })
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
        let pid = ProcessId::must(process_id);
        let handle = self.handle.clone();

        let state = py
            .allow_threads(|| self.rt.block_on(handle.get_session_state(&pid)))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to get pipeline state: {e}"
                ))
            })?;

        let json_str = serde_json::to_string(&state)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let json_mod = py.import_bound("json")?;
        let py_dict = json_mod.call_method1("loads", (json_str,))?;
        Ok(py_dict.into())
    }

    /// Capture a checkpoint of a running pipeline process.
    ///
    /// Returns the checkpoint as a JSON-serializable dict.
    ///
    /// Args:
    ///     process_id: The process ID to checkpoint.
    #[pyo3(signature = (process_id))]
    fn checkpoint(&self, py: Python<'_>, process_id: &str) -> PyResult<PyObject> {
        let pid = ProcessId::must(process_id);
        let handle = self.handle.clone();

        let snapshot = py
            .allow_threads(|| self.rt.block_on(handle.checkpoint(pid)))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to checkpoint: {e}"
                ))
            })?;

        let json_str = serde_json::to_string(&snapshot)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let json_mod = py.import_bound("json")?;
        let py_dict = json_mod.call_method1("loads", (json_str,))?;
        Ok(py_dict.into())
    }

    /// Resume a pipeline process from a checkpoint snapshot.
    ///
    /// Returns the restored process ID.
    ///
    /// Args:
    ///     snapshot_json: JSON string of the checkpoint snapshot.
    ///     pipeline_name: Optional pipeline name (defaults to the first registered).
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

        let config = self.get_pipeline_config(pipeline_name)?;
        let handle = self.handle.clone();

        let pid = py
            .allow_threads(|| self.rt.block_on(handle.resume_from_checkpoint(snapshot, config)))
            .map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to resume from checkpoint: {e}"
                ))
            })?;

        Ok(pid.to_string())
    }

    /// Describe a pipeline's structure (stages, routing, bounds, state_schema).
    ///
    /// Returns the full PipelineConfig as a Python dict.
    /// If pipeline_name is None, uses the default pipeline.
    #[pyo3(signature = (pipeline_name=None))]
    fn describe_pipeline(&self, py: Python<'_>, pipeline_name: Option<&str>) -> PyResult<PyObject> {
        let name = pipeline_name.unwrap_or(&self.default_pipeline);
        let config = self.pipeline_configs.get(name)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(
                format!("Pipeline '{}' not found. Available: {:?}", name, self.pipeline_configs.keys().collect::<Vec<_>>())
            ))?;
        let json_str = serde_json::to_string(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let json_mod = py.import_bound("json")?;
        Ok(json_mod.call_method1("loads", (json_str,))?.into())
    }

    /// List all registered tools with name, description, and parameter schema.
    ///
    /// Returns a list of dicts: [{"name": str, "description": str, "parameters": dict}, ...]
    fn list_tools(&self, py: Python<'_>) -> PyResult<PyObject> {
        let tools: Vec<serde_json::Value> = self.tools.list_all_tools()
            .into_iter()
            .map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }))
            .collect();
        let json_str = serde_json::to_string(&tools)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let json_mod = py.import_bound("json")?;
        Ok(json_mod.call_method1("loads", (json_str,))?.into())
    }

    /// Get JSON Schema for PipelineConfig (for editor validation).
    ///
    /// This is a static method — no runner instance needed.
    /// Returns a dict conforming to JSON Schema spec.
    #[staticmethod]
    fn get_schema(py: Python<'_>) -> PyResult<PyObject> {
        let schema = crate::kernel::orchestrator_types::pipeline_config_json_schema();
        let json_str = serde_json::to_string(&schema)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let json_mod = py.import_bound("json")?;
        Ok(json_mod.call_method1("loads", (json_str,))?.into())
    }

    /// Shut down the kernel actor and tokio runtime.
    fn shutdown(&self) {
        self.cancel.cancel();
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
        self.cancel.cancel();
    }
}

impl PyPipelineRunner {
    /// Inner run logic — handles both direct calls and nested calls from tools.
    #[allow(clippy::too_many_arguments)]
    fn run_inner(
        &self,
        handle: KernelHandle,
        process_id: ProcessId,
        pipeline_config: PipelineConfig,
        input: &str,
        user_id: &str,
        session_id: &str,
        agents: Arc<AgentRegistry>,
        metadata: Option<serde_json::Value>,
    ) -> crate::types::Result<crate::worker::WorkerResult> {
        let envelope = Envelope::new_minimal(user_id, session_id, input, metadata);
        if tokio::runtime::Handle::try_current().is_ok() {
            // Already inside tokio (called from a Python tool within a pipeline).
            // Use block_in_place to avoid nested block_on panic.
            tokio::task::block_in_place(|| {
                self.rt.handle().block_on(crate::worker::run_pipeline_with_envelope(
                    &handle,
                    process_id,
                    pipeline_config,
                    envelope,
                    &agents,
                ))
            })
        } else {
            // Normal call from Python main thread
            self.rt.block_on(crate::worker::run_pipeline_with_envelope(
                &handle,
                process_id,
                pipeline_config,
                envelope,
                &agents,
            ))
        }
    }

    /// Get pipeline config by name, defaulting to the first registered.
    fn get_pipeline_config(&self, name: Option<&str>) -> PyResult<PipelineConfig> {
        let key = name.unwrap_or(&self.default_pipeline);
        self.pipeline_configs.get(key).cloned().ok_or_else(|| {
            pyo3::exceptions::PyKeyError::new_err(format!(
                "Pipeline '{}' not found. Available: {:?}",
                key,
                self.pipeline_configs.keys().collect::<Vec<_>>()
            ))
        })
    }

    /// Rebuild tool and agent registries from current tool_executors + pipeline configs.
    ///
    /// Delegates to AgentFactoryBuilder which handles:
    /// - Decision tree (Gate→Deterministic, child_pipeline→PipelineAgent, etc.)
    /// - Per-stage ACL via AclToolExecutor::wrap_registry()
    /// - PipelineAgent OnceLock backfill
    fn rebuild_registries(&mut self) {
        // Build tool registry from accumulated executors
        let mut builder = crate::worker::tools::ToolRegistryBuilder::new();
        for (name, executor) in &self.tool_executors {
            builder = builder.add_tool(name.clone(), executor.clone());
        }
        let tools = builder.build();

        // Build agent registry via shared factory
        let agents = crate::worker::agent_factory::AgentFactoryBuilder::new(
            self.llm.clone(),
            self.prompts.clone(),
            tools.clone(),
            self.handle.clone(),
        )
        .add_pipelines(self.pipeline_configs.values().cloned())
        .build();

        self.tools = tools;
        self.agents = agents;
    }
}


/// Convert an optional Python object to a serde_json::Value.
/// Returns None if the input is None or Python None.
fn py_obj_to_json_value(
    py: Python<'_>,
    obj: Option<PyObject>,
) -> PyResult<Option<serde_json::Value>> {
    match obj {
        None => Ok(None),
        Some(o) if o.is_none(py) => Ok(None),
        Some(o) => {
            let json_mod = py.import_bound("json")?;
            let json_str: String = json_mod
                .call_method1("dumps", (o,))?
                .extract()?;
            let value: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to parse metadata as JSON: {e}"
                ))
            })?;
            Ok(Some(value))
        }
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
