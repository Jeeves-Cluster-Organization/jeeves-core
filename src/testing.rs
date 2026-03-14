//! Pipeline test harness for integration testing.
//!
//! Provides a simple API to run pipelines with mock agents
//! without wiring up the full HTTP gateway.
//!
//! ```ignore
//! let result = PipelineTestHarness::new(config)
//!     .mock_agent("understand", my_agent)
//!     .run("Hello, world!")
//!     .await?;
//! assert!(result.terminated());
//! ```

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::envelope::Envelope;
use crate::kernel::orchestrator_types::PipelineConfig;
use crate::kernel::Kernel;
use crate::types::{ProcessId, Result};
use crate::worker::actor::spawn_kernel;
use crate::worker::agent::{Agent, AgentRegistry};
use crate::worker::llm::PipelineEvent;
use crate::worker::WorkerResult;

/// Test harness for running pipelines in isolation.
///
/// Register agents via `mock_agent()`, then call `run()` or `run_streaming()`.
/// For LLM-backed agents, construct `LlmAgent` with your mock LLM/tools
/// and register it as a mock agent directly.
pub struct PipelineTestHarness {
    config: PipelineConfig,
    agents: AgentRegistry,
}

impl PipelineTestHarness {
    /// Create a new test harness with the given pipeline config.
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            config,
            agents: AgentRegistry::new(),
        }
    }

    /// Register a mock agent by name.
    pub fn mock_agent(mut self, name: &str, agent: impl Agent + 'static) -> Self {
        self.agents.register(name, Arc::new(agent));
        self
    }

    /// Run the pipeline to completion with the given input (buffered mode).
    pub async fn run(self, input: &str) -> Result<WorkerResult> {
        let kernel = Kernel::new();
        let cancel = CancellationToken::new();
        let handle = spawn_kernel(kernel, cancel.clone());

        let pid = ProcessId::must(format!(
            "test_{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        ));
        let pipeline_name = self.config.name.clone();
        let envelope = Envelope::new_minimal("test_user", "test_session", input, None);

        handle
            .initialize_session(pid.clone(), self.config, envelope, false)
            .await?;

        let agents = Arc::new(self.agents);
        let result = crate::worker::run_pipeline_loop(&handle, &pid, &agents, None, &pipeline_name).await;

        cancel.cancel();
        result
    }

    /// Run the pipeline with streaming events.
    pub async fn run_streaming(
        self,
        input: &str,
    ) -> Result<(WorkerResult, Vec<PipelineEvent>)> {
        let kernel = Kernel::new();
        let cancel = CancellationToken::new();
        let handle = spawn_kernel(kernel, cancel.clone());

        let pid = ProcessId::must(format!(
            "test_{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        ));
        let envelope = Envelope::new_minimal("test_user", "test_session", input, None);

        let agents = Arc::new(self.agents);

        let (jh, mut rx) = crate::worker::run_pipeline_streaming(
            handle,
            pid,
            self.config,
            envelope,
            agents,
        )
        .await?;

        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }

        let result = jh.await.map_err(|e| crate::types::Error::internal(format!("Join error: {e}")))?;
        cancel.cancel();

        Ok((result?, events))
    }
}

impl std::fmt::Debug for PipelineTestHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PipelineTestHarness")
            .field("config", &self.config.name)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::builder::PipelineBuilder;
    use crate::worker::agent::DeterministicAgent;

    #[tokio::test]
    async fn test_harness_simple_pipeline() {
        let config = PipelineBuilder::new("test")
            .stage("s1", "s1")
                .has_llm(false)
                .default_next("s2")
                .done()
            .stage("s2", "s2")
                .has_llm(false)
                .done()
            .bounds(10, 10, 10)
            .build()
            .unwrap();

        let result = PipelineTestHarness::new(config)
            .mock_agent("s1", DeterministicAgent)
            .mock_agent("s2", DeterministicAgent)
            .run("hello")
            .await
            .unwrap();

        assert!(result.terminated());
    }

    #[tokio::test]
    async fn test_harness_streaming() {
        let config = PipelineBuilder::new("stream_test")
            .stage("s1", "s1")
                .has_llm(false)
                .done()
            .bounds(10, 10, 10)
            .build()
            .unwrap();

        let (result, events) = PipelineTestHarness::new(config)
            .mock_agent("s1", DeterministicAgent)
            .run_streaming("hello")
            .await
            .unwrap();

        assert!(result.terminated());
        assert!(!events.is_empty());
    }
}
