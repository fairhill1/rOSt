#![no_std]
#![no_main]

extern crate alloc;
use librost::*;

// Simple bump allocator for userspace (required even if we don't use it)
use core::alloc::{GlobalAlloc, Layout};

struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut() // Panic if actually used
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No-op
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== IPC Receiver Test ===\r\n");

    // Get our PID
    let my_pid = getpid();
    print_debug("My PID: ");
    print_debug("\r\n");

    print_debug("Waiting for IPC message (polling with yields)...\r\n");

    // Poll for message in a loop (non-blocking)
    let mut msg_buf = [0u8; 32];
    let mut attempts = 0;
    let max_attempts = 1000; // Try for ~10 seconds with yields

    let bytes_received = loop {
        let result = recv_message(&mut msg_buf, 0); // 0 = non-blocking
        if result > 0 {
            break result;
        }

        attempts += 1;
        if attempts >= max_attempts {
            print_debug("No message received after polling\r\n");
            print_debug("Sender (PID 1) should have sent message to receiver (PID 2)\r\n");
            exit(1);
        }

        // Yield to other threads between polls
        // (In a real implementation, this would be a yield syscall)
        for _ in 0..100000 {
            unsafe { core::arch::asm!("nop"); }
        }
    };

    print_debug("✓ Received IPC message (");
    print_debug(" bytes)\r\n");

    // Extract shared memory ID from message (first 4 bytes)
    let shm_id = msg_buf[0] as i32
        | ((msg_buf[1] as i32) << 8)
        | ((msg_buf[2] as i32) << 16)
        | ((msg_buf[3] as i32) << 24);

    print_debug("Shared memory ID from message: ");
    print_debug("\r\n");

    // Map the shared memory
    print_debug("Mapping shared memory...\r\n");
    let shm_ptr = shm_map(shm_id);
    if shm_ptr.is_null() {
        print_debug("ERROR: Failed to map shared memory\r\n");
        exit(1);
    }
    print_debug("✓ Shared memory mapped\r\n");

    // Read message from shared memory
    print_debug("\r\n");
    print_debug("===== MESSAGE FROM SENDER =====\r\n");

    // Read up to 256 bytes (or until null terminator)
    let mut i = 0;
    unsafe {
        while i < 256 {
            let byte = *shm_ptr.add(i);
            if byte == 0 {
                break;
            }
            // Write byte to UART
            core::ptr::write_volatile(0x09000000 as *mut u8, byte);
            i += 1;
        }
    }

    print_debug("\r\n");
    print_debug("==============================\r\n");
    print_debug("\r\n");

    print_debug("✓ IPC test successful! Sender and receiver communicated.\r\n");
    print_debug("\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in IPC receiver!\r\n");
    exit(1);
}
