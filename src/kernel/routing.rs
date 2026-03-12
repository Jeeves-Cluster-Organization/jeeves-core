//! Routing expression evaluation for pipeline orchestration.
//!
//! Pure functions that evaluate routing expression trees against agent outputs
//! and metadata. Borrow-checker friendly — no mutable state required.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Routing Types
// =============================================================================

/// Routing rule: expression tree + target stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub expr: RoutingExpr,
    pub target: String,
}

/// Routing expression tree evaluated recursively by the kernel.
///
/// Serde: internally tagged with "op" field.
/// JSON example: `{"op": "Eq", "field": {"scope": "Current", "key": "intent"}, "value": "greet"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum RoutingExpr {
    /// Equality: field == value
    Eq { field: FieldRef, value: serde_json::Value },
    /// Inequality: field != value
    Neq { field: FieldRef, value: serde_json::Value },
    /// Greater than (numeric f64 comparison)
    Gt { field: FieldRef, value: serde_json::Value },
    /// Less than (numeric f64 comparison)
    Lt { field: FieldRef, value: serde_json::Value },
    /// Greater than or equal (numeric f64 comparison)
    Gte { field: FieldRef, value: serde_json::Value },
    /// Less than or equal (numeric f64 comparison)
    Lte { field: FieldRef, value: serde_json::Value },
    /// String contains substring, or array contains element
    Contains { field: FieldRef, value: serde_json::Value },
    /// Field exists and is not null
    Exists { field: FieldRef },
    /// Field is absent or null
    NotExists { field: FieldRef },
    /// All sub-expressions must be true
    And { exprs: Vec<RoutingExpr> },
    /// At least one sub-expression must be true
    Or { exprs: Vec<RoutingExpr> },
    /// Negate a sub-expression
    Not { expr: Box<RoutingExpr> },
    /// Unconditional match (always true)
    Always,
}

/// Scoped field reference for routing expressions.
///
/// Serde: internally tagged with "scope" field.
/// JSON example: `{"scope": "Agent", "agent": "understand", "key": "topic"}`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "scope")]
pub enum FieldRef {
    /// output[current_agent][key]
    Current { key: String },
    /// output[agent][key] — cross-agent reference
    Agent { agent: String, key: String },
    /// envelope.audit.metadata with dot-notation traversal
    Meta { path: String },
    /// interrupt_response[key] — serialized InterruptResponse
    Interrupt { key: String },
    /// envelope.state[key] — merged accumulator state
    State { key: String },
}

// =============================================================================
// Evaluation Functions
// =============================================================================

/// Evaluate routing for a stage (Temporal/K8s pattern).
///
/// Evaluation order:
/// 1. If agent failed AND error_next set → route to error_next
/// 2. Evaluate routing rules (first match wins)
/// 3. If no match AND default_next set → route to default_next
/// 4. If no match AND no default_next → None (caller terminates with COMPLETED)
pub fn evaluate_routing(
    stage: &super::orchestrator_types::PipelineStage,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    agent_name: &str,
    agent_failed: bool,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
    state: &HashMap<String, serde_json::Value>,
) -> Option<String> {
    // 1. Error path
    if agent_failed {
        if let Some(ref error_next) = stage.error_next {
            return Some(error_next.clone());
        }
    }

    // 2. Routing rules (first match wins)
    for rule in &stage.routing {
        if evaluate_expr(&rule.expr, agent_outputs, agent_name, metadata, interrupt_response, state) {
            return Some(rule.target.clone());
        }
    }

    // 3. Default fallback
    if let Some(ref default_next) = stage.default_next {
        return Some(default_next.clone());
    }

    // 4. No match — Temporal pattern: kernel terminates
    None
}

