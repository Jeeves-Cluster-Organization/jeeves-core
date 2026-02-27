//! Tool catalog — typed metadata, parameter validation, prompt generation.
//!
//! Owns tool *metadata* (not implementations — Python keeps the async callables).
//! Eliminates 54 untyped dict accesses and 12 stringly-typed dispatches from Python.

use crate::envelope::enums::{RiskSemantic, RiskSeverity, ToolCategory};
use crate::types::Error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// =============================================================================
// Parameter types
// =============================================================================

/// Parameter type for tool inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Int,
    Float,
    Bool,
    StringList,
    Enum(Vec<String>),
    Optional(Box<ParamType>),
}

impl ParamType {
    /// Validate a JSON value against this parameter type.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        match self {
            ParamType::String => {
                if value.is_string() {
                    Ok(())
                } else {
                    Err(format!("expected string, got {}", value_type_name(value)))
                }
            }
            ParamType::Int => {
                if value.is_i64() || value.is_u64() {
                    Ok(())
                } else {
                    Err(format!("expected integer, got {}", value_type_name(value)))
                }
            }
            ParamType::Float => {
                if value.is_number() {
                    Ok(())
                } else {
                    Err(format!("expected number, got {}", value_type_name(value)))
                }
            }
            ParamType::Bool => {
                if value.is_boolean() {
                    Ok(())
                } else {
                    Err(format!("expected boolean, got {}", value_type_name(value)))
                }
            }
            ParamType::StringList => {
                if let Some(arr) = value.as_array() {
                    for (i, item) in arr.iter().enumerate() {
                        if !item.is_string() {
                            return Err(format!(
                                "expected string at index {}, got {}",
                                i,
                                value_type_name(item)
                            ));
                        }
                    }
                    Ok(())
                } else {
                    Err(format!("expected array, got {}", value_type_name(value)))
                }
            }
            ParamType::Enum(variants) => {
                if let Some(s) = value.as_str() {
                    if variants.contains(&s.to_string()) {
                        Ok(())
                    } else {
                        Err(format!(
                            "invalid enum value '{}', expected one of: {}",
                            s,
                            variants.join(", ")
                        ))
                    }
                } else {
                    Err(format!("expected string for enum, got {}", value_type_name(value)))
                }
            }
            ParamType::Optional(inner) => {
                if value.is_null() {
                    Ok(())
                } else {
                    inner.validate(value)
                }
            }
        }
    }

    /// Human-readable type name for prompt generation.
    pub fn display_name(&self) -> String {
        match self {
            ParamType::String => "string".to_string(),
            ParamType::Int => "integer".to_string(),
            ParamType::Float => "number".to_string(),
            ParamType::Bool => "boolean".to_string(),
            ParamType::StringList => "string[]".to_string(),
            ParamType::Enum(variants) => format!("enum({})", variants.join("|")),
            ParamType::Optional(inner) => format!("{}?", inner.display_name()),
        }
    }
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// =============================================================================
// Parameter definition
// =============================================================================

/// A single parameter definition for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    pub param_type: ParamType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

impl ParamDef {
    pub fn is_required(&self) -> bool {
        self.default.is_none() && !matches!(self.param_type, ParamType::Optional(_))
    }
}

// =============================================================================
// Tool entry
// =============================================================================

/// Complete tool metadata entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub id: String,
    pub description: String,
    pub parameters: Vec<ParamDef>,
    pub category: ToolCategory,
    pub risk_semantic: RiskSemantic,
    pub risk_severity: RiskSeverity,
}

impl ToolEntry {
    /// Generate a prompt line for this tool.
    ///
    /// Format: `- tool_id(param1: type, param2?: type): description`
    pub fn to_prompt_line(&self) -> String {
        let params: Vec<String> = self
            .parameters
            .iter()
            .map(|p| {
                let optional = if p.is_required() { "" } else { "?" };
                format!("{}{}: {}", p.name, optional, p.param_type.display_name())
            })
            .collect();

        format!("- {}({}): {}", self.id, params.join(", "), self.description)
    }
}

