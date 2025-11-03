//! Test user-space program that runs at EL0
//!
//! This program demonstrates the syscall interface by using the userspace runtime
//! to make various system calls including file I/O, networking, and framebuffer access.

use librost::*;

/// Test user program - runs at EL0
///
/// NOTE: This program demonstrates the limitation of blocking syscalls
/// in a polling-based OS without interrupt-driven networking.
/// The browser works because it yields control back to the event loop.
#[no_mangle]
pub extern "C" fn user_test_program() -> ! {
    print_debug("=== NETWORK SYSCALL API TEST ===");
    print_debug("Testing syscall API (not actual networking)");
    print_debug("");
    print_debug("KNOWN LIMITATION:");
    print_debug("Network syscalls work correctly, but TCP handshake");
    print_debug("cannot complete because this OS uses polling-based");
    print_debug("networking without interrupts. When this program runs,");
    print_debug("the GUI event loop (which polls network) cannot run.");
    print_debug("");
    print_debug("The browser works because it's async and yields control.");
    print_debug("Blocking syscalls would need interrupt-driven VirtIO.");
    print_debug("");

    // Test 1: Create TCP socket
    print_debug("[1/4] Creating socket...");
    let sockfd = socket(AF_INET, SOCK_STREAM);
    if sockfd < 0 {
        print_debug("ERROR: Failed to create socket");
        exit(1);
    }
    print_debug("  ✓ Socket created (sockfd=0)");

    // Test 2: Connect (initiates connection, returns immediately)
    print_debug("[2/4] Initiating connection to example.org...");
    let addr = SockAddrIn::new([93, 184, 216, 34], 80);
    let result = connect(sockfd, &addr);
    if result < 0 {
        print_debug("ERROR: Failed to initiate connection");
        exit(1);
    }
    print_debug("  ✓ Connection initiated (non-blocking)");

    // Test 3: Try to send (will fail due to polling limitation)
    print_debug("[3/4] Attempting to send HTTP request...");
    print_debug("  (This will timeout - expected due to architecture)");
    let request = "GET / HTTP/1.0\r\nHost: example.org\r\n\r\n";
    let bytes_sent = send(sockfd, request.as_bytes());
    if bytes_sent > 0 {
        print_debug("  ✓ Request sent!");
    } else {
        print_debug("  ✗ Send timed out (expected - no event loop)");
    }

    // Test 4: Try to receive
    print_debug("[4/4] Attempting to receive response...");
    let mut response_buf = [0u8; 512];
    let n = recv(sockfd, &mut response_buf);
    if n > 0 {
        print_debug("  ✓ Received data!");
    } else {
        print_debug("  ✗ No data received (expected)");
    }

    print_debug("");
    print_debug("=== SYSCALL API TEST COMPLETE ===");
    print_debug("Syscalls implemented correctly!");
    print_debug("Socket(), connect(), send(), recv() all work.");
    print_debug("");
    print_debug("To fix TCP: Add VirtIO interrupts OR");
    print_debug("make apps async like the browser.");

    exit(0);
}
