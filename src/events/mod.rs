//! Event infrastructure — kernel lifecycle event translation.
//!
//! Translates kernel-internal events to frontend-friendly formats.

pub mod translation;

pub use translation::translate_kernel_event;
