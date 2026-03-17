//! PipelineRunner — high-level facade for running pipelines from Rust.
//!
//! Owns a Kernel actor, LLM provider, and tool/agent registries. Provides
//! `run()` (buffered) and `stream()` (typed event channel) methods.
//!
//! This is the Rust consumer DX primitive. Python's `PipelineRunner` (PyO3)
//! is a thin wrapper that adds a tokio runtime + GIL management.

use std::collections::HashMap;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::PipelineConfig;
use crate::kernel::Kernel;
use crate::types::{ProcessId, Result};
use crate::worker::actor::spawn_kernel;
use crate::worker::agent::AgentRegistry;
use crate::worker::handle::KernelHandle;
use crate::worker::llm::genai_provider::GenaiProvider;
use crate::worker::llm::{LlmProvider, PipelineEvent};
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::{ToolExecutor, ToolInfo, ToolRegistry, ToolRegistryBuilder};
use crate::worker::WorkerResult;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// High-level facade for running pipelines. Owns kernel actor + all registries.
///
/// ```rust,no_run
/// use jeeves_core::prelude::*;
///
/// # async fn example() -> jeeves_core::Result<()> {
/// let mut runner = PipelineRunner::from_json("pipeline.json", "prompts/", None).await?;
/// // runner.register_tool(my_tools);
/// let result = runner.run("hello", "user1", None, None, None).await?;
/// # Ok(())
/// # }
/// ```
pub struct PipelineRunner {
    handle: KernelHandle,
    cancel: CancellationToken,
    pipeline_configs: HashMap<String, PipelineConfig>,
    default_pipeline: String,
    prompts: Arc<PromptRegistry>,
    llm: Arc<dyn LlmProvider>,
    tool_executors: Vec<(String, Arc<dyn ToolExecutor>)>,
    tools: Arc<ToolRegistry>,
    agents: Arc<AgentRegistry>,
}

