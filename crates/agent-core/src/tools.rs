use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub data: serde_json::Value,
    pub content: Vec<ContentPart>,
}

impl ToolOutput {
    pub fn json(data: serde_json::Value) -> Self {
        Self {
            data,
            content: vec![],
        }
    }

    pub fn text(s: impl Into<String>) -> Self {
        let s = s.into();
        Self {
            data: serde_json::Value::String(s.clone()),
            content: vec![ContentPart::Text { text: s }],
        }
    }

    pub fn with_content(data: serde_json::Value, content: Vec<ContentPart>) -> Self {
        Self { data, content }
    }

    pub fn as_llm_text(&self) -> String {
        if self.content.is_empty() {
            self.data.to_string()
        } else {
            self.content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Ref {
        content_type: String,
        ref_id: String,
    },
    Blob {
        content_type: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
}

mod base64_bytes {
    use base64::Engine;
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], ser: S) -> std::result::Result<S::Ok, S::Error> {
        ser.serialize_str(&base64::engine::general_purpose::STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> std::result::Result<Vec<u8>, D::Error> {
        let s = String::deserialize(de)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(D::Error::custom)
    }
}

pub trait ContentResolver: Send + Sync + std::fmt::Debug {
    fn resolve(&self, ref_id: &str, content_type: &str) -> Option<Vec<u8>>;
}

#[async_trait]
pub trait Tool: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    /// JSON Schema for parameters.
    fn schema(&self) -> serde_json::Value;

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput>;
}

pub type DynTool = Arc<dyn Tool>;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}
