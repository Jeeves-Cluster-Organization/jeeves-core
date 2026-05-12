//! `ToolExecutor` trait plus its value types and the `AclToolExecutor` wrapper.
//!
//! The [`ToolRegistry`](super::registry::ToolRegistry) consumes these to dispatch
//! tool calls; consumers implement `ToolExecutor` directly to plug in their own
//! tool surface.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::registry::ToolRegistry;
use crate::types::ToolName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: ToolName,
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
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], ser: S) -> Result<S::Ok, S::Error> {
        use base64::Engine;
        ser.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        use base64::Engine;
        let s = String::deserialize(de)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(D::Error::custom)
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
    fn requires_confirmation(
        &self,
        _name: &str,
        _params: &serde_json::Value,
    ) -> Option<ConfirmationRequest> {
        None
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
                if let Some(executor) = registry.get(tool.name.as_str()) {
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
            .filter(|t| self.allowed.contains(t.name.as_str()))
            .collect()
    }

    fn requires_confirmation(
        &self,
        name: &str,
        params: &serde_json::Value,
    ) -> Option<ConfirmationRequest> {
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
    async fn execute(
        &self,
        name: &str,
        _params: serde_json::Value,
    ) -> crate::types::Result<ToolOutput> {
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
            ContentPart::Ref {
                content_type: "image/png".into(),
                ref_id: "img_1".into(),
            },
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
            Ok(ToolOutput::json(
                serde_json::json!({"tool": name, "executed": true}),
            ))
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
                    name: (*n).into(),
                    description: format!("{} tool", n),
                    parameters: serde_json::json!({"type": "object"}),
                })
                .collect(),
        })
    }

    #[tokio::test]
    async fn acl_allows_permitted_tool() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl = AclToolExecutor::new(inner, vec!["search".to_string(), "read".to_string()]);
        let result = acl.execute("search", serde_json::json!({})).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn acl_rejects_disallowed_tool() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl = AclToolExecutor::new(inner, vec!["search".to_string()]);
        let result = acl.execute("write", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not in allowed_tools"));
    }

    #[test]
    fn acl_filters_list_tools() {
        let inner = mock_executor(&["search", "read", "write"]);
        let acl = AclToolExecutor::new(inner, vec!["search".to_string(), "read".to_string()]);
        let tools = acl.list_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().any(|t| t.name.as_str() == "search"));
        assert!(tools.iter().any(|t| t.name.as_str() == "read"));
    }

    #[derive(Debug)]
    struct ConfirmableExecutor;

    #[async_trait]
    impl ToolExecutor for ConfirmableExecutor {
        async fn execute(
            &self,
            name: &str,
            _params: serde_json::Value,
        ) -> crate::types::Result<ToolOutput> {
            Ok(ToolOutput::json(
                serde_json::json!({"tool": name, "executed": true}),
            ))
        }
        fn list_tools(&self) -> Vec<ToolInfo> {
            vec![
                ToolInfo {
                    name: "safe_op".into(),
                    description: "safe".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
                ToolInfo {
                    name: "delete_all".into(),
                    description: "destructive".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            ]
        }
        fn requires_confirmation(
            &self,
            name: &str,
            _params: &serde_json::Value,
        ) -> Option<ConfirmationRequest> {
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
        assert!(executor
            .requires_confirmation("a", &serde_json::json!({}))
            .is_none());
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
        assert!(executor
            .requires_confirmation("safe_op", &serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn acl_delegates_confirmation_for_allowed() {
        let inner: Arc<dyn ToolExecutor> = Arc::new(ConfirmableExecutor);
        let acl = AclToolExecutor::new(
            inner,
            vec!["delete_all".to_string(), "safe_op".to_string()],
        );
        assert!(acl
            .requires_confirmation("delete_all", &serde_json::json!({}))
            .is_some());
        assert!(acl
            .requires_confirmation("safe_op", &serde_json::json!({}))
            .is_none());
    }

    #[test]
    fn acl_blocks_confirmation_for_disallowed() {
        let inner: Arc<dyn ToolExecutor> = Arc::new(ConfirmableExecutor);
        let acl = AclToolExecutor::new(inner, vec!["safe_op".to_string()]);
        assert!(acl
            .requires_confirmation("delete_all", &serde_json::json!({}))
            .is_none());
    }
}
