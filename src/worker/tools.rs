//! Tool executor trait and registry.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Metadata about a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool executor trait — implementations provide actual tool functionality.
#[async_trait]
pub trait ToolExecutor: Send + Sync + std::fmt::Debug {
    /// Execute a tool by name with JSON params, returning JSON result.
    async fn execute(&self, name: &str, params: serde_json::Value) -> crate::types::Result<serde_json::Value>;

    /// List available tools.
    fn list_tools(&self) -> Vec<ToolInfo>;
}

/// Registry of tool executors keyed by name.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool executor.
    pub fn register(&mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) {
        self.executors.insert(name.into(), executor);
    }

    /// Get a tool executor by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolExecutor>> {
        self.executors.get(name)
    }

    /// Execute a tool by name.
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> crate::types::Result<serde_json::Value> {
        let executor = self
            .executors
            .get(name)
            .ok_or_else(|| crate::types::Error::not_found(format!("Tool not found: {}", name)))?;
        executor.execute(name, params).await
    }

    /// List all available tools across all executors.
    pub fn list_all_tools(&self) -> Vec<ToolInfo> {
        self.executors
            .values()
            .flat_map(|e| e.list_tools())
            .collect()
    }
}

/// No-op tool executor for agents that don't need tools.
#[derive(Debug)]
pub struct NoopToolExecutor;

#[async_trait]
impl ToolExecutor for NoopToolExecutor {
    async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<serde_json::Value> {
        Err(crate::types::Error::not_found(format!(
            "No tool executor registered for: {}",
            name
        )))
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![]
    }
}
