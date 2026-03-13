//! AgentCard — discovery metadata for agents and pipelines.
//!
//! Used by CommBus federation for agent discovery (ListAgentCards).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata card describing an agent or pipeline's capabilities.
///
/// Registered with the kernel for federation discovery. Other pipelines
/// can query available agents via ListAgentCards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_event_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub published_event_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
}

/// Registry of agent cards for discovery.
#[derive(Debug, Default)]
pub struct AgentCardRegistry {
    cards: HashMap<String, AgentCard>,
}

impl AgentCardRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an agent card. Overwrites if name already exists.
    pub fn register(&mut self, card: AgentCard) {
        self.cards.insert(card.name.clone(), card);
    }

    /// List all agent cards, optionally filtered by a predicate.
    pub fn list(&self, filter: Option<&str>) -> Vec<&AgentCard> {
        match filter {
            Some(f) => self.cards.values()
                .filter(|c| c.name.contains(f) || c.capabilities.iter().any(|cap| cap.contains(f)))
                .collect(),
            None => self.cards.values().collect(),
        }
    }

    /// Get a specific agent card by name.
    pub fn get(&self, name: &str) -> Option<&AgentCard> {
        self.cards.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_list() {
        let mut registry = AgentCardRegistry::new();
        registry.register(AgentCard {
            name: "assistant".to_string(),
            description: "Personal assistant".to_string(),
            pipeline_name: Some("assistant_pipeline".to_string()),
            capabilities: vec!["chat".to_string(), "search".to_string()],
            accepted_event_types: vec![],
            published_event_types: vec![],
            input_schema: None,
            output_schema: None,
        });

        assert_eq!(registry.list(None).len(), 1);
        assert_eq!(registry.list(Some("chat")).len(), 1);
        assert_eq!(registry.list(Some("nonexistent")).len(), 0);
        assert!(registry.get("assistant").is_some());
        assert!(registry.get("missing").is_none());
    }
}
