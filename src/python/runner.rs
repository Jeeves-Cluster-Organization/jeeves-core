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
use crate::kernel::orchestrator_types::{NodeKind, PipelineConfig};
use crate::kernel::Kernel;
use crate::types::ProcessId;
use crate::worker::actor::spawn_kernel;
use crate::worker::agent::{AgentRegistry, DeterministicAgent, LlmAgent, McpDelegatingAgent};
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

        // Build agents from pipeline config (no tools registered yet)
        let agents = Arc::new(build_agents_from_config(
            &pipeline_config,
            &tools,
            &llm,
            &prompts,
        ));

        let mut pipeline_configs = HashMap::new();
        pipeline_configs.insert(pipeline_name.clone(), pipeline_config);

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
    #[pyo3(signature = (input, user_id="user", session_id=None, pipeline_name=None))]
    fn run(
        &self,
        py: Python<'_>,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
    ) -> PyResult<PyObject> {
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

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
    #[pyo3(signature = (input, user_id="user", session_id=None, pipeline_name=None))]
    fn stream(
        &self,
        py: Python<'_>,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
    ) -> PyResult<PyEventIterator> {
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let handle = self.handle.clone();
        let agents = self.agents.clone();
        let envelope = Envelope::new_minimal(user_id, &sid, input, None);

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
    ) -> crate::types::Result<crate::worker::WorkerResult> {
        if tokio::runtime::Handle::try_current().is_ok() {
            // Already inside tokio (called from a Python tool within a pipeline).
            // Use block_in_place to avoid nested block_on panic.
            tokio::task::block_in_place(|| {
                self.rt.handle().block_on(crate::worker::run_pipeline(
                    &handle,
                    process_id,
                    pipeline_config,
                    input,
                    user_id,
                    session_id,
                    &agents,
                ))
            })
        } else {
            // Normal call from Python main thread
            self.rt.block_on(crate::worker::run_pipeline(
                &handle,
                process_id,
                pipeline_config,
                input,
                user_id,
                session_id,
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
    fn rebuild_registries(&mut self) {
        let mut tool_registry = ToolRegistry::new();
        for (name, executor) in &self.tool_executors {
            tool_registry.register(name.clone(), executor.clone());
        }
        let tools = Arc::new(tool_registry);

        // Build agents from ALL registered pipeline configs
        let mut agent_registry = AgentRegistry::new();
        for config in self.pipeline_configs.values() {
            merge_agents_from_config(
                &mut agent_registry,
                config,
                &tools,
                &self.llm,
                &self.prompts,
            );
        }

        self.tools = tools;
        self.agents = Arc::new(agent_registry);
    }
}

/// Build an AgentRegistry from a single pipeline config.
fn build_agents_from_config(
    config: &PipelineConfig,
    tools: &Arc<ToolRegistry>,
    llm: &Arc<dyn LlmProvider>,
    prompts: &Arc<PromptRegistry>,
) -> AgentRegistry {
    let mut registry = AgentRegistry::new();
    merge_agents_from_config(&mut registry, config, tools, llm, prompts);
    registry
}

/// Merge agents from a pipeline config into an existing registry.
///
/// Auto-creates agents based on pipeline stage configuration:
/// - Gate nodes → DeterministicAgent
/// - has_llm=true → LlmAgent (prompt_key defaults to agent name)
/// - has_llm=false + matching tool → McpDelegatingAgent
/// - has_llm=false + no tool → DeterministicAgent
fn merge_agents_from_config(
    registry: &mut AgentRegistry,
    config: &PipelineConfig,
    tools: &Arc<ToolRegistry>,
    llm: &Arc<dyn LlmProvider>,
    prompts: &Arc<PromptRegistry>,
) {
    use crate::worker::agent::Agent;

    for stage in &config.stages {
        let agent_name = &stage.agent;
        if agent_name.is_empty() {
            continue;
        }
        // Skip if already registered (first registration wins)
        if registry.get(agent_name).is_some() {
            continue;
        }

        let agent: Arc<dyn Agent> = match stage.node_kind {
            NodeKind::Gate => Arc::new(DeterministicAgent),
            _ if stage.has_llm => {
                let prompt_key = stage
                    .prompt_key
                    .clone()
                    .unwrap_or_else(|| agent_name.clone());
                Arc::new(LlmAgent {
                    llm: llm.clone(),
                    prompts: prompts.clone(),
                    tools: tools.clone(),
                    prompt_key,
                    temperature: stage.temperature,
                    max_tokens: stage.max_tokens,
                    model: stage.model_role.clone(),
                    max_tool_rounds: 10,
                })
            }
            _ => {
                // has_llm=false: check if a matching tool exists
                if tools.get(agent_name).is_some() {
                    Arc::new(McpDelegatingAgent {
                        tool_name: agent_name.clone(),
                        tools: tools.clone(),
                    })
                } else {
                    Arc::new(DeterministicAgent)
                }
            }
        };

        registry.register(agent_name.clone(), agent);
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
    dict.set_item("terminated", result.terminated)?;
    dict.set_item(
        "terminal_reason",
        result
            .terminal_reason
            .as_ref()
            .map(|r| format!("{r:?}")),
    )?;
    dict.set_item("outputs", py_outputs)?;

    Ok(dict.into())
}
