//! Agent factory — builds AgentRegistry from pipeline config.
//!
//! Single decision tree used by both Rust consumers and PyO3 runner.
//! Handles PipelineAgent OnceLock backfill automatically.

use std::collections::HashMap;
use std::sync::Arc;

use crate::kernel::orchestrator_types::{NodeKind, PipelineConfig};
use crate::worker::agent::{
    Agent, AgentRegistry, DeterministicAgent, LlmAgent, McpDelegatingAgent, PipelineAgent,
};
use crate::worker::handle::KernelHandle;
use crate::worker::llm::LlmProvider;
use crate::worker::prompts::PromptRegistry;
use crate::worker::tools::{AclToolExecutor, ToolRegistry};

/// Builds AgentRegistry from pipeline configs + shared resources.
///
/// # Example
/// ```ignore
/// let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
///     .add_pipeline(config)
///     .build();
/// ```
pub struct AgentFactoryBuilder {
    llm: Arc<dyn LlmProvider>,
    prompts: Arc<PromptRegistry>,
    tools: Arc<ToolRegistry>,
    handle: KernelHandle,
    pipeline_configs: HashMap<String, PipelineConfig>,
}

impl std::fmt::Debug for AgentFactoryBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentFactoryBuilder")
            .field("pipelines", &self.pipeline_configs.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AgentFactoryBuilder {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        prompts: Arc<PromptRegistry>,
        tools: Arc<ToolRegistry>,
        handle: KernelHandle,
    ) -> Self {
        Self {
            llm,
            prompts,
            tools,
            handle,
            pipeline_configs: HashMap::new(),
        }
    }

    /// Add a single pipeline config.
    pub fn add_pipeline(mut self, config: PipelineConfig) -> Self {
        self.pipeline_configs.insert(config.name.clone(), config);
        self
    }

    /// Add multiple pipeline configs.
    pub fn add_pipelines(mut self, configs: impl IntoIterator<Item = PipelineConfig>) -> Self {
        for config in configs {
            self.pipeline_configs.insert(config.name.clone(), config);
        }
        self
    }

    /// Build AgentRegistry with OnceLock backfill for PipelineAgents.
    pub fn build(self) -> Arc<AgentRegistry> {
        let mut registry = AgentRegistry::new();
        for config in self.pipeline_configs.values() {
            merge_agents(&mut registry, config, &self);
        }
        let agents = Arc::new(registry);
        backfill_pipeline_agents(&agents);
        agents
    }
}

fn merge_agents(
    registry: &mut AgentRegistry,
    config: &PipelineConfig,
    ctx: &AgentFactoryBuilder,
) {
    for stage in &config.stages {
        let agent_name = &stage.agent;
        if agent_name.is_empty() || registry.get(agent_name).is_some() {
            continue; // Skip empty or already-registered (first wins)
        }

        // Per-stage ACL: wrap ToolRegistry if allowed_tools is set
        let stage_tools = match &stage.allowed_tools {
            Some(allowed) if !allowed.is_empty() => {
                AclToolExecutor::wrap_registry(ctx.tools.clone(), allowed)
            }
            _ => ctx.tools.clone(),
        };

        let agent: Arc<dyn Agent> = match stage.node_kind {
            NodeKind::Gate => Arc::new(DeterministicAgent),

            _ if stage.agent_config.child_pipeline.is_some() => {
                match stage
                    .agent_config
                    .child_pipeline
                    .as_deref()
                    .and_then(|n| ctx.pipeline_configs.get(n))
                {
                    Some(child_config) => Arc::new(PipelineAgent {
                        pipeline_name: stage
                            .agent_config
                            .child_pipeline
                            .clone()
                            .unwrap_or_default(),
                        pipeline_config: child_config.clone(),
                        handle: ctx.handle.clone(),
                        agents: std::sync::OnceLock::new(),
                    }),
                    None => {
                        tracing::warn!(
                            agent = %agent_name,
                            child = ?stage.agent_config.child_pipeline,
                            "child_pipeline config not found, using DeterministicAgent"
                        );
                        Arc::new(DeterministicAgent)
                    }
                }
            }

            _ if stage.agent_config.has_llm => {
                let prompt_key = stage
                    .agent_config
                    .prompt_key
                    .clone()
                    .unwrap_or_else(|| agent_name.clone());
                Arc::new(LlmAgent {
                    llm: ctx.llm.clone(),
                    prompts: ctx.prompts.clone(),
                    tools: stage_tools, // ACL-filtered if allowed_tools set
                    prompt_key,
                    temperature: stage.agent_config.temperature,
                    max_tokens: stage.agent_config.max_tokens,
                    model: stage.agent_config.model_role.clone(),
                    max_tool_rounds: stage.agent_config.max_tool_rounds,
                })
            }

            _ => {
                if ctx.tools.get(agent_name).is_some() {
                    Arc::new(McpDelegatingAgent {
                        tool_name: agent_name.clone(),
                        tools: stage_tools, // ACL-filtered if allowed_tools set
                    })
                } else {
                    Arc::new(DeterministicAgent)
                }
            }
        };

        registry.register(agent_name.clone(), agent);
    }
}

fn backfill_pipeline_agents(agents: &Arc<AgentRegistry>) {
    for name in agents.list_names() {
        if let Some(agent) = agents.get(&name) {
            if let Some(pa) =
                (agent.as_ref() as &dyn std::any::Any).downcast_ref::<PipelineAgent>()
            {
                let _ = pa.agents.set(agents.clone());
            }
        }
    }
}
