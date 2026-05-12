//! Prompt template loading and rendering.
//!
//! Templates are `.txt` files with `{var}` placeholders. Missing vars
//! resolve to empty string.

use std::collections::HashMap;
use std::path::PathBuf;

/// Prompt template registry — loads from a directory.
#[derive(Debug, Clone)]
pub struct PromptRegistry {
    /// Base directory for prompt files.
    base_dir: Option<PathBuf>,
    /// In-memory overrides (for testing or embedded prompts).
    overrides: HashMap<String, String>,
}

impl PromptRegistry {
    /// Create a registry backed by a directory of `.txt` files.
    pub fn from_dir(dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: Some(dir.into()),
            overrides: HashMap::new(),
        }
    }

    /// Create an empty registry (in-memory only).
    pub fn empty() -> Self {
        Self {
            base_dir: None,
            overrides: HashMap::new(),
        }
    }

    /// Register an in-memory prompt template.
    pub fn insert(&mut self, key: impl Into<String>, template: impl Into<String>) {
        self.overrides.insert(key.into(), template.into());
    }

    /// Load a prompt template by key.
    pub fn load(&self, key: &str) -> Option<String> {
        // Check overrides first
        if let Some(t) = self.overrides.get(key) {
            return Some(t.clone());
        }
        // Try filesystem
        if let Some(ref base) = self.base_dir {
            let path = base.join(format!("{}.txt", key));
            if path.exists() {
                return std::fs::read_to_string(&path).ok();
            }
        }
        None
    }

    /// Render a template with variable substitution.
    pub fn render(&self, key: &str, vars: &HashMap<String, String>) -> Option<String> {
        let template = self.load(key)?;
        Some(render_template(&template, vars))
    }
}

/// Simple `{var}` substitution. Missing vars → empty string.
pub fn render_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    // Strip remaining unreplaced vars
    strip_unresolved_vars(&result)
}

fn strip_unresolved_vars(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            // Look for closing brace
            let mut var = String::new();
            let mut found_close = false;
            for inner in chars.by_ref() {
                if inner == '}' {
                    found_close = true;
                    break;
                }
                var.push(inner);
            }
            if !found_close {
                // Not a valid var reference — emit as-is
                result.push('{');
                result.push_str(&var);
            } else if is_identifier(&var) {
                // Valid identifier-like var reference — strip (emit empty)
            } else {
                // Not an identifier (e.g. JSON like {"key": "value"}) — preserve
                result.push('{');
                result.push_str(&var);
                result.push('}');
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a string looks like a template variable name (alphanumeric + underscore).
fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_basic() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());
        vars.insert("topic".to_string(), "Rust".to_string());
        let result = render_template("Hello {name}, let's discuss {topic}.", &vars);
        assert_eq!(result, "Hello Alice, let's discuss Rust.");
    }

    #[test]
    fn test_render_template_missing_vars() {
        let vars = HashMap::new();
        let result = render_template("Hello {name}!", &vars);
        assert_eq!(result, "Hello !");
    }

    #[test]
    fn test_prompt_registry_override() {
        let mut reg = PromptRegistry::empty();
        reg.insert("greet", "Hello {user}!");
        let mut vars = HashMap::new();
        vars.insert("user".to_string(), "World".to_string());
        let rendered = reg.render("greet", &vars).unwrap();
        assert_eq!(rendered, "Hello World!");
    }

    #[test]
    fn test_prompt_registry_missing_key() {
        let reg = PromptRegistry::empty();
        assert!(reg.load("nonexistent").is_none());
    }

    #[test]
    fn test_render_preserves_json_examples() {
        let mut vars = HashMap::new();
        vars.insert("user_message".to_string(), "Hello".to_string());
        let template = r#"User: {user_message}
Example output:
{"intent": "general", "topic": "greeting"}"#;
        let result = render_template(template, &vars);
        assert_eq!(result, r#"User: Hello
Example output:
{"intent": "general", "topic": "greeting"}"#);
    }

    #[test]
    fn test_strip_only_identifiers() {
        let vars = HashMap::new();
        // Identifier vars get stripped
        let result = render_template("Hello {name}!", &vars);
        assert_eq!(result, "Hello !");
        // JSON-like braces preserved
        let result = render_template(r#"{"key": "value"}"#, &vars);
        assert_eq!(result, r#"{"key": "value"}"#);
    }
}
