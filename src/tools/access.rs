//! Tool access control — agent-scoped tool permissions.
//!
//! Controls which agents can use which tools. Replaces Python's
//! stringly-typed access checks with compile-time-safe lookups.

use std::collections::{HashMap, HashSet};

/// Agent → tools access policy.
///
/// An agent can only execute tools it has been granted access to.
/// If no policy is set for an agent, it has access to nothing.
#[derive(Debug, Default)]
pub struct ToolAccessPolicy {
    /// agent_name → set of allowed tool_ids
    grants: HashMap<String, HashSet<String>>,
}

impl ToolAccessPolicy {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    /// Grant an agent access to a tool.
    pub fn grant(&mut self, agent_name: &str, tool_id: &str) {
        self.grants
            .entry(agent_name.to_string())
            .or_default()
            .insert(tool_id.to_string());
    }

    /// Grant an agent access to multiple tools at once.
    pub fn grant_many(&mut self, agent_name: &str, tool_ids: &[String]) {
        let set = self.grants.entry(agent_name.to_string()).or_default();
        for id in tool_ids {
            set.insert(id.clone());
        }
    }

    /// Revoke an agent's access to a tool.
    pub fn revoke(&mut self, agent_name: &str, tool_id: &str) {
        if let Some(set) = self.grants.get_mut(agent_name) {
            set.remove(tool_id);
        }
    }

    /// Check if an agent has access to a tool.
    pub fn check_access(&self, agent_name: &str, tool_id: &str) -> bool {
        self.grants
            .get(agent_name)
            .map_or(false, |set| set.contains(tool_id))
    }

    /// Get all tool ids an agent has access to.
    pub fn tools_for_agent(&self, agent_name: &str) -> Vec<String> {
        self.grants
            .get(agent_name)
            .map(|set| {
                let mut ids: Vec<String> = set.iter().cloned().collect();
                ids.sort();
                ids
            })
            .unwrap_or_default()
    }

    /// Clear all grants for an agent.
    pub fn clear_agent(&mut self, agent_name: &str) {
        self.grants.remove(agent_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_and_check() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("reporter", "search_web");
        policy.grant("reporter", "interview_npc");

        assert!(policy.check_access("reporter", "search_web"));
        assert!(policy.check_access("reporter", "interview_npc"));
        assert!(!policy.check_access("reporter", "delete_data"));
        assert!(!policy.check_access("editor", "search_web"));
    }

    #[test]
    fn test_grant_many() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant_many(
            "reporter",
            &["search_web".to_string(), "interview_npc".to_string()],
        );

        assert!(policy.check_access("reporter", "search_web"));
        assert!(policy.check_access("reporter", "interview_npc"));
    }

    #[test]
    fn test_revoke() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("reporter", "search_web");
        policy.revoke("reporter", "search_web");

        assert!(!policy.check_access("reporter", "search_web"));
    }

    #[test]
    fn test_tools_for_agent() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("reporter", "b_tool");
        policy.grant("reporter", "a_tool");

        let tools = policy.tools_for_agent("reporter");
        assert_eq!(tools, vec!["a_tool", "b_tool"]); // sorted

        assert!(policy.tools_for_agent("unknown").is_empty());
    }

    #[test]
    fn test_clear_agent() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("reporter", "search_web");
        policy.clear_agent("reporter");

        assert!(!policy.check_access("reporter", "search_web"));
    }
}
