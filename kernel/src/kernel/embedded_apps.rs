//! Embedded userspace applications
//!
//! Contains ELF binaries embedded into the kernel image at compile time.

/// Embedded shell ELF binary (temporarily disabled - needs allocator fix)
pub static SHELL_ELF: &[u8] = &[];
// include_bytes!(concat!(
//     env!("CARGO_MANIFEST_DIR"),
//     "/../target/aarch64-unknown-none/release/shell"
// ));

/// Embedded image viewer ELF binary (temporarily disabled - needs allocator fix)
pub static IMAGE_VIEWER_ELF: &[u8] = &[];
// include_bytes!(concat!(
//     env!("CARGO_MANIFEST_DIR"),
//     "/../target/aarch64-unknown-none/release/image_viewer"
// ));

/// Embedded CSV viewer ELF binary (temporarily disabled - needs allocator fix)
pub static CSV_VIEWER_ELF: &[u8] = &[];
// include_bytes!(concat!(
//     env!("CARGO_MANIFEST_DIR"),
//     "/../target/aarch64-unknown-none/release/csv_viewer"
// ));

/// Embedded window manager ELF binary
pub static WINDOW_MANAGER_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/window_manager"
));

/// Embedded terminal ELF binary
pub static TERMINAL_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/terminal"
));

/// Embedded file server ELF binary
pub static FILE_SERVER_ELF: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/aarch64-unknown-none/release/file_server"
));
