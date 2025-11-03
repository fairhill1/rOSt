//! Userspace runtime - syscall wrappers for EL0 programs

mod syscalls;

// Re-export all syscalls
pub use syscalls::*;
