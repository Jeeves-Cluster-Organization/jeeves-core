//! Tool access control — agent-scoped tool permissions.
//!
//! Controls which agents can use which tools via compile-time-safe lookups.

use std::collections::{HashMap, HashSet};

use crate::types::{AgentName, ToolName};

/// Agent → tools access policy.
///
/// An agent can only execute tools it has been granted access to.
/// If no policy is set for an agent, it has access to nothing.
#[derive(Debug, Default)]
pub struct ToolAccessPolicy {
    grants: HashMap<AgentName, HashSet<ToolName>>,
}

impl ToolAccessPolicy {
    pub fn new() -> Self {
        Self {
            grants: HashMap::new(),
        }
    }

    /// Grant an agent access to a tool. Accepts anything coercible to
    /// `AgentName` / `ToolName` (string literals, `String`, or the newtypes).
    pub fn grant(&mut self, agent_name: impl Into<AgentName>, tool_id: impl Into<ToolName>) {
        self.grants.entry(agent_name.into()).or_default().insert(tool_id.into());
    }

    /// Grant an agent access to multiple tools at once.
    pub fn grant_many<I>(&mut self, agent_name: impl Into<AgentName>, tool_ids: I)
    where
        I: IntoIterator,
        I::Item: Into<ToolName>,
    {
        let set = self.grants.entry(agent_name.into()).or_default();
        for id in tool_ids {
            set.insert(id.into());
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
            .is_some_and(|set| set.contains(tool_id))
    }

    /// Get all tool ids an agent has access to (sorted).
    pub fn tools_for_agent(&self, agent_name: &str) -> Vec<ToolName> {
        self.grants
            .get(agent_name)
            .map(|set| {
                let mut ids: Vec<ToolName> = set.iter().cloned().collect();
                ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
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
        policy.grant("agent_a", "tool_x");
        policy.grant("agent_a", "tool_y");

        assert!(policy.check_access("agent_a", "tool_x"));
        assert!(policy.check_access("agent_a", "tool_y"));
        assert!(!policy.check_access("agent_a", "tool_z"));
        assert!(!policy.check_access("agent_b", "tool_x"));
    }

    #[test]
    fn test_grant_many() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant_many("agent_a", ["tool_x", "tool_y"]);

        assert!(policy.check_access("agent_a", "tool_x"));
        assert!(policy.check_access("agent_a", "tool_y"));
    }

    #[test]
    fn test_revoke() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("agent_a", "tool_x");
        policy.revoke("agent_a", "tool_x");

        assert!(!policy.check_access("agent_a", "tool_x"));
    }

    #[test]
    fn test_tools_for_agent() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("agent_a", "tool_b");
        policy.grant("agent_a", "tool_a");

        let tools = policy.tools_for_agent("agent_a");
        assert_eq!(
            tools.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
            vec!["tool_a", "tool_b"],
        );

        assert!(policy.tools_for_agent("unknown").is_empty());
    }

    #[test]
    fn test_clear_agent() {
        let mut policy = ToolAccessPolicy::new();
        policy.grant("agent_a", "tool_x");
        policy.clear_agent("agent_a");

        assert!(!policy.check_access("agent_a", "tool_x"));
    }
}
