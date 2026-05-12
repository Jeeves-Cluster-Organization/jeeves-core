//! State accumulation across pipeline loop-backs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum MergeStrategy {
    #[default]
    Replace,
    /// Append to an array; creates the array if absent.
    Append,
    /// Shallow-merge object keys.
    MergeDict,
}

/// Schema entry for a state field — keyed lookup with a merge strategy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StateField {
    pub key: String,
    pub merge: MergeStrategy,
}