/// Recursively evaluate a routing expression against agent outputs and metadata.
pub fn evaluate_expr(
    expr: &RoutingExpr,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    current_agent: &str,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
    state: &HashMap<String, serde_json::Value>,
) -> bool {
    match expr {
        RoutingExpr::Always => true,

        RoutingExpr::Eq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .is_some_and(|v| v == *value)
        }

        RoutingExpr::Neq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .is_some_and(|v| v != *value)
        }

        RoutingExpr::Gt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .and_then(|v| Some(v.as_f64()? > value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .and_then(|v| Some(v.as_f64()? < value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Gte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .and_then(|v| Some(v.as_f64()? >= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .and_then(|v| Some(v.as_f64()? <= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Contains { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .is_some_and(|v| {
                    // String contains substring
                    if let (Some(s), Some(substr)) = (v.as_str(), value.as_str()) {
                        return s.contains(substr);
                    }
                    // Array contains element
                    if let Some(arr) = v.as_array() {
                        return arr.contains(value);
                    }
                    false
                })
        }

        RoutingExpr::Exists { field } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .is_some_and(|v| !v.is_null())
        }

        RoutingExpr::NotExists { field } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response, state)
                .map_or(true, |v| v.is_null())
        }

        RoutingExpr::And { exprs } => {
            exprs.iter().all(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response, state))
        }

        RoutingExpr::Or { exprs } => {
            exprs.iter().any(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response, state))
        }

        RoutingExpr::Not { expr } => {
            !evaluate_expr(expr, agent_outputs, current_agent, metadata, interrupt_response, state)
        }
    }
}

/// Resolve a field reference to its value from agent outputs, metadata, interrupt response, or state.
pub fn resolve_field(
    field: &FieldRef,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    current_agent: &str,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
    state: &HashMap<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    match field {
        FieldRef::Current { key } => {
            agent_outputs.get(current_agent)?.get(key).cloned()
        }
        FieldRef::Agent { agent, key } => {
            agent_outputs.get(agent.as_str())?.get(key).cloned()
        }
        FieldRef::Meta { path } => {
            // Dot-notation traversal: "session_context.has_history"
            let parts: Vec<&str> = path.split('.').collect();
            if parts.is_empty() {
                return None;
            }
            let first = metadata.get(parts[0])?;
            let mut current = first.clone();
            for part in &parts[1..] {
                current = current.get(part)?.clone();
            }
            Some(current)
        }
        FieldRef::Interrupt { key } => {
            interrupt_response?.get(key).cloned()
        }
        FieldRef::State { key } => {
            state.get(key).cloned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::orchestrator_types::{JoinStrategy, PipelineStage};
    use serde_json::Value;

    // ── Test helpers ─────────────────────────────────────────────────────

    /// Empty outputs and metadata — the common case for routing tests.
    fn empty_ctx() -> (HashMap<String, HashMap<String, Value>>, HashMap<String, Value>) {
        (HashMap::new(), HashMap::new())
    }

    /// Build an outputs map with a single agent's key-value pairs.
    fn outputs_with(agent: &str, kvs: &[(&str, Value)]) -> HashMap<String, HashMap<String, Value>> {
        let mut agent_out = HashMap::new();
        for (k, v) in kvs {
            agent_out.insert(k.to_string(), v.clone());
        }
        let mut outputs = HashMap::new();
        outputs.insert(agent.to_string(), agent_out);
        outputs
    }

    // =========================================================================
    // RoutingExpr unit tests (evaluate_expr directly)
    // =========================================================================

    #[test]
    fn test_evaluate_expr_neq() {
        let (outputs, metadata) = empty_ctx();

        // Field doesn't exist → Neq returns false (field must exist)
        let expr = RoutingExpr::Neq {
            field: FieldRef::Current { key: "k".to_string() },
            value: serde_json::json!("x"),
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));

        // Field exists and differs → true
        let outputs2 = outputs_with("a1", &[("k", serde_json::json!("y"))]);
        assert!(evaluate_expr(&expr, &outputs2, "a1", &metadata, None, &HashMap::new()));

        // Field exists and matches → false
        let outputs3 = outputs_with("a1", &[("k", serde_json::json!("x"))]);
        assert!(!evaluate_expr(&expr, &outputs3, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_gt_lt() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("score", serde_json::json!(0.8))]);

        let gt = RoutingExpr::Gt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(evaluate_expr(&gt, &outputs, "a1", &metadata, None, &HashMap::new()));

        let lt = RoutingExpr::Lt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(!evaluate_expr(&lt, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_contains_string() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("text", serde_json::json!("hello world"))]);

        let expr = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("world"),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));

        let expr2 = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("missing"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_exists_not_exists() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("k", serde_json::json!("v"))]);

        let exists = RoutingExpr::Exists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(evaluate_expr(&exists, &outputs, "a1", &metadata, None, &HashMap::new()));

        let not_exists = RoutingExpr::NotExists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(!evaluate_expr(&not_exists, &outputs, "a1", &metadata, None, &HashMap::new()));

        let exists_missing = RoutingExpr::Exists { field: FieldRef::Current { key: "nope".to_string() } };
        assert!(!evaluate_expr(&exists_missing, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_and_or_not() {
        let (outputs, metadata) = empty_ctx();

        let t = RoutingExpr::Always;
        let f = RoutingExpr::Not { expr: Box::new(RoutingExpr::Always) };

        let and_tt = RoutingExpr::And { exprs: vec![t.clone(), t.clone()] };
        assert!(evaluate_expr(&and_tt, &outputs, "a1", &metadata, None, &HashMap::new()));

        let and_tf = RoutingExpr::And { exprs: vec![t.clone(), f.clone()] };
        assert!(!evaluate_expr(&and_tf, &outputs, "a1", &metadata, None, &HashMap::new()));

        let or_tf = RoutingExpr::Or { exprs: vec![t.clone(), f.clone()] };
        assert!(evaluate_expr(&or_tf, &outputs, "a1", &metadata, None, &HashMap::new()));

        let or_ff = RoutingExpr::Or { exprs: vec![f.clone(), f.clone()] };
        assert!(!evaluate_expr(&or_ff, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_field_ref_agent() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("understand", &[("topic", serde_json::json!("time"))]);

        let expr = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(evaluate_expr(&expr, &outputs, "current", &metadata, None, &HashMap::new()));

        // Agent hasn't run → field doesn't exist → Eq returns false
        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "missing_agent".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "current", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_field_ref_meta_dot_notation() {
        let (outputs, mut metadata) = empty_ctx();
        metadata.insert("session_context".to_string(), serde_json::json!({
            "has_history": true,
            "nested": { "deep": 42 }
        }));

        let expr = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.has_history".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));

        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.nested.deep".to_string() },
            value: serde_json::json!(42),
        };
        assert!(evaluate_expr(&expr2, &outputs, "a1", &metadata, None, &HashMap::new()));

        // Missing path
        let expr3 = RoutingExpr::Exists {
            field: FieldRef::Meta { path: "session_context.nonexistent".to_string() },
        };
        assert!(!evaluate_expr(&expr3, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_expr_field_ref_interrupt() {
        let (outputs, metadata) = empty_ctx();

        let interrupt_val = serde_json::json!({
            "approved": true,
            "text": "yes go ahead",
        });

        let expr = RoutingExpr::Eq {
            field: FieldRef::Interrupt { key: "approved".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, Some(&interrupt_val), &HashMap::new()));

        // No interrupt response → field doesn't exist
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    // =========================================================================
    // Serde round-trip tests
    // =========================================================================

    #[test]
    fn test_routing_rule_serde_roundtrip() {
        let rule = RoutingRule {
            expr: RoutingExpr::And {
                exprs: vec![
                    RoutingExpr::Or {
                        exprs: vec![
                            RoutingExpr::Eq {
                                field: FieldRef::Current { key: "intent".to_string() },
                                value: serde_json::json!("greet"),
                            },
                            RoutingExpr::Neq {
                                field: FieldRef::Agent { agent: "other".to_string(), key: "status".to_string() },
                                value: serde_json::json!("done"),
                            },
                        ],
                    },
                    RoutingExpr::Not {
                        expr: Box::new(RoutingExpr::Exists {
                            field: FieldRef::Meta { path: "skip".to_string() },
                        }),
                    },
                ],
            },
            target: "next_stage".to_string(),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: RoutingRule = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();
        let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
        assert_eq!(v1, v2);
    }

    // =========================================================================
    // Edge case tests (Phase 1b audit)
    // =========================================================================

    #[test]
    fn test_gt_lt_with_non_numeric_string_returns_false() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("val", serde_json::json!("not_a_number"))]);

        let gt = RoutingExpr::Gt {
            field: FieldRef::Current { key: "val".to_string() },
            value: serde_json::json!(5),
        };
        assert!(!evaluate_expr(&gt, &outputs, "a1", &metadata, None, &HashMap::new()));

        let lt = RoutingExpr::Lt {
            field: FieldRef::Current { key: "val".to_string() },
            value: serde_json::json!(5),
        };
        assert!(!evaluate_expr(&lt, &outputs, "a1", &metadata, None, &HashMap::new()));

        let gte = RoutingExpr::Gte {
            field: FieldRef::Current { key: "val".to_string() },
            value: serde_json::json!(5),
        };
        assert!(!evaluate_expr(&gte, &outputs, "a1", &metadata, None, &HashMap::new()));

        let lte = RoutingExpr::Lte {
            field: FieldRef::Current { key: "val".to_string() },
            value: serde_json::json!(5),
        };
        assert!(!evaluate_expr(&lte, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_contains_on_json_array_element_membership() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("tags", serde_json::json!(["a", "b", "c"]))]);

        let contains = RoutingExpr::Contains {
            field: FieldRef::Current { key: "tags".to_string() },
            value: serde_json::json!("b"),
        };
        assert!(evaluate_expr(&contains, &outputs, "a1", &metadata, None, &HashMap::new()));

        let not_contains = RoutingExpr::Contains {
            field: FieldRef::Current { key: "tags".to_string() },
            value: serde_json::json!("z"),
        };
        assert!(!evaluate_expr(&not_contains, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_contains_on_non_string_non_array_returns_false() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("count", serde_json::json!(42))]);

        let contains = RoutingExpr::Contains {
            field: FieldRef::Current { key: "count".to_string() },
            value: serde_json::json!(4),
        };
        assert!(!evaluate_expr(&contains, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_deeply_nested_meta_with_missing_intermediate() {
        let (outputs, mut metadata) = empty_ctx();
        metadata.insert("a".to_string(), serde_json::json!({"b": {"c": 99}}));

        // Valid deep path
        let expr = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "a.b.c".to_string() },
            value: serde_json::json!(99),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));

        // Missing intermediate "x" → resolves to None, Exists returns false
        let missing = RoutingExpr::Exists {
            field: FieldRef::Meta { path: "a.x.c".to_string() },
        };
        assert!(!evaluate_expr(&missing, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_field_ref_agent_nonexistent_agent_resolves_to_null() {
        let (outputs, metadata) = empty_ctx();

        // Nonexistent agent → resolve returns None → Exists returns false
        let expr = RoutingExpr::Exists {
            field: FieldRef::Agent { agent: "ghost".to_string(), key: "k".to_string() },
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));

        // NotExists on nonexistent agent → true
        let not_exists = RoutingExpr::NotExists {
            field: FieldRef::Agent { agent: "ghost".to_string(), key: "k".to_string() },
        };
        assert!(evaluate_expr(&not_exists, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_eq_type_mismatch_string_vs_number() {
        let (_, metadata) = empty_ctx();
        let outputs = outputs_with("a1", &[("val", serde_json::json!("5"))]);

        // String "5" != number 5 → no match
        let expr = RoutingExpr::Eq {
            field: FieldRef::Current { key: "val".to_string() },
            value: serde_json::json!(5),
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_neq_missing_field_returns_false() {
        // Neq requires field to exist — missing field → false (not true)
        let (outputs, metadata) = empty_ctx();

        let expr = RoutingExpr::Neq {
            field: FieldRef::Current { key: "missing".to_string() },
            value: serde_json::json!("anything"),
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None, &HashMap::new()));
    }

    #[test]
    fn test_evaluate_routing_error_next_on_failure() {
        let stage = PipelineStage {
            name: "s1".to_string(),
            agent: "a1".to_string(),
            routing: vec![],
            default_next: Some("s2".to_string()),
            error_next: Some("s_err".to_string()),
            max_visits: None,
            join_strategy: JoinStrategy::default(),
            output_schema: None,
            allowed_tools: None,
            node_kind: crate::kernel::orchestrator_types::NodeKind::default(),
            output_key: None,
        };
        let (outputs, metadata) = empty_ctx();

        // agent_failed=true → routes to error_next, not default_next
        let result = evaluate_routing(&stage, &outputs, "a1", true, &metadata, None, &HashMap::new());
        assert_eq!(result, Some("s_err".to_string()));

        // agent_failed=false → falls through to default_next
        let result = evaluate_routing(&stage, &outputs, "a1", false, &metadata, None, &HashMap::new());
        assert_eq!(result, Some("s2".to_string()));
    }

    #[test]
    fn test_field_ref_serde_roundtrip() {
        let variants: Vec<FieldRef> = vec![
            FieldRef::Current { key: "intent".to_string() },
            FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            FieldRef::Meta { path: "session.has_history".to_string() },
            FieldRef::Interrupt { key: "approved".to_string() },
            FieldRef::State { key: "accumulated".to_string() },
        ];
        for field_ref in variants {
            let json = serde_json::to_string(&field_ref).unwrap();
            let deserialized: FieldRef = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
            let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
            assert_eq!(v1, v2, "FieldRef round-trip failed for: {}", json);
        }
    }
}