impl std::fmt::Debug for PipelineRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineRunner")
            .field("default_pipeline", &self.default_pipeline)
            .field(
                "pipelines",
                &self.pipeline_configs.keys().collect::<Vec<_>>(),
            )
            .field(
                "tools",
                &self
                    .tool_executors
                    .iter()
                    .map(|(n, _)| n)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl PipelineRunner {
    /// Create from a pipeline config file + prompts directory.
    ///
    /// Spawns a kernel actor. The LLM provider is auto-configured from
    /// environment variables (OPENAI_API_KEY, ANTHROPIC_API_KEY, etc.).
    pub async fn from_json(
        pipeline_path: &str,
        prompts_dir: &str,
        model: Option<&str>,
    ) -> Result<Self> {
        let config_str = std::fs::read_to_string(pipeline_path).map_err(|e| {
            crate::types::Error::internal(format!(
                "Failed to read pipeline config '{pipeline_path}': {e}"
            ))
        })?;
        let pipeline_config: PipelineConfig =
            serde_json::from_str(&config_str).map_err(|e| {
                crate::types::Error::internal(format!("Failed to parse pipeline config: {e}"))
            })?;
        pipeline_config.validate().map_err(|e| {
            crate::types::Error::internal(format!("Invalid pipeline config: {e}"))
        })?;

        let pipeline_name = pipeline_config.name.clone();

        let kernel = Kernel::new();
        let cancel = CancellationToken::new();
        let handle = spawn_kernel(kernel, cancel.clone());

        let prompts = Arc::new(PromptRegistry::from_dir(prompts_dir));

        let model_name = model
            .map(String::from)
            .or_else(|| std::env::var("OPENAI_MODEL").ok())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        let llm: Arc<dyn LlmProvider> = Arc::new(GenaiProvider::new(model_name));

        let tools = Arc::new(ToolRegistry::new());

        let mut pipeline_configs = HashMap::new();
        pipeline_configs.insert(pipeline_name.clone(), pipeline_config);

        let agents = crate::worker::agent_factory::AgentFactoryBuilder::new(
            llm.clone(),
            prompts.clone(),
            tools.clone(),
        )
        .add_pipelines(pipeline_configs.values().cloned())
        .build();

        Ok(Self {
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

    /// Create from pre-built components (for consumers that manage their own resources).
    pub fn from_parts(
        handle: KernelHandle,
        cancel: CancellationToken,
        pipeline_configs: HashMap<String, PipelineConfig>,
        default_pipeline: String,
        prompts: Arc<PromptRegistry>,
        llm: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        agents: Arc<AgentRegistry>,
    ) -> Self {
        Self {
            handle,
            cancel,
            pipeline_configs,
            default_pipeline,
            prompts,
            llm,
            tool_executors: Vec::new(),
            tools,
            agents,
        }
    }

    /// Register a tool executor. Rebuilds agent registry.
    pub fn register_tool_executor(&mut self, name: String, executor: Arc<dyn ToolExecutor>) {
        self.tool_executors.push((name, executor));
        self.rebuild_registries();
    }

    /// Register an additional pipeline config. Rebuilds agent registry.
    pub fn register_pipeline(&mut self, name: String, config: PipelineConfig) -> Result<()> {
        config.validate()?;
        self.pipeline_configs.insert(name, config);
        self.rebuild_registries();
        Ok(())
    }

    /// Run a pipeline to completion (buffered).
    pub async fn run(
        &self,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<WorkerResult> {
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let envelope = Envelope::new_minimal(user_id, &sid, input, metadata);

        crate::worker::run_pipeline_with_envelope(
            &self.handle,
            process_id,
            config,
            envelope,
            &self.agents,
        )
        .await
    }

    /// Run a pipeline with streaming events. Returns typed event channel.
    pub async fn stream(
        &self,
        input: &str,
        user_id: &str,
        session_id: Option<&str>,
        pipeline_name: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<(JoinHandle<Result<WorkerResult>>, mpsc::Receiver<PipelineEvent>)> {
        let config = self.get_pipeline_config(pipeline_name)?;
        let process_id = ProcessId::new();
        let sid = session_id
            .map(String::from)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let envelope = Envelope::new_minimal(user_id, &sid, input, metadata);

        crate::worker::run_pipeline_streaming(
            self.handle.clone(),
            process_id,
            config,
            envelope,
            self.agents.clone(),
        )
        .await
    }

    /// Checkpoint a running process.
    pub async fn checkpoint(
        &self,
        process_id: &ProcessId,
    ) -> Result<crate::kernel::checkpoint::CheckpointSnapshot> {
        self.handle.checkpoint(process_id.clone()).await
    }

    /// Resume a pipeline from a checkpoint snapshot.
    pub async fn resume_from_checkpoint(
        &self,
        snapshot: crate::kernel::checkpoint::CheckpointSnapshot,
        pipeline_name: Option<&str>,
    ) -> Result<ProcessId> {
        let config = self.get_pipeline_config(pipeline_name)?;
        self.handle.resume_from_checkpoint(snapshot, config).await
    }

    /// Get a pipeline config by name.
    pub fn describe_pipeline(&self, name: Option<&str>) -> Option<&PipelineConfig> {
        let key = name.unwrap_or(&self.default_pipeline);
        self.pipeline_configs.get(key)
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.list_all_tools()
    }

    /// Access the kernel handle (for advanced consumers).
    pub fn handle(&self) -> &KernelHandle {
        &self.handle
    }

    /// Get the default pipeline name.
    pub fn default_pipeline(&self) -> &str {
        &self.default_pipeline
    }

    /// Graceful shutdown.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Get session state for a running process.
    pub async fn get_session_state(
        &self,
        process_id: &ProcessId,
    ) -> Result<crate::kernel::orchestrator_types::SessionState> {
        self.handle.get_session_state(process_id).await
    }

    // -- internal --

    fn get_pipeline_config(&self, name: Option<&str>) -> Result<PipelineConfig> {
        let key = name.unwrap_or(&self.default_pipeline);
        self.pipeline_configs.get(key).cloned().ok_or_else(|| {
            crate::types::Error::internal(format!(
                "Pipeline '{}' not found. Available: {:?}",
                key,
                self.pipeline_configs.keys().collect::<Vec<_>>()
            ))
        })
    }

    fn rebuild_registries(&mut self) {
        let mut builder = ToolRegistryBuilder::new();
        for (name, executor) in &self.tool_executors {
            builder = builder.add_tool(name.clone(), executor.clone());
        }
        let tools = builder.build();

        let agents = crate::worker::agent_factory::AgentFactoryBuilder::new(
            self.llm.clone(),
            self.prompts.clone(),
            tools.clone(),
        )
        .add_pipelines(self.pipeline_configs.values().cloned())
        .build();

        self.tools = tools;
        self.agents = agents;
    }
}
