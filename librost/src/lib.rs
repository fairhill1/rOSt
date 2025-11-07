//! librost - rOSt userspace runtime library
//!
//! Provides syscall interface for userspace programs running at EL0.
//! This is a #![no_std] library that can be linked by both kernel and userspace binaries.

#![no_std]

extern crate alloc;

pub mod runtime;
pub mod ipc_protocol;
pub mod graphics;
pub mod input;
pub mod sync;

// Re-export everything from runtime for convenience
pub use runtime::*;

// Re-export sync helpers
pub use sync::{sync_and_notify, sync_memory};

// Re-export image codecs (only when feature is enabled)
#[cfg(feature = "image-codecs")]
pub use image_codecs;

/// Debug tracing macro - only active with verbose-debug feature
///
/// Use this instead of direct print_debug calls. Debug output is compiled
/// out completely in normal builds, but can be enabled with:
/// `cargo build --release --features verbose-debug`
///
/// # Examples
/// ```
/// debug_trace!("Entering function");
/// debug_trace!("Value: {}\r\n", x);  // Note: no format! support in no_std
/// ```
#[macro_export]
#[cfg(feature = "verbose-debug")]
macro_rules! debug_trace {
    ($msg:expr) => {
        $crate::print_debug($msg)
    };
}

/// Debug trace (no-op when verbose-debug is disabled)
#[macro_export]
#[cfg(not(feature = "verbose-debug"))]
macro_rules! debug_trace {
    ($msg:expr) => {
        // Compiled to nothing
    };
}
