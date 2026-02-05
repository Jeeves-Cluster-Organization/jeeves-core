//! Strongly-typed identifiers.
//!
//! All IDs are validated at construction time and implement common traits.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Macro to define a strongly-typed ID newtype wrapper.
///
/// Generates: struct, `from_string()`, `as_str()`, Display, Serialize, Deserialize.
/// Optionally generates `new()` (UUID v4) and `Default` if `uuid` flag is passed.
macro_rules! define_id {
    ($name:ident, uuid) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

            pub fn as_str(&self) -> &str {
                &self.0
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
    };
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn from_string(s: String) -> Result<Self, &'static str> {
                if s.is_empty() {
                    return Err(concat!(stringify!($name), " cannot be empty"));
                }
                Ok(Self(s))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(ProcessId, uuid);
define_id!(EnvelopeId, uuid);
define_id!(RequestId, uuid);
define_id!(SessionId, uuid);
define_id!(UserId);
