//! Userspace shell - runs at EL0
//!
//! Simple interactive shell for rOSt that uses only syscalls.

#![no_std]
#![no_main]

use librost::*;

/// Entry point for userspace shell
#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Userspace Shell ===\r\n");
    print_debug("Running at EL0 with privilege separation\r\n");
    print_debug("\r\n");

    // TODO: Implement interactive shell loop
    print_debug("Shell functionality coming soon...\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in userspace shell!\r\n");
    exit(1);
}
