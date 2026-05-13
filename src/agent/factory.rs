//! Agent factory — builds an `AgentRegistry` from one or more `Workflow`s.

use std::collections::HashMap;
use std::sync::Arc;

use crate::workflow::Workflow;
use crate::agent::{
    Agent, AgentRegistry, DeterministicAgent, LlmAgent, ToolDelegatingAgent,
};
use crate::agent::llm::LlmProvider;
use crate::agent::prompts::PromptRegistry;
use crate::tools::{AclToolExecutor, ContentResolver, ToolRegistry};

/// Builds an `AgentRegistry` from one or more `Workflow`s plus shared resources.
///
/// # Example
/// ```text
/// let agents = AgentFactoryBuilder::new(llm, prompts, tools)
///     .add_workflow(workflow)
///     .build();
/// ```
pub struct AgentFactoryBuilder {
    llm: Arc<dyn LlmProvider>,
    prompts: Arc<PromptRegistry>,
    tools: Arc<ToolRegistry>,
    workflows: HashMap<String, Workflow>,
    content_resolver: Option<Arc<dyn ContentResolver>>,
    hooks: Vec<crate::agent::hooks::DynHook>,
    agent_hooks: Vec<crate::agent::hooks::DynAgentHook>,
}

impl std::fmt::Debug for AgentFactoryBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentFactoryBuilder")
            .field("workflows", &self.workflows.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AgentFactoryBuilder {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        prompts: Arc<PromptRegistry>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            llm,
            prompts,
            tools,
            workflows: HashMap::new(),
            content_resolver: None,
            hooks: Vec::new(),
            agent_hooks: Vec::new(),
        }
    }

    /// Set a content resolver for lazy `ContentPart::Ref` resolution in LLM agents.
    pub fn with_content_resolver(mut self, resolver: Arc<dyn ContentResolver>) -> Self {
        self.content_resolver = Some(resolver);
        self
    }

    /// Register an `LlmAgentHook` applied to every `LlmAgent` built by this
    /// factory. Hooks run in registration order; for `before_tool_call`,
    /// the first non-`Continue` decision wins.
    pub fn with_hook(mut self, hook: crate::agent::hooks::DynHook) -> Self {
        self.hooks.push(hook);
        self
    }

    /// Register an `AgentHook` fired by the runner around every
    /// `Agent::process()` call. Works for all agent kinds.
    pub fn with_agent_hook(mut self, hook: crate::agent::hooks::DynAgentHook) -> Self {
        self.agent_hooks.push(hook);
        self
    }

    /// Add a single workflow.
    pub fn add_workflow(mut self, workflow: Workflow) -> Self {
        self.workflows.insert(workflow.name.clone(), workflow);
        self
    }

    /// Add multiple workflows.
    pub fn add_workflows(mut self, workflows: impl IntoIterator<Item = Workflow>) -> Self {
        for workflow in workflows {
            self.workflows.insert(workflow.name.clone(), workflow);
        }
        self
    }

    /// Build an `AgentRegistry` from the registered workflows.
    pub fn build(self) -> Arc<AgentRegistry> {
        let mut registry = AgentRegistry::new();
        for config in self.workflows.values() {
            merge_agents(&mut registry, config, &self);
        }
        for hook in &self.agent_hooks {
            registry.register_agent_hook(hook.clone());
        }
        Arc::new(registry)
    }
}

