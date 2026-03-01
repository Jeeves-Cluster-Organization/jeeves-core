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
) -> Option<String> {
    // 1. Error path
    if agent_failed {
        if let Some(ref error_next) = stage.error_next {
            return Some(error_next.clone());
        }
    }

    // 2. Routing rules (first match wins)
    for rule in &stage.routing {
        if evaluate_expr(&rule.expr, agent_outputs, agent_name, metadata, interrupt_response) {
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
) -> bool {
    match expr {
        RoutingExpr::Always => true,

        RoutingExpr::Eq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| v == *value)
        }

        RoutingExpr::Neq { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| v != *value)
        }

        RoutingExpr::Gt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? > value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lt { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? < value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Gte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? >= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Lte { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .and_then(|v| Some(v.as_f64()? <= value.as_f64()?))
                .unwrap_or(false)
        }

        RoutingExpr::Contains { field, value } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| {
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
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(false, |v| !v.is_null())
        }

        RoutingExpr::NotExists { field } => {
            resolve_field(field, agent_outputs, current_agent, metadata, interrupt_response)
                .map_or(true, |v| v.is_null())
        }

        RoutingExpr::And { exprs } => {
            exprs.iter().all(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response))
        }

        RoutingExpr::Or { exprs } => {
            exprs.iter().any(|e| evaluate_expr(e, agent_outputs, current_agent, metadata, interrupt_response))
        }

        RoutingExpr::Not { expr } => {
            !evaluate_expr(expr, agent_outputs, current_agent, metadata, interrupt_response)
        }
    }
}

/// Resolve a field reference to its value from agent outputs, metadata, or interrupt response.
pub fn resolve_field(
    field: &FieldRef,
    agent_outputs: &HashMap<String, HashMap<String, serde_json::Value>>,
    current_agent: &str,
    metadata: &HashMap<String, serde_json::Value>,
    interrupt_response: Option<&serde_json::Value>,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // =========================================================================
    // RoutingExpr unit tests (evaluate_expr directly)
    // =========================================================================

    #[test]
    fn test_evaluate_expr_neq() {
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();
        let metadata: HashMap<String, Value> = HashMap::new();

        // Field doesn't exist → Neq returns false (field must exist)
        let expr = RoutingExpr::Neq {
            field: FieldRef::Current { key: "k".to_string() },
            value: serde_json::json!("x"),
        };
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        // Field exists and differs → true
        let mut agent_out = HashMap::new();
        agent_out.insert("k".to_string(), serde_json::json!("y"));
        let mut outputs2 = HashMap::new();
        outputs2.insert("a1".to_string(), agent_out);
        assert!(evaluate_expr(&expr, &outputs2, "a1", &metadata, None));

        // Field exists and matches → false
        let mut agent_out2 = HashMap::new();
        agent_out2.insert("k".to_string(), serde_json::json!("x"));
        let mut outputs3 = HashMap::new();
        outputs3.insert("a1".to_string(), agent_out2);
        assert!(!evaluate_expr(&expr, &outputs3, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_gt_lt() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("score".to_string(), serde_json::json!(0.8));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let gt = RoutingExpr::Gt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(evaluate_expr(&gt, &outputs, "a1", &metadata, None));

        let lt = RoutingExpr::Lt {
            field: FieldRef::Current { key: "score".to_string() },
            value: serde_json::json!(0.5),
        };
        assert!(!evaluate_expr(&lt, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_contains_string() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("text".to_string(), serde_json::json!("hello world"));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let expr = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("world"),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        let expr2 = RoutingExpr::Contains {
            field: FieldRef::Current { key: "text".to_string() },
            value: serde_json::json!("missing"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_exists_not_exists() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("k".to_string(), serde_json::json!("v"));
        let mut outputs = HashMap::new();
        outputs.insert("a1".to_string(), agent_out);

        let exists = RoutingExpr::Exists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(evaluate_expr(&exists, &outputs, "a1", &metadata, None));

        let not_exists = RoutingExpr::NotExists { field: FieldRef::Current { key: "k".to_string() } };
        assert!(!evaluate_expr(&not_exists, &outputs, "a1", &metadata, None));

        let exists_missing = RoutingExpr::Exists { field: FieldRef::Current { key: "nope".to_string() } };
        assert!(!evaluate_expr(&exists_missing, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_and_or_not() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let t = RoutingExpr::Always;
        let f = RoutingExpr::Not { expr: Box::new(RoutingExpr::Always) };

        let and_tt = RoutingExpr::And { exprs: vec![t.clone(), t.clone()] };
        assert!(evaluate_expr(&and_tt, &outputs, "a1", &metadata, None));

        let and_tf = RoutingExpr::And { exprs: vec![t.clone(), f.clone()] };
        assert!(!evaluate_expr(&and_tf, &outputs, "a1", &metadata, None));

        let or_tf = RoutingExpr::Or { exprs: vec![t.clone(), f.clone()] };
        assert!(evaluate_expr(&or_tf, &outputs, "a1", &metadata, None));

        let or_ff = RoutingExpr::Or { exprs: vec![f.clone(), f.clone()] };
        assert!(!evaluate_expr(&or_ff, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_agent() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let mut agent_out = HashMap::new();
        agent_out.insert("topic".to_string(), serde_json::json!("time"));
        let mut outputs = HashMap::new();
        outputs.insert("understand".to_string(), agent_out);

        let expr = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(evaluate_expr(&expr, &outputs, "current", &metadata, None));

        // Agent hasn't run → field doesn't exist → Eq returns false
        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Agent { agent: "missing_agent".to_string(), key: "topic".to_string() },
            value: serde_json::json!("time"),
        };
        assert!(!evaluate_expr(&expr2, &outputs, "current", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_meta_dot_notation() {
        let mut metadata: HashMap<String, Value> = HashMap::new();
        metadata.insert("session_context".to_string(), serde_json::json!({
            "has_history": true,
            "nested": { "deep": 42 }
        }));
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let expr = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.has_history".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, None));

        let expr2 = RoutingExpr::Eq {
            field: FieldRef::Meta { path: "session_context.nested.deep".to_string() },
            value: serde_json::json!(42),
        };
        assert!(evaluate_expr(&expr2, &outputs, "a1", &metadata, None));

        // Missing path
        let expr3 = RoutingExpr::Exists {
            field: FieldRef::Meta { path: "session_context.nonexistent".to_string() },
        };
        assert!(!evaluate_expr(&expr3, &outputs, "a1", &metadata, None));
    }

    #[test]
    fn test_evaluate_expr_field_ref_interrupt() {
        let metadata: HashMap<String, Value> = HashMap::new();
        let outputs: HashMap<String, HashMap<String, Value>> = HashMap::new();

        let interrupt_val = serde_json::json!({
            "approved": true,
            "text": "yes go ahead",
        });

        let expr = RoutingExpr::Eq {
            field: FieldRef::Interrupt { key: "approved".to_string() },
            value: serde_json::json!(true),
        };
        assert!(evaluate_expr(&expr, &outputs, "a1", &metadata, Some(&interrupt_val)));

        // No interrupt response → field doesn't exist
        assert!(!evaluate_expr(&expr, &outputs, "a1", &metadata, None));
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

    #[test]
    fn test_field_ref_serde_roundtrip() {
        let variants: Vec<FieldRef> = vec![
            FieldRef::Current { key: "intent".to_string() },
            FieldRef::Agent { agent: "understand".to_string(), key: "topic".to_string() },
            FieldRef::Meta { path: "session.has_history".to_string() },
            FieldRef::Interrupt { key: "approved".to_string() },
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
