//! librost - rOSt userspace runtime library
//!
//! Provides syscall interface for userspace programs running at EL0.
//! This is a #![no_std] library that can be linked by both kernel and userspace binaries.

#![no_std]

pub mod runtime;
pub mod ipc_protocol;

// Re-export everything from runtime for convenience
pub use runtime::*;

// Re-export image codecs (only when feature is enabled)
#[cfg(feature = "image-codecs")]
pub use image_codecs;
