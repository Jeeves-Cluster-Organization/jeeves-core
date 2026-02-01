//! Generated protobuf code.
//!
//! This module includes the generated code from engine.proto.
//! The actual generation happens in build.rs via tonic-build.

// Include generated protobuf code from OUT_DIR
// Package jeeves.engine.v1 becomes module structure
#[allow(clippy::all)]
#[allow(non_camel_case_types)]
pub mod jeeves {
    pub mod engine {
        pub mod v1 {
            tonic::include_proto!("jeeves.engine.v1");
        }
    }
}

// Re-export for convenience
pub use jeeves::engine::v1::*;
