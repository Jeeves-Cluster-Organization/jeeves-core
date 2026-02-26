//! Event infrastructure — kernel lifecycle event translation.
//!
//! Translates kernel-internal events to frontend-friendly formats at the Rust
//! layer so Python bridges can forward pre-translated events directly.

pub mod translation;

pub use translation::translate_kernel_event;
