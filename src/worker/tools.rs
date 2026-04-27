//! Tool executor trait and registry.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;

/// Metadata about a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// =============================================================================
// ToolOutput + ContentPart — separates pipeline data from LLM content
// =============================================================================

/// Output from a tool execution.
///
/// Separates two concerns:
/// - `data`: structured JSON for pipeline mechanics (state merge, checkpoint, output_schema, routing).
/// - `content`: rich content parts consumed only by the LLM message builder (multimodal).
///
/// When `content` is empty, the agent falls back to `data.to_string()` for the LLM message.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Structured data for pipeline orchestration. Always lightweight JSON.
    pub data: serde_json::Value,
    /// Rich content for LLM message construction only. Empty = text-only (backward compat).
    pub content: Vec<ContentPart>,
}

impl ToolOutput {
    /// Create a text-only tool output (no rich content). Backward-compatible path.
    pub fn json(data: serde_json::Value) -> Self {
        Self { data, content: vec![] }
    }

    /// Create a tool output with both pipeline data and rich content parts.
    pub fn with_content(data: serde_json::Value, content: Vec<ContentPart>) -> Self {
        Self { data, content }
    }
}

/// A content part for multimodal LLM messages.
///
/// Uses MIME `content_type` strings (e.g. `image/jpeg`, `audio/wav`) so the kernel
/// never needs to understand specific content types — the LLM provider maps them
/// to the appropriate API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text content.
    Text { text: String },
    /// Reference to content stored externally, resolved lazily via [`ContentResolver`].
    Ref {
        content_type: String,
        ref_id: String,
    },
    /// Inline binary content (small items only — icons, thumbnails, charts).
    Blob {
        content_type: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
}

/// Serde helper for base64 encoding of `Vec<u8>` in Blob variant.
mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde::de::Error;

    pub fn serialize<S: Serializer>(data: &[u8], ser: S) -> Result<S::Ok, S::Error> {
        use base64::Engine;
        ser.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        use base64::Engine;
        let s = String::deserialize(de)?;
        base64::engine::general_purpose::STANDARD.decode(&s).map_err(D::Error::custom)
    }
}

// =============================================================================
// ContentResolver — consumer-provided lazy content resolution
// =============================================================================

/// Resolves [`ContentPart::Ref`] to actual bytes at LLM-send time.
///
/// Consumers register an implementation to bridge external content stores
/// (e.g. frame buffers, file caches) into the LLM message pipeline.
/// If no resolver is registered, Ref parts degrade to text placeholders.
pub trait ContentResolver: Send + Sync + std::fmt::Debug {
    /// Resolve a content reference to bytes. Returns `None` if the ref is stale or unknown.
    fn resolve(&self, ref_id: &str, content_type: &str) -> Option<Vec<u8>>;
}

/// Request for user confirmation before executing a tool.
///
/// Returned by `ToolExecutor::requires_confirmation()` when a tool operation
/// is destructive and needs explicit approval (e.g., deletes, destructive bash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    pub message: String,
    pub action_data: Option<serde_json::Value>,
}

/// Tool executor trait — implementations provide actual tool functionality.
#[async_trait]
pub trait ToolExecutor: Send + Sync + std::fmt::Debug {
    /// Execute a tool by name with JSON params, returning structured output.
    ///
    /// `ToolOutput.data` feeds pipeline mechanics (state, routing, checkpoint).
    /// `ToolOutput.content` feeds the LLM message builder (multimodal).
    async fn execute(&self, name: &str, params: serde_json::Value) -> crate::types::Result<ToolOutput>;

    /// List available tools.
    fn list_tools(&self) -> Vec<ToolInfo>;

    /// Check if a tool operation requires user confirmation before execution.
    ///
    /// Default: no confirmation needed. Override for destructive tools.
    fn requires_confirmation(&self, _name: &str, _params: &serde_json::Value) -> Option<ConfirmationRequest> {
        None
    }
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
    #[instrument(skip(self, params), fields(tool = %name))]
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> crate::types::Result<ToolOutput> {
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

    /// Check if a tool requires user confirmation before execution.
    pub fn requires_confirmation(&self, name: &str, params: &serde_json::Value) -> Option<ConfirmationRequest> {
        self.executors.get(name)?.requires_confirmation(name, params)
    }
}

/// Builder for composing tool registries from multiple sources.
///
/// Replaces the repeated 6-line registration loop in every consumer:
/// ```text
/// let tools = ToolRegistryBuilder::new()
///     .add_executor(Arc::new(MyTools::new()))
///     .add_executor(Arc::new(SearchTools::new()))
///     .add_tool("custom", Arc::new(custom_executor))
///     .build();
/// ```
#[derive(Debug, Default)]
pub struct ToolRegistryBuilder {
    executors: Vec<(String, Arc<dyn ToolExecutor>)>,
}

impl ToolRegistryBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register all tools from an executor (each tool name maps to the same Arc).
    pub fn add_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        for info in executor.list_tools() {
            self.executors.push((info.name, executor.clone()));
        }
        self
    }

    /// Register a single tool name to a specific executor.
    pub fn add_tool(mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) -> Self {
        self.executors.push((name.into(), executor));
        self
    }

    /// Build into Arc<ToolRegistry>.
    pub fn build(self) -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        for (name, executor) in self.executors {
            registry.register(name, executor);
        }
        Arc::new(registry)
    }
}