// =============================================================================
// Tool catalog
// =============================================================================

/// In-memory tool catalog. Owns metadata, not implementations.
#[derive(Debug, Default)]
pub struct ToolCatalog {
    entries: HashMap<String, ToolEntry>,
}

impl ToolCatalog {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a tool entry.
    pub fn register(&mut self, entry: ToolEntry) -> crate::types::Result<()> {
        if entry.id.is_empty() {
            return Err(Error::validation("Tool id cannot be empty"));
        }
        self.entries.insert(entry.id.clone(), entry);
        Ok(())
    }

    /// Get a tool entry by id.
    pub fn get(&self, tool_id: &str) -> Option<&ToolEntry> {
        self.entries.get(tool_id)
    }

    /// Check if a tool exists.
    pub fn has_tool(&self, tool_id: &str) -> bool {
        self.entries.contains_key(tool_id)
    }

    /// List all tool ids.
    pub fn list_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.entries.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// List all tool entries.
    pub fn list_entries(&self) -> Vec<&ToolEntry> {
        let mut entries: Vec<&ToolEntry> = self.entries.values().collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries
    }

    /// Validate parameters against a tool's parameter definitions.
    ///
    /// Returns a list of validation errors (empty = valid).
    pub fn validate_params(
        &self,
        tool_id: &str,
        params: &Value,
    ) -> crate::types::Result<Vec<String>> {
        let entry = self
            .entries
            .get(tool_id)
            .ok_or_else(|| Error::not_found(format!("Unknown tool: {}", tool_id)))?;

        let param_map = params.as_object().ok_or_else(|| {
            Error::validation("Parameters must be a JSON object")
        })?;

        let mut errors = Vec::new();

        // Check required parameters are present
        for param_def in &entry.parameters {
            if param_def.is_required() && !param_map.contains_key(&param_def.name) {
                errors.push(format!("Missing required parameter: {}", param_def.name));
            }
        }

        // Build param name lookup for checking unknown params
        let known_names: HashMap<&str, &ParamDef> = entry
            .parameters
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();

        // Validate types of provided parameters
        for (key, value) in param_map {
            if let Some(param_def) = known_names.get(key.as_str()) {
                if let Err(e) = param_def.param_type.validate(value) {
                    errors.push(format!("Parameter '{}': {}", key, e));
                }
            } else {
                errors.push(format!("Unknown parameter: {}", key));
            }
        }

        Ok(errors)
    }

