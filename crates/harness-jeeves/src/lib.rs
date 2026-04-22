//! Jeeves-style harness over `agent-core`.
//!
//! Ships opt-in middleware for the production shape:
//!   * [`confirmation::ConfirmationGate`] — `BeforeToolCall` hook that defers
//!     to a consumer-provided `Decider` for destructive tools.
//!   * [`compaction::SlidingWindow`] — `TransformContext` hook that trims
//!     old turns once the message window exceeds a budget.
//!
//! Observability helpers live under [`observability`] (OTel gated behind the
//! `otel` feature).

pub mod compaction;
pub mod confirmation;
pub mod observability;
