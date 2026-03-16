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
/// ```text
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::orchestrator_types::{AgentConfig, PipelineStage};
    use crate::kernel::Kernel;
    use crate::worker::actor::spawn_kernel;
    use crate::worker::llm::mock::MockLlmProvider;
    use crate::worker::tools::{ToolExecutor, ToolInfo, ToolRegistryBuilder};
    use std::any::Any;
    use tokio_util::sync::CancellationToken;

    fn test_stage(name: &str, has_llm: bool, node_kind: NodeKind) -> PipelineStage {
        PipelineStage {
            name: name.to_string(),
            agent: name.to_string(),
            node_kind,
            agent_config: AgentConfig {
                has_llm,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn test_config(name: &str, stages: Vec<PipelineStage>) -> PipelineConfig {
        PipelineConfig::test_default(name, stages)
    }

    fn spawn_test_kernel() -> (KernelHandle, CancellationToken) {
        let cancel = CancellationToken::new();
        let handle = spawn_kernel(Kernel::new(), cancel.clone());
        (handle, cancel)
    }

    #[derive(Debug)]
    struct DummyToolExecutor;

    #[async_trait::async_trait]
    impl ToolExecutor for DummyToolExecutor {
        async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<serde_json::Value> {
            Ok(serde_json::json!({"tool": name}))
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            vec![ToolInfo {
                name: "my_tool".to_string(),
                description: "test".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            }]
        }
    }

    #[tokio::test]
    async fn gate_creates_deterministic() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![test_stage("gate1", false, NodeKind::Gate)]))
            .build();

        let agent = agents.get("gate1").expect("gate1 should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<DeterministicAgent>().is_some());
        cancel.cancel();
    }

    #[tokio::test]
    async fn has_llm_creates_llm_agent() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![test_stage("llm1", true, NodeKind::Agent)]))
            .build();

        let agent = agents.get("llm1").expect("llm1 should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>().is_some());
        cancel.cancel();
    }

    #[tokio::test]
    async fn tool_match_creates_mcp_agent() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new()
            .add_executor(Arc::new(DummyToolExecutor))
            .build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![test_stage("my_tool", false, NodeKind::Agent)]))
            .build();

        let agent = agents.get("my_tool").expect("my_tool should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<McpDelegatingAgent>().is_some());
        cancel.cancel();
    }

    #[tokio::test]
    async fn no_tool_creates_deterministic() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![test_stage("unknown", false, NodeKind::Agent)]))
            .build();

        let agent = agents.get("unknown").expect("unknown should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<DeterministicAgent>().is_some());
        cancel.cancel();
    }

    #[tokio::test]
    async fn first_win_skips_duplicate() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let stage1 = test_stage("agent1", true, NodeKind::Agent);
        let stage2 = test_stage("agent1", false, NodeKind::Gate); // same name, different config

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![stage1, stage2]))
            .build();

        // First registration wins — should be LlmAgent, not DeterministicAgent
        let agent = agents.get("agent1").expect("agent1 should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>().is_some());
        cancel.cancel();
    }

    #[tokio::test]
    async fn multiple_pipelines_merged() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p1", vec![test_stage("a", true, NodeKind::Agent)]))
            .add_pipeline(test_config("p2", vec![test_stage("b", false, NodeKind::Gate)]))
            .build();

        assert!(agents.get("a").is_some(), "agent from p1");
        assert!(agents.get("b").is_some(), "agent from p2");
        cancel.cancel();
    }

    #[tokio::test]
    async fn allowed_tools_filters_registry() {
        let (handle, cancel) = spawn_test_kernel();
        let tools = ToolRegistryBuilder::new()
            .add_executor(Arc::new(DummyToolExecutor))
            .build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let mut stage = test_stage("filtered", true, NodeKind::Agent);
        stage.allowed_tools = Some(vec!["nonexistent_tool".to_string()]);

        let agents = AgentFactoryBuilder::new(llm, prompts, tools, handle)
            .add_pipeline(test_config("p", vec![stage]))
            .build();

        let agent = agents.get("filtered").expect("filtered should exist");
        let llm_agent = (agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>()
            .expect("should be LlmAgent");
        // The filtered registry should NOT contain "my_tool" since allowed_tools only has "nonexistent_tool"
        assert!(llm_agent.tools.get("my_tool").is_none(), "my_tool should be filtered out");
        cancel.cancel();
    }
}
