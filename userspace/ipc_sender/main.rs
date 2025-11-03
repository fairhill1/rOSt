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
    print_debug("=== IPC Sender Test ===\r\n");

    // Get our PID
    let my_pid = getpid();
    print_debug("My PID: ");
    print_debug("\r\n");

    // Create shared memory (1KB)
    print_debug("Creating shared memory (1KB)...\r\n");
    let shm_id = shm_create(1024);
    if shm_id < 0 {
        print_debug("ERROR: Failed to create shared memory\r\n");
        exit(1);
    }
    print_debug("✓ Shared memory created, ID: ");
    print_debug("\r\n");

    // Map shared memory
    print_debug("Mapping shared memory...\r\n");
    let shm_ptr = shm_map(shm_id);
    if shm_ptr.is_null() {
        print_debug("ERROR: Failed to map shared memory\r\n");
        exit(1);
    }
    print_debug("✓ Shared memory mapped\r\n");

    // Write message to shared memory
    let message = b"Hello from IPC sender! This message is in shared memory.";
    print_debug("Writing message to shared memory...\r\n");
    unsafe {
        core::ptr::copy_nonoverlapping(
            message.as_ptr(),
            shm_ptr,
            message.len()
        );
    }
    print_debug("✓ Message written\r\n");

    // Send IPC message to receiver
    // Sender is PID 0, receiver is PID 1 (confirmed by getpid() output)
    let receiver_pid = 1;
    print_debug("Sending IPC message to receiver (PID 1)...\r\n");

    // Encode shared memory ID in message
    let mut ipc_msg = [0u8; 32];
    ipc_msg[0] = (shm_id & 0xFF) as u8;
    ipc_msg[1] = ((shm_id >> 8) & 0xFF) as u8;
    ipc_msg[2] = ((shm_id >> 16) & 0xFF) as u8;
    ipc_msg[3] = ((shm_id >> 24) & 0xFF) as u8;

    let result = send_message(receiver_pid, &ipc_msg);
    if result < 0 {
        print_debug("ERROR: Failed to send message (receiver might not be running)\r\n");
        print_debug("Make sure to run 'exec ipc_receiver' first!\r\n");
        exit(1);
    }
    print_debug("✓ IPC message sent successfully\r\n");

    print_debug("\r\n");
    print_debug("Sender done! Receiver should now read the shared memory.\r\n");
    print_debug("\r\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in IPC sender!\r\n");
    exit(1);
}
