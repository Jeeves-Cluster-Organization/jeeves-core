//! The four base tools: read, write, edit, bash.

pub mod bash;
pub mod edit;
pub mod read;
pub mod write;

use agent_core::DynTool;
use std::sync::Arc;

pub fn default_tools() -> Vec<DynTool> {
    vec![
        Arc::new(read::ReadTool) as DynTool,
        Arc::new(write::WriteTool) as DynTool,
        Arc::new(edit::EditTool) as DynTool,
        Arc::new(bash::BashTool) as DynTool,
    ]
}
