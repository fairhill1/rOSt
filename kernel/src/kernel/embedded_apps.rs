//! Embedded userspace applications
//!
//! Contains ELF binaries embedded into the kernel image at compile time.

/// Embedded shell ELF binary
pub static SHELL_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/shell"
));

/// Embedded image viewer ELF binary
pub static IMAGE_VIEWER_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/image_viewer"
));

/// Embedded IPC sender test binary
pub static IPC_SENDER_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/ipc_sender"
));

/// Embedded IPC receiver test binary
pub static IPC_RECEIVER_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/ipc_receiver"
));
