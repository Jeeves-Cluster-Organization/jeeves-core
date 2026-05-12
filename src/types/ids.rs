//! Strongly-typed identifiers. All IDs validate at construction.
//!
//! Two macro forms:
//! - `define_id!(Name, uuid)` — also provides `new()` (UUID v4) and `Default`.
//! - `define_id!(Name)` — no auto-generated form; must always be constructed
//!   from a non-empty string via `must()` or `from_string()`.
//!
//! Every ID implements `AsRef<str>` and `Borrow<str>` so it works as a
//! `HashMap` key looked up by `&str` without an allocation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;
use std::fmt;

macro_rules! define_id {
    ($name:ident, uuid) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4().to_string())
            }

            pub fn from_string(s: String) -> Result<Self, &'static str> {
                if s.is_empty() {
                    return Err(concat!(stringify!($name), " cannot be empty"));
                }
                Ok(Self(s))
            }

            /// Panics if empty. Use only with known-good values.
            pub fn must(s: impl Into<String>) -> Self {
                let s = s.into();
                assert!(!s.is_empty(), concat!(stringify!($name), " cannot be empty"));
                Self(s)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::must(s)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self::must(s)
            }
        }
    };
    ($name:ident) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn from_string(s: String) -> Result<Self, &'static str> {
                if s.is_empty() {
                    return Err(concat!(stringify!($name), " cannot be empty"));
                }
                Ok(Self(s))
            }

            /// Panics if empty. Use only with known-good values.
            pub fn must(s: impl Into<String>) -> Self {
                let s = s.into();
                assert!(!s.is_empty(), concat!(stringify!($name), " cannot be empty"));
                Self(s)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::must(s)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self::must(s)
            }
        }
    };
}

// Run-instance identifiers (UUID-generated).
define_id!(RunId, uuid);
define_id!(EnvelopeId, uuid);
define_id!(RequestId, uuid);
define_id!(SessionId, uuid);

// Caller-supplied identifiers.
define_id!(UserId);

// Workflow / agent / tool identifiers (caller-supplied, must be non-empty).
define_id!(StageName);
define_id!(AgentName);
define_id!(ToolName);
define_id!(PromptKey);
define_id!(OutputKey);
define_id!(RoutingFnName);
define_id!(InterruptId);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn must_constructs_from_non_empty() {
        let s = StageName::must("analyze");
        assert_eq!(s.as_str(), "analyze");
        assert_eq!(format!("{s}"), "analyze");
    }

    #[test]
    #[should_panic]
    fn must_panics_on_empty() {
        let _ = AgentName::must("");
    }

    #[test]
    fn from_string_rejects_empty() {
        assert!(ToolName::from_string(String::new()).is_err());
        assert!(ToolName::from_string("ok".into()).is_ok());
    }

    #[test]
    fn hashmap_lookup_by_str_uses_borrow_impl() {
        // Borrow<str> means HashMap<StageName, _> looks up by &str
        // without first constructing a temporary StageName.
        let mut m: HashMap<StageName, i32> = HashMap::new();
        m.insert(StageName::must("classify_intent"), 1);
        assert_eq!(m.get("classify_intent"), Some(&1));
        assert_eq!(m.get("nope"), None);
    }

    #[test]
    fn serde_round_trips_as_bare_string() {
        let s = PromptKey::must("dialogue.plan");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#""dialogue.plan""#);
        let back: PromptKey = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn process_id_default_is_unique() {
        let a = RunId::default();
        let b = RunId::default();
        assert_ne!(a, b);
    }

    #[test]
    fn as_ref_str_works() {
        let n = RoutingFnName::must("router");
        let s: &str = n.as_ref();
        assert_eq!(s, "router");
    }
}
