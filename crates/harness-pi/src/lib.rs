//! Pi-style harness over `agent-core`.
//!
//! Ships the four base tools (read / write / edit / bash) and a context-file
//! autoloader (AGENTS.md / CLAUDE.md walk). Installs no hooks by default —
//! isolation is the container's job, not the library's.

pub mod context;
pub mod settings;
pub mod tools;

pub use settings::Settings;
