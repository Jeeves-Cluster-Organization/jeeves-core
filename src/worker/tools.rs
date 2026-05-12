//! Tool executor trait and registry.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::instrument;

use crate::tools::{ToolAccessPolicy, ToolCatalog, ToolHealthTracker};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool execution output.
///
/// `data` is structured JSON for pipeline mechanics (state merge, routing).
/// `content` is multimodal parts used only when constructing the next LLM
/// message; empty `content` falls back to `data.to_string()` as text.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub data: serde_json::Value,
    pub content: Vec<ContentPart>,
}

impl ToolOutput {
    pub fn json(data: serde_json::Value) -> Self {
        Self { data, content: vec![] }
    }

    pub fn with_content(data: serde_json::Value, content: Vec<ContentPart>) -> Self {
        Self { data, content }
    }
}

/// Multimodal content part for LLM messages. `content_type` is a MIME string;
/// providers map it to their native format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    /// Resolved lazily by a registered [`ContentResolver`]; unresolvable refs
    /// degrade to a text placeholder.
    Ref {
        content_type: String,
        ref_id: String,
    },
    /// Small inline binary (icons, thumbnails, charts) — base64 on the wire.
    Blob {
        content_type: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
}

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

/// Resolves a [`ContentPart::Ref`] to bytes when the next LLM message is built.
/// Consumers implement this to bridge external content stores (frame buffers,
/// file caches) into the message pipeline.
pub trait ContentResolver: Send + Sync + std::fmt::Debug {
    /// `None` when the ref is stale or unknown.
    fn resolve(&self, ref_id: &str, content_type: &str) -> Option<Vec<u8>>;
}

/// Returned by [`ToolExecutor::requires_confirmation`] to gate destructive
/// calls behind user approval (suspends the agent via `FlowInterrupt`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    pub message: String,
    pub action_data: Option<serde_json::Value>,
}

#[async_trait]
pub trait ToolExecutor: Send + Sync + std::fmt::Debug {
    async fn execute(&self, name: &str, params: serde_json::Value) -> crate::types::Result<ToolOutput>;

    fn list_tools(&self) -> Vec<ToolInfo>;

    /// Default: no confirmation. Override for destructive tools.
    fn requires_confirmation(&self, _name: &str, _params: &serde_json::Value) -> Option<ConfirmationRequest> {
        None
    }
}

/// Tool executors keyed by name, with an optional policy / catalog / health
/// chain gated around every `execute_for` call:
///
/// 1. [`ToolAccessPolicy`] — agent × tool ACL; default-deny when attached.
/// 2. [`ToolCatalog`] — typed param validation for tools listed in the catalog.
/// 3. [`ToolHealthTracker`] — circuit-breaker + sliding-window metrics.
///
/// Each gate is opt-in. Outcomes (success/failure + latency) are recorded into
/// the health tracker after execution.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
    access_policy: Option<Arc<ToolAccessPolicy>>,
    catalog: Option<Arc<ToolCatalog>>,
    health: Option<Arc<Mutex<ToolHealthTracker>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) {
        self.executors.insert(name.into(), executor);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolExecutor>> {
        self.executors.get(name)
    }

    /// Runs the full policy → catalog → health → executor → record chain.
    /// `agent_name` must always be supplied; an attached `ToolAccessPolicy`
    /// rejects calls from agents without a matching grant.
    #[instrument(skip(self, params), fields(tool = %name, agent = %agent_name))]
    pub async fn execute_for(
        &self,
        agent_name: &str,
        name: &str,
        params: serde_json::Value,
    ) -> crate::types::Result<ToolOutput> {
        if let Some(policy) = &self.access_policy {
            if !policy.check_access(agent_name, name) {
                return Err(crate::types::Error::policy_violation(format!(
                    "Agent '{}' is not granted tool '{}'",
                    agent_name, name
                )));
            }
        }

        if let Some(catalog) = &self.catalog {
            if catalog.has_tool(name) {
                let errors = catalog.validate_params(name, &params)?;
                if !errors.is_empty() {
                    return Err(crate::types::Error::validation(format!(
                        "Invalid params for tool '{}': {}",
                        name,
                        errors.join("; ")
                    )));
                }
            }
        }

        if let Some(health) = &self.health {
            let broken = health
                .lock()
                .map(|h| h.should_circuit_break(name))
                .unwrap_or(false);
            if broken {
                return Err(crate::types::Error::policy_violation(format!(
                    "Tool '{}' circuit-broken (too many recent failures)",
                    name
                )));
            }
        }

        let executor = self
            .executors
            .get(name)
            .ok_or_else(|| crate::types::Error::not_found(format!("Tool not found: {}", name)))?;

        let start = Instant::now();
        let result = executor.execute(name, params).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        if let Some(health) = &self.health {
            let (success, error_type) = match &result {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_error_code().to_string())),
            };
            if let Ok(mut h) = health.lock() {
                h.record_execution(name, success, latency_ms, error_type);
            }
        }

        result
    }

    pub fn list_all_tools(&self) -> Vec<ToolInfo> {
        self.executors
            .values()
            .flat_map(|e| e.list_tools())
            .collect()
    }

    pub fn requires_confirmation(&self, name: &str, params: &serde_json::Value) -> Option<ConfirmationRequest> {
        self.executors.get(name)?.requires_confirmation(name, params)
    }

    pub fn access_policy(&self) -> Option<&Arc<ToolAccessPolicy>> {
        self.access_policy.as_ref()
    }

    pub fn catalog(&self) -> Option<&Arc<ToolCatalog>> {
        self.catalog.as_ref()
    }

    pub fn health_tracker(&self) -> Option<&Arc<Mutex<ToolHealthTracker>>> {
        self.health.as_ref()
    }
}

