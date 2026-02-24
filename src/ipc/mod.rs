//! TCP+msgpack IPC transport layer.
//!
//! Replaces the gRPC service layer. Implements length-prefixed msgpack framing
//! matching the protocol defined in `python/jeeves_infra/ipc/transport.py`.

pub mod codec;
pub mod dispatch;
pub mod handlers;
pub mod server;

pub use server::IpcServer;
