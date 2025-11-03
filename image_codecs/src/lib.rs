//! Image codec library for rOSt
//!
//! Provides BMP, PNG, and JPEG decoding for both kernel and userspace.
//! This is a `#![no_std]` crate that can be shared across privilege levels.

#![no_std]

extern crate alloc;
use alloc::vec::Vec;

// Re-export codec modules
pub mod bmp;
pub mod png;
pub mod jpeg;

// Re-export common types
pub use bmp::{BmpImage, decode_bmp};
pub use png::decode_png;
pub use jpeg::decode_jpeg;