/// Fluent builder for [`ToolRegistry`] — attach executors and the optional
/// policy / catalog / health chain in one expression.
#[derive(Debug, Default)]
pub struct ToolRegistryBuilder {
    executors: Vec<(String, Arc<dyn ToolExecutor>)>,
    access_policy: Option<Arc<ToolAccessPolicy>>,
    catalog: Option<Arc<ToolCatalog>>,
    health: Option<Arc<Mutex<ToolHealthTracker>>>,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers every tool listed by `executor` against the same Arc.
    pub fn add_executor(mut self, executor: Arc<dyn ToolExecutor>) -> Self {
        for info in executor.list_tools() {
            self.executors.push((info.name, executor.clone()));
        }
        self
    }

    pub fn add_tool(mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) -> Self {
        self.executors.push((name.into(), executor));
        self
    }

    pub fn with_access_policy(mut self, policy: Arc<ToolAccessPolicy>) -> Self {
        self.access_policy = Some(policy);
        self
    }

    pub fn with_catalog(mut self, catalog: Arc<ToolCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    pub fn with_health_tracker(mut self, health: Arc<Mutex<ToolHealthTracker>>) -> Self {
        self.health = Some(health);
        self
    }

    pub fn build(self) -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        for (name, executor) in self.executors {
            registry.register(name, executor);
        }
        registry.access_policy = self.access_policy;
        registry.catalog = self.catalog;
        registry.health = self.health;
        Arc::new(registry)
    }
}

/// Wraps a [`ToolExecutor`] (or whole registry, via [`wrap_registry`]) with an
/// explicit allow-list. The LLM never sees the disallowed tools because
/// `list_tools()` is filtered; `execute()` rejects them with
/// `Error::policy_violation`.
///
/// [`wrap_registry`]: AclToolExecutor::wrap_registry
#[derive(Debug)]
pub struct AclToolExecutor {
    inner: Arc<dyn ToolExecutor>,
    allowed: std::collections::HashSet<String>,
}

impl AclToolExecutor {
    pub fn new(inner: Arc<dyn ToolExecutor>, allowed: impl IntoIterator<Item = String>) -> Self {
        Self {
            inner,
            allowed: allowed.into_iter().collect(),
        }
    }