/// ACL-enforcing wrapper — filters tool access at the ToolRegistry layer.
///
/// All agent types (LlmAgent, ToolDelegatingAgent, etc.) get ACL enforcement
/// automatically when their ToolRegistry is wrapped with this.
///
/// - `list_tools()` returns only allowed tools (LLM never sees disallowed ones)
/// - `execute()` rejects disallowed tools with `Error::policy_violation`
#[derive(Debug)]
pub struct AclToolExecutor {
    inner: Arc<dyn ToolExecutor>,
    allowed: std::collections::HashSet<String>,
}

impl AclToolExecutor {
    /// Create a new ACL executor wrapping `inner` with only the `allowed` tool names.
    pub fn new(inner: Arc<dyn ToolExecutor>, allowed: impl IntoIterator<Item = String>) -> Self {
        Self {
            inner,
            allowed: allowed.into_iter().collect(),
        }
    }

    /// Create a filtered ToolRegistry from an existing one.
    ///
    /// Returns a new registry containing only the allowed tools.
    /// Empty `allowed` = zero tools (pure text generation).
    pub fn wrap_registry(registry: Arc<ToolRegistry>, allowed: &[String]) -> Arc<ToolRegistry> {
        if allowed.is_empty() {
            return Arc::new(ToolRegistry::new()); // Empty = no tools
        }
        let allowed_set: std::collections::HashSet<&str> =
            allowed.iter().map(|s| s.as_str()).collect();
        let mut filtered = ToolRegistry::new();
        for tool in registry.list_all_tools() {
            if allowed_set.contains(tool.name.as_str()) {
                if let Some(executor) = registry.get(&tool.name) {
                    filtered.register(tool.name, executor.clone());
                }
            }
        }
        Arc::new(filtered)
    }
}

#[async_trait]
impl ToolExecutor for AclToolExecutor {
    async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> crate::types::Result<ToolOutput> {
        if !self.allowed.contains(name) {
            return Err(crate::types::Error::policy_violation(format!(
                "Tool '{}' not in allowed_tools: {:?}",
                name, self.allowed
            )));
        }
        self.inner.execute(name, params).await
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        self.inner
            .list_tools()
            .into_iter()
            .filter(|t| self.allowed.contains(&t.name))
            .collect()
    }

    fn requires_confirmation(&self, name: &str, params: &serde_json::Value) -> Option<ConfirmationRequest> {
        if !self.allowed.contains(name) {
            return None;
        }
        self.inner.requires_confirmation(name, params)
    }
}

/// No-op tool executor for agents that don't need tools.
#[derive(Debug)]
pub struct NoopToolExecutor;

#[async_trait]
impl ToolExecutor for NoopToolExecutor {
    async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<ToolOutput> {
        Err(crate::types::Error::not_found(format!(
            "No tool executor registered for: {}",
            name
        )))
    }

    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockExecutor {
        tools: Vec<ToolInfo>,
    }

    #[async_trait]
    impl ToolExecutor for MockExecutor {
        async fn execute(
            &self,
            name: &str,
            _params: serde_json::Value,
        ) -> crate::types::Result<ToolOutput> {
            Ok(ToolOutput::json(serde_json::json!({"tool": name, "executed": true})))
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            self.tools.clone()
        }
    }

    fn mock_executor(names: &[&str]) -> Arc<dyn ToolExecutor> {
        Arc::new(MockExecutor {
            tools: names
                .iter()
                .map(|n| ToolInfo {
                    name: n.to_string(),
                    description: format!("{} tool", n),
                    parameters: serde_json::json!({"type": "object"}),
                })
                .collect(),
        })
    }

