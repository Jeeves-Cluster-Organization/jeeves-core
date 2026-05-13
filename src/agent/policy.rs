//! Agent-level execution policies.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Strategy when the LLM context exceeds `Stage::max_context_tokens`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, JsonSchema)]
pub enum ContextOverflow {
    #[default]
    Fail,
    /// Drop oldest non-system messages until under the limit.
    TruncateOldest,
}