    /// Empty `allowed` yields a registry with zero tools — pure text generation.
    pub fn wrap_registry(registry: Arc<ToolRegistry>, allowed: &[String]) -> Arc<ToolRegistry> {
        if allowed.is_empty() {
            return Arc::new(ToolRegistry::new());
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

    #[tokio::test]
    async fn acl_wrap_registry_search_read_delete() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["search", "read", "delete"]))
            .build();

        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["search".to_string(), "read".to_string()],
        );

        assert!(wrapped.get("search").is_some());
        assert!(wrapped.get("read").is_some());
        assert!(wrapped.get("delete").is_none());

        let delete_result = wrapped.execute_for("agent", "delete", serde_json::json!({})).await;
        assert!(delete_result.is_err());
        let err_msg = delete_result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found") || err_msg.contains("not in allowed_tools"),
            "unexpected error: {err_msg}",
        );

        let search_result = wrapped.execute_for("agent", "search", serde_json::json!({})).await;
        assert!(search_result.is_ok());
    }

    #[tokio::test]
    async fn acl_wrap_registry_list_tools_only_shows_allowed() {
        // Each tool gets its own executor so `list_all_tools` counts correctly;
        // a shared executor's `list_tools()` returns every tool it knows about.
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
        assert!(tool_names.contains(&"alpha"));
        assert!(tool_names.contains(&"gamma"));
        assert!(!tool_names.contains(&"beta"));
        assert!(!tool_names.contains(&"delta"));
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn acl_wrap_registry_silently_skips_unknown_allowed_tool() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(mock_executor(&["search"]))
            .build();

        let wrapped = AclToolExecutor::wrap_registry(
            registry,
            &["search".to_string(), "nonexistent".to_string()],
        );

        assert_eq!(wrapped.list_all_tools().len(), 1);
        assert!(wrapped.get("search").is_some());
        assert!(wrapped.get("nonexistent").is_none());
    }

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
        assert!(acl.requires_confirmation("delete_all", &serde_json::json!({})).is_none());
    }

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

    #[derive(Debug)]
    struct FlakyExecutor {
        fail: bool,
    }

    #[async_trait]
    impl ToolExecutor for FlakyExecutor {
        async fn execute(&self, name: &str, _params: serde_json::Value) -> crate::types::Result<ToolOutput> {
            if self.fail {
                Err(crate::types::Error::internal(format!("{} failed", name)))
            } else {
                Ok(ToolOutput::json(serde_json::json!({"tool": name, "ok": true})))
            }
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            vec![ToolInfo {
                name: "do_thing".into(),
                description: "test tool".into(),
                parameters: serde_json::json!({"type": "object"}),
            }]
        }
    }

    #[tokio::test]
    async fn execute_for_denies_when_policy_lacks_grant() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("planner", "do_thing");
        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(FlakyExecutor { fail: false }))
            .with_access_policy(Arc::new(policy))
            .build();

        let denied = registry
            .execute_for("reporter", "do_thing", serde_json::json!({}))
            .await;
        assert!(denied.is_err());
        let msg = denied.unwrap_err().to_string();
        assert!(msg.contains("not granted"), "unexpected error: {msg}");

        let allowed = registry
            .execute_for("planner", "do_thing", serde_json::json!({}))
            .await;
        assert!(allowed.is_ok());
    }

    #[tokio::test]
    async fn execute_for_default_denies_when_policy_has_no_grants_for_agent() {
        let policy = ToolAccessPolicy::new();
        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(FlakyExecutor { fail: false }))
            .with_access_policy(Arc::new(policy))
            .build();

        let result = registry
            .execute_for("anyone", "do_thing", serde_json::json!({}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_for_passes_when_no_policy_is_set() {
        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(FlakyExecutor { fail: false }))
            .build();

        let result = registry
            .execute_for("anyone", "do_thing", serde_json::json!({}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_for_validates_params_against_catalog() {
        use crate::envelope::enums::{RiskSemantic, RiskSeverity, ToolCategory};
        use crate::tools::catalog::{ParamDef, ParamType, ToolEntry};

        let mut catalog = ToolCatalog::new();
        catalog.register(ToolEntry {
            id: "do_thing".into(),
            description: "test".into(),
            parameters: vec![ParamDef {
                name: "query".into(),
                param_type: ParamType::String,
                description: "search query".into(),
                default: None,
            }],
            category: ToolCategory::Read,
            risk_semantic: RiskSemantic::ReadOnly,
            risk_severity: RiskSeverity::Low,
        }).unwrap();

        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(FlakyExecutor { fail: false }))
            .with_catalog(Arc::new(catalog))
            .build();

        let bad = registry.execute_for("test_agent", "do_thing", serde_json::json!({})).await;
        assert!(bad.is_err());
        let msg = bad.unwrap_err().to_string();
        assert!(
            msg.contains("Missing required parameter") || msg.contains("Invalid params"),
            "unexpected error: {msg}",
        );

        let good = registry.execute_for("test_agent", "do_thing", serde_json::json!({"query": "rust"})).await;
        assert!(good.is_ok());
    }

    #[tokio::test]
    async fn execute_for_circuit_breaks_after_threshold_failures() {
        let cfg = crate::tools::health::HealthConfig {
            circuit_break_error_threshold: 3,
            window_size: 50,
            ..Default::default()
        };
        let tracker = Arc::new(Mutex::new(ToolHealthTracker::new(cfg)));
        let registry = ToolRegistryBuilder::new()
            .add_executor(Arc::new(FlakyExecutor { fail: true }))
            .with_health_tracker(tracker.clone())
            .build();

        for _ in 0..3 {
            let _ = registry.execute_for("test_agent", "do_thing", serde_json::json!({})).await;
        }

        let blocked = registry.execute_for("test_agent", "do_thing", serde_json::json!({})).await;
        assert!(blocked.is_err());
        let msg = blocked.unwrap_err().to_string();
        assert!(msg.contains("circuit-broken"), "unexpected error: {msg}");

        assert!(tracker.lock().unwrap().should_circuit_break("do_thing"));
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