    #[test]
    fn builder_add_executor() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["a", "b"]))
            .build();
        assert!(registry.get("a").is_some());
        assert!(registry.get("b").is_some());
        assert!(registry.get("c").is_none());
    }

    #[test]
    fn builder_multiple_executors() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["a"]))
            .add_executor(mock_executor(&["b"]))
            .add_tool("c", mock_executor(&["c"]))
            .build();
        assert_eq!(registry.list_all_tools().len(), 3);
    }

    #[test]
    fn builder_last_wins_on_duplicate() {
        let exec1 = mock_executor(&["a"]);
        let exec2 = mock_executor(&["a"]);
        let registry = ToolRegistryBuilder::new()
            .add_executor(exec1)
            .add_executor(exec2)
            .build();
        assert!(registry.get("a").is_some());
    }

    #[tokio::test]
    async fn acl_allows_permitted_tool() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl =
            AclToolExecutor::new(inner, vec!["search".to_string(), "read".to_string()]);
        let result = acl.execute("search", serde_json::json!({})).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn acl_rejects_disallowed_tool() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl = AclToolExecutor::new(inner, vec!["search".to_string()]);
        let result = acl.execute("write", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not in allowed_tools"));
    }

    #[test]
    fn acl_filters_list_tools() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl =
            AclToolExecutor::new(inner, vec!["search".to_string(), "read".to_string()]);
        let tools = acl.list_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t.name == "search"));
        assert!(tools.iter().any(|t| t.name == "read"));
    }

    #[test]
    fn wrap_registry_empty_acl_returns_no_tools() {
        let registry = ToolRegistryBuilder::new()
            .add_tool("a", mock_executor(&["a"]))
            .add_tool("b", mock_executor(&["b"]))
            .build();
        let wrapped = AclToolExecutor::wrap_registry(registry.clone(), &[]);
        assert_eq!(wrapped.list_all_tools().len(), 0);
    }

    #[test]
    fn wrap_registry_filters_tools() {
        let registry = ToolRegistryBuilder::new()
            .add_tool("a", mock_executor(&["a"]))
            .add_tool("b", mock_executor(&["b"]))
            .add_tool("c", mock_executor(&["c"]))
            .build();
        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["a".to_string(), "c".to_string()],
        );
        assert_eq!(wrapped.list_all_tools().len(), 2);
        assert!(wrapped.get("a").is_some());
        assert!(wrapped.get("b").is_none());
        assert!(wrapped.get("c").is_some());
    }

    // =========================================================================
    // 9c: AclToolExecutor integration tests
    // =========================================================================

    #[tokio::test]
    async fn acl_wrap_registry_search_read_delete() {
        // Create a registry with tools: "search", "read", "delete"
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["search", "read", "delete"]))
            .build();

        // Wrap with allowed: ["search", "read"]
        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["search".to_string(), "read".to_string()],
        );

        // Verify wrapped.get("search") is Some
        assert!(wrapped.get("search").is_some(), "search should be accessible");

        // Verify wrapped.get("read") is Some
        assert!(wrapped.get("read").is_some(), "read should be accessible");

        // Verify wrapped.get("delete") is None
        assert!(wrapped.get("delete").is_none(), "delete should be filtered out");

        // Execute "delete" on wrapped → verify returns Err containing "not found"
        // (wrap_registry removes it from the registry entirely, so it's a "not found" error)
        let delete_result = wrapped.execute("delete", serde_json::json!({})).await;
        assert!(delete_result.is_err(), "executing delete should fail");
        let err_msg = delete_result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found") || err_msg.contains("not in allowed_tools"),
            "error should indicate tool is not available, got: {}",
            err_msg
        );

        // Execute "search" on wrapped → verify returns Ok
        let search_result = wrapped.execute("search", serde_json::json!({})).await;
        assert!(search_result.is_ok(), "executing search should succeed");
    }

    #[tokio::test]
    async fn acl_wrap_registry_list_tools_only_shows_allowed() {
        // Use separate executors per tool so list_all_tools counts correctly.
        // (A single executor's list_tools() returns all its tools regardless of
        // which registry key it's stored under.)
        let registry = ToolRegistryBuilder::new()
            .add_tool("alpha", mock_executor(&["alpha"]))
            .add_tool("beta", mock_executor(&["beta"]))
            .add_tool("gamma", mock_executor(&["gamma"]))
            .add_tool("delta", mock_executor(&["delta"]))
            .build();

        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["alpha".to_string(), "gamma".to_string()],
        );

        let tools = wrapped.list_all_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"alpha"), "alpha should be listed");
        assert!(tool_names.contains(&"gamma"), "gamma should be listed");
        assert!(!tool_names.contains(&"beta"), "beta should NOT be listed");
        assert!(!tool_names.contains(&"delta"), "delta should NOT be listed");
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn acl_wrap_registry_with_nonexistent_allowed_tool() {
        // If allowed list references a tool not in the registry, it's silently ignored
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["search"]))
            .build();

        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["search".to_string(), "nonexistent".to_string()],
        );

        // Only "search" should be present (nonexistent is silently skipped)
        assert_eq!(wrapped.list_all_tools().len(), 1);
        assert!(wrapped.get("search").is_some());
        assert!(wrapped.get("nonexistent").is_none());
    }

    // =========================================================================
    // Confirmation gate tests
    // =========================================================================

    #[derive(Debug)]
    struct ConfirmableExecutor;

    #[async_trait]
    impl ToolExecutor for ConfirmableExecutor {
        async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<ToolOutput> {
            Ok(ToolOutput::json(serde_json::json!({"tool": name, "executed": true})))
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            vec![
                ToolInfo { name: "safe_op".into(), description: "safe".into(), parameters: serde_json::json!({"type": "object"}) },
                ToolInfo { name: "delete_all".into(), description: "destructive".into(), parameters: serde_json::json!({"type": "object"}) },
            ]
        }
        fn requires_confirmation(&self, name: &str, _params: &serde_json::Value) -> Option<ConfirmationRequest> {
            if name == "delete_all" {
                Some(ConfirmationRequest {
                    message: "This will delete everything!".into(),
                    action_data: Some(serde_json::json!({"target": "all"})),
                })
            } else {
                None
            }
        }
    }

    #[test]
    fn confirmation_default_returns_none() {
        let executor = mock_executor(&["a"]);
        assert!(executor.requires_confirmation("a", &serde_json::json!({})).is_none());
    }

    #[test]
    fn confirmation_returns_request_for_destructive_tool() {
        let executor = ConfirmableExecutor;
        let result = executor.requires_confirmation("delete_all", &serde_json::json!({}));
        assert!(result.is_some());
        let req = result.unwrap();
        assert!(req.message.contains("delete everything"));
        assert!(req.action_data.is_some());
    }

    #[test]
    fn confirmation_returns_none_for_safe_tool() {
        let executor = ConfirmableExecutor;
        assert!(executor.requires_confirmation("safe_op", &serde_json::json!({})).is_none());
    }

    #[test]
    fn registry_delegates_confirmation() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(ConfirmableExecutor))
            .build();
        assert!(registry.requires_confirmation("delete_all", &serde_json::json!({})).is_some());
        assert!(registry.requires_confirmation("safe_op", &serde_json::json!({})).is_none());
        assert!(registry.requires_confirmation("nonexistent", &serde_json::json!({})).is_none());
    }

    #[test]
    fn acl_delegates_confirmation_for_allowed() {
        let inner: Arc<dyn ToolExecutor> = Arc::new(ConfirmableExecutor);
        let acl = AclToolExecutor::new(inner, vec!["delete_all".to_string(), "safe_op".to_string()]);
        assert!(acl.requires_confirmation("delete_all", &serde_json::json!({})).is_some());
        assert!(acl.requires_confirmation("safe_op", &serde_json::json!({})).is_none());
    }

    #[test]
    fn acl_blocks_confirmation_for_disallowed() {
        let inner: Arc<dyn ToolExecutor> = Arc::new(ConfirmableExecutor);
        let acl = AclToolExecutor::new(inner, vec!["safe_op".to_string()]);
        // delete_all not in allowed set → confirmation check returns None
        assert!(acl.requires_confirmation("delete_all", &serde_json::json!({})).is_none());
    }

    // =========================================================================
    // ToolOutput + ContentPart tests
    // =========================================================================

    #[test]
    fn tool_output_json_has_empty_content() {
        let out = ToolOutput::json(serde_json::json!({"key": "value"}));
        assert!(out.content.is_empty());
        assert_eq!(out.data["key"], "value");
    }

    #[test]
    fn tool_output_with_content_preserves_both() {
        let out = ToolOutput::with_content(
            serde_json::json!({"ref_id": "frame_1"}),
            vec![ContentPart::Ref {
                content_type: "image/jpeg".into(),
                ref_id: "frame_1".into(),
            }],
        );
        assert_eq!(out.data["ref_id"], "frame_1");
        assert_eq!(out.content.len(), 1);
        match &out.content[0] {
            ContentPart::Ref { content_type, ref_id } => {
                assert_eq!(content_type, "image/jpeg");
                assert_eq!(ref_id, "frame_1");
            }
            other => panic!("Expected Ref, got {:?}", other),
        }
    }

    #[test]
    fn content_part_serde_round_trip() {
        let parts = vec![
            ContentPart::Text { text: "hello".into() },
            ContentPart::Ref { content_type: "image/png".into(), ref_id: "img_1".into() },
        ];
        let json = serde_json::to_string(&parts).unwrap();
        let deserialized: Vec<ContentPart> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), 2);
        match &deserialized[0] {
            ContentPart::Text { text } => assert_eq!(text, "hello"),
            other => panic!("Expected Text, got {:?}", other),
        }
        match &deserialized[1] {
            ContentPart::Ref { content_type, ref_id } => {
                assert_eq!(content_type, "image/png");
                assert_eq!(ref_id, "img_1");
            }
            other => panic!("Expected Ref, got {:?}", other),
        }
    }
}