    /// Fill in default values for missing optional parameters.
    pub fn fill_defaults(&self, tool_id: &str, params: &mut Value) -> crate::types::Result<()> {
        let entry = self
            .entries
            .get(tool_id)
            .ok_or_else(|| Error::not_found(format!("Unknown tool: {}", tool_id)))?;

        if let Some(map) = params.as_object_mut() {
            for param_def in &entry.parameters {
                if !map.contains_key(&param_def.name) {
                    if let Some(default) = &param_def.default {
                        map.insert(param_def.name.clone(), default.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Generate formatted prompt section for LLM consumption.
    ///
    /// If `allowed_tools` is Some, only include those tools.
    pub fn generate_prompt(&self, allowed_tools: Option<&[String]>) -> String {
        let entries: Vec<&ToolEntry> = if let Some(allowed) = allowed_tools {
            allowed
                .iter()
                .filter_map(|id| self.entries.get(id))
                .collect()
        } else {
            self.list_entries()
        };

        if entries.is_empty() {
            return String::new();
        }

        let mut lines = Vec::with_capacity(entries.len() + 1);
        lines.push("Available tools:".to_string());
        for entry in entries {
            lines.push(entry.to_prompt_line());
        }
        lines.join("\n")
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> ToolEntry {
        ToolEntry {
            id: "search_web".to_string(),
            description: "Search the web for information".to_string(),
            parameters: vec![
                ParamDef {
                    name: "query".to_string(),
                    param_type: ParamType::String,
                    description: "Search query".to_string(),
                    default: None,
                },
                ParamDef {
                    name: "max_results".to_string(),
                    param_type: ParamType::Optional(Box::new(ParamType::Int)),
                    description: "Maximum results".to_string(),
                    default: Some(serde_json::json!(10)),
                },
            ],
            category: ToolCategory::Read,
            risk_semantic: RiskSemantic::ReadOnly,
            risk_severity: RiskSeverity::Low,
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        assert!(catalog.has_tool("search_web"));
        assert!(!catalog.has_tool("nonexistent"));
        assert_eq!(catalog.len(), 1);

        let entry = catalog.get("search_web").unwrap();
        assert_eq!(entry.description, "Search the web for information");
    }

    #[test]
    fn test_register_empty_id_fails() {
        let mut catalog = ToolCatalog::new();
        let mut entry = sample_entry();
        entry.id = String::new();
        assert!(catalog.register(entry).is_err());
    }

    #[test]
    fn test_validate_params_valid() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let params = serde_json::json!({"query": "rust programming"});
        let errors = catalog.validate_params("search_web", &params).unwrap();
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_validate_params_missing_required() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let params = serde_json::json!({});
        let errors = catalog.validate_params("search_web", &params).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Missing required parameter: query"));
    }

    #[test]
    fn test_validate_params_wrong_type() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let params = serde_json::json!({"query": 42});
        let errors = catalog.validate_params("search_web", &params).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("expected string"));
    }

    #[test]
    fn test_validate_params_unknown_param() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let params = serde_json::json!({"query": "test", "bogus": true});
        let errors = catalog.validate_params("search_web", &params).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Unknown parameter: bogus"));
    }

    #[test]
    fn test_validate_params_unknown_tool() {
        let catalog = ToolCatalog::new();
        let params = serde_json::json!({});
        assert!(catalog.validate_params("nonexistent", &params).is_err());
    }

    #[test]
    fn test_fill_defaults() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let mut params = serde_json::json!({"query": "test"});
        catalog.fill_defaults("search_web", &mut params).unwrap();

        assert_eq!(params["max_results"], 10);
    }

    #[test]
    fn test_fill_defaults_no_overwrite() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let mut params = serde_json::json!({"query": "test", "max_results": 5});
        catalog.fill_defaults("search_web", &mut params).unwrap();

        assert_eq!(params["max_results"], 5);
    }

    #[test]
    fn test_generate_prompt() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let prompt = catalog.generate_prompt(None);
        assert!(prompt.contains("Available tools:"));
        assert!(prompt.contains("search_web(query: string, max_results?: integer?): Search the web"));
    }

    #[test]
    fn test_generate_prompt_filtered() {
        let mut catalog = ToolCatalog::new();
        catalog.register(sample_entry()).unwrap();

        let prompt = catalog.generate_prompt(Some(&["nonexistent".to_string()]));
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_prompt_line_format() {
        let entry = sample_entry();
        let line = entry.to_prompt_line();
        assert_eq!(
            line,
            "- search_web(query: string, max_results?: integer?): Search the web for information"
        );
    }

    #[test]
    fn test_param_type_enum_validation() {
        let pt = ParamType::Enum(vec!["asc".to_string(), "desc".to_string()]);
        assert!(pt.validate(&serde_json::json!("asc")).is_ok());
        assert!(pt.validate(&serde_json::json!("bad")).is_err());
        assert!(pt.validate(&serde_json::json!(42)).is_err());
    }

    #[test]
    fn test_param_type_string_list_validation() {
        let pt = ParamType::StringList;
        assert!(pt.validate(&serde_json::json!(["a", "b"])).is_ok());
        assert!(pt.validate(&serde_json::json!([1, 2])).is_err());
        assert!(pt.validate(&serde_json::json!("not array")).is_err());
    }
}
