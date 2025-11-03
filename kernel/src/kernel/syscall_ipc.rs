// IPC syscall implementations
// Shared memory and message passing for inter-process communication

use crate::kernel::syscall::{SyscallError, IpcMessage, MAX_MESSAGE_SIZE};
use alloc::vec;

/// Create a shared memory region
/// Returns: shared memory ID on success, negative error code on failure
pub fn sys_shm_create(size: usize) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] shm_create(size=");
    crate::kernel::uart_write_string(")\r\n");

    if size == 0 || size > 16 * 1024 * 1024 {  // Max 16MB
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get current process
    let process_id = match get_current_process() {
        Some(pid) => pid,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Allocate physical memory for shared region
    let memory = vec![0u8; size].into_boxed_slice();
    let physical_addr = memory.as_ptr() as u64;

    // Leak the memory (it will be managed by the shared memory system)
    alloc::boxed::Box::leak(memory);

    // Allocate shared memory region in process table
    let shm_id = crate::kernel::thread::with_process_mut(process_id, |process| {
        process.shm_table.alloc(size, physical_addr)
    });

    match shm_id {
        Some(Some(id)) => {
            crate::kernel::uart_write_string("[SYSCALL] shm_create() -> id=");
            if id < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + id as u8);
                }
            }
            crate::kernel::uart_write_string("\r\n");
            id as i64
        }
        _ => SyscallError::OutOfMemory.as_i64(),
    }
}

/// Map a shared memory region into process address space
/// Returns: virtual address on success, negative error code on failure
pub fn sys_shm_map(shm_id: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] shm_map(id=");
    if shm_id < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + shm_id as u8);
        }
    }
    crate::kernel::uart_write_string(")\r\n");

    // CRITICAL: Search ALL processes for the shared memory region
    // Shared memory is meant to be shared across processes!
    if let Some(physical_addr) = crate::kernel::thread::find_shared_memory(shm_id) {
        crate::kernel::uart_write_string("[SYSCALL] shm_map() -> 0x");
        // Print hex address
        let hex_chars = b"0123456789ABCDEF";
        for i in (0..16).rev() {
            let digit = (physical_addr >> (i * 4)) & 0xF;
            unsafe {
                core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[digit as usize]);
            }
        }
        crate::kernel::uart_write_string("\r\n");
        physical_addr as i64
    } else {
        // Not found in any process
        crate::kernel::uart_write_string("[SYSCALL] shm_map() -> not found\r\n");
        SyscallError::InvalidArgument.as_i64()
    }
}

/// Unmap a shared memory region
/// Returns: 0 on success, negative error code on failure
pub fn sys_shm_unmap(shm_id: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] shm_unmap(id=");
    if shm_id < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + shm_id as u8);
        }
    }
    crate::kernel::uart_write_string(")\r\n");

    // Get current process
    let process_id = match get_current_process() {
        Some(pid) => pid,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Unmap the region
    let result = crate::kernel::thread::with_process_mut(process_id, |process| {
        if let Some(region) = process.shm_table.get_mut(shm_id) {
            region.virtual_addr = None;
            true
        } else {
            false
        }
    });

    match result {
        Some(true) => {
            crate::kernel::uart_write_string("[SYSCALL] shm_unmap() -> success\r\n");
            0
        }
        _ => SyscallError::InvalidArgument.as_i64(),
    }
}

/// Send a message to another process
/// Returns: 0 on success, negative error code on failure
pub fn sys_send_message(dest_pid: u32, data: *const u8, len: usize) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] send_message(dest_pid=");
    if dest_pid < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + dest_pid as u8);
        }
    }
    crate::kernel::uart_write_string(", len=");
    crate::kernel::uart_write_string(")\r\n");

    if data.is_null() || len == 0 || len > MAX_MESSAGE_SIZE {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get sender process ID
    let sender_pid = match get_current_process() {
        Some(pid) => pid as u32,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Copy message data from userspace
    let mut msg = IpcMessage {
        sender_pid,
        data_len: len as u32,
        data: [0u8; MAX_MESSAGE_SIZE],
    };
    unsafe {
        core::ptr::copy_nonoverlapping(data, msg.data.as_mut_ptr(), len);
    }

    // Push message to destination process queue
    let result = crate::kernel::thread::with_process_mut(dest_pid as usize, |process| {
        process.message_queue.push(msg)
    });

    match result {
        Some(true) => {
            crate::kernel::uart_write_string("[SYSCALL] send_message() -> success\r\n");
            0
        }
        Some(false) => {
            crate::kernel::uart_write_string("[SYSCALL] send_message() -> queue full\r\n");
            SyscallError::OutOfMemory.as_i64()
        }
        None => {
            crate::kernel::uart_write_string("[SYSCALL] send_message() -> process not found\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
    }
}

/// Receive a message from message queue
/// Returns: number of bytes received on success, 0 if no message, negative on error
pub fn sys_recv_message(buf: *mut u8, len: usize, timeout_ms: u32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] recv_message(len=");
    crate::kernel::uart_write_string(", timeout=");
    crate::kernel::uart_write_string(")\r\n");

    if buf.is_null() || len == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get current process
    let process_id = match get_current_process() {
        Some(pid) => pid,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Try to receive message (with simple polling if timeout > 0)
    let start_time = crate::kernel::get_time_ms();
    loop {
        let msg = crate::kernel::thread::with_process_mut(process_id, |process| {
            process.message_queue.pop()
        });

        if let Some(Some(msg)) = msg {
            // Copy message to user buffer
            let copy_len = core::cmp::min(msg.data_len as usize, len);
            unsafe {
                core::ptr::copy_nonoverlapping(msg.data.as_ptr(), buf, copy_len);
            }

            crate::kernel::uart_write_string("[SYSCALL] recv_message() -> ");
            if copy_len < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + copy_len as u8);
                }
            }
            crate::kernel::uart_write_string(" bytes\r\n");

            return copy_len as i64;
        }

        // Check timeout
        if timeout_ms == 0 {
            // Non-blocking: return immediately if no message
            return 0;
        }

        let elapsed = crate::kernel::get_time_ms() - start_time;
        if elapsed >= timeout_ms as u64 {
            // Timeout reached
            crate::kernel::uart_write_string("[SYSCALL] recv_message() -> timeout\r\n");
            return 0;
        }

        // CRITICAL: Yield to other threads instead of busy-waiting
        // This allows other processes to run while we wait for a message
        crate::kernel::thread::yield_now();
    }
}

/// Helper: Get current process ID
fn get_current_process() -> Option<usize> {
    let scheduler = crate::kernel::scheduler::SCHEDULER.lock();
    scheduler.current_thread.and_then(|thread_id| {
        scheduler.threads.iter()
            .find(|t| t.id == thread_id)
            .map(|t| t.process_id)
    })
}