fn merge_agents(
    registry: &mut AgentRegistry,
    config: &Workflow,
    ctx: &AgentFactoryBuilder,
) {
    for stage in &config.stages {
        let agent_name = &stage.agent;
        if agent_name.is_empty() || registry.get(agent_name.as_str()).is_some() {
            continue; // Skip empty or already-registered (first wins)
        }

        // Default-deny: with no policy attached, agents get zero tools.
        let allowed = ctx
            .tools
            .access_policy()
            .map(|p| p.tools_for_agent(agent_name.as_str()))
            .unwrap_or_default();
        let stage_tools = AclToolExecutor::wrap_registry(ctx.tools.clone(), &allowed);

        let agent: Arc<dyn Agent> = if stage.agent_config.has_llm {
            let prompt_key = stage
                .agent_config
                .prompt_key
                .clone()
                .unwrap_or_else(|| agent_name.as_str().into());
            Arc::new(LlmAgent {
                llm: ctx.llm.clone(),
                prompts: ctx.prompts.clone(),
                tools: stage_tools,
                agent_name: agent_name.clone(),
                prompt_key,
                temperature: stage.agent_config.temperature,
                max_tokens: stage.agent_config.max_tokens,
                model: stage.agent_config.model_role.clone(),
                max_tool_rounds: crate::agent::DEFAULT_MAX_TOOL_ROUNDS,
                content_resolver: ctx.content_resolver.clone(),
                hooks: ctx.hooks.clone(),
            })
        } else if ctx.tools.get(agent_name.as_str()).is_some() {
            Arc::new(ToolDelegatingAgent {
                agent_name: agent_name.clone(),
                tool_name: agent_name.as_str().into(),
                tools: stage_tools,
            })
        } else {
            Arc::new(DeterministicAgent)
        };

        registry.register(agent_name.clone(), agent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{AgentConfig, Stage};
    use crate::agent::llm::mock::MockLlmProvider;
    use crate::tools::{ToolExecutor, ToolInfo, ToolRegistryBuilder};
    use std::any::Any;

    fn test_stage(name: &str, has_llm: bool) -> Stage {
        Stage {
            name: name.into(),
            agent: name.into(),
            agent_config: AgentConfig {
                has_llm,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn test_config(name: &str, stages: Vec<Stage>) -> Workflow {
        Workflow::test_default(name, stages)
    }

    #[derive(Debug)]
    struct DummyToolExecutor;

    #[async_trait::async_trait]
    impl ToolExecutor for DummyToolExecutor {
        async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<crate::tools::ToolOutput> {
            Ok(crate::tools::ToolOutput::json(serde_json::json!({"tool": name})))
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            vec![ToolInfo {
                name: "my_tool".into(),
                description: "test".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            }]
        }
    }

    #[test]
    fn has_llm_creates_llm_agent() {
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![test_stage("llm1", true)]))
            .build();

        let agent = agents.get("llm1").expect("llm1 should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>().is_some());
    }

    #[test]
    fn tool_match_creates_tool_delegating_agent() {
        let tools = ToolRegistryBuilder::new()
            .add_executor(Arc::new(DummyToolExecutor))
            .build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![test_stage("my_tool", false)]))
            .build();

        let agent = agents.get("my_tool").expect("my_tool should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<ToolDelegatingAgent>().is_some());
    }

    #[test]
    fn no_tool_creates_deterministic() {
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![test_stage("unknown", false)]))
            .build();

        let agent = agents.get("unknown").expect("unknown should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<DeterministicAgent>().is_some());
    }

    #[test]
    fn first_win_skips_duplicate() {
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let stage1 = test_stage("agent1", true);
        let stage2 = test_stage("agent1", false);

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![stage1, stage2]))
            .build();

        // First registration wins — should be LlmAgent, not DeterministicAgent
        let agent = agents.get("agent1").expect("agent1 should exist");
        assert!((agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>().is_some());
    }

    #[test]
    fn multiple_pipelines_merged() {
        let tools = ToolRegistryBuilder::new().build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p1", vec![test_stage("a", true)]))
            .add_workflow(test_config("p2", vec![test_stage("b", false)]))
            .build();

        assert!(agents.get("a").is_some(), "agent from p1");
        assert!(agents.get("b").is_some(), "agent from p2");
    }

    #[test]
    fn policy_filters_registry_per_agent() {
        let mut policy = crate::tools::ToolAccessPolicy::new();
        policy.grant("filtered", "nonexistent_tool");
        let tools = ToolRegistryBuilder::new()
            .add_executor(Arc::new(DummyToolExecutor))
            .with_access_policy(Arc::new(policy))
            .build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![test_stage("filtered", true)]))
            .build();

        let agent = agents.get("filtered").expect("filtered should exist");
        let llm_agent = (agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>()
            .expect("should be LlmAgent");
        assert!(llm_agent.tools.get("my_tool").is_none());
    }

    #[test]
    fn no_policy_yields_empty_tool_registry() {
        let tools = ToolRegistryBuilder::new()
            .add_executor(Arc::new(DummyToolExecutor))
            .build();
        let llm = Arc::new(MockLlmProvider::default());
        let prompts = Arc::new(PromptRegistry::empty());

        let agents = AgentFactoryBuilder::new(llm, prompts, tools)
            .add_workflow(test_config("p", vec![test_stage("ungated", true)]))
            .build();

        let agent = agents.get("ungated").expect("ungated should exist");
        let llm_agent = (agent.as_ref() as &dyn Any).downcast_ref::<LlmAgent>()
            .expect("should be LlmAgent");
        assert!(llm_agent.tools.get("my_tool").is_none(), "no policy → no tools");
    }
}
