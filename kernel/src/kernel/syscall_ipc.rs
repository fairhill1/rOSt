// IPC syscall implementations
// Shared memory and message passing for inter-process communication

use crate::kernel::syscall::{SyscallError, IpcMessage, MAX_MESSAGE_SIZE};

/// Simple bump allocator for non-cacheable shared memory region
/// Physical range: 0x60000000 - 0x70000000 (256MB)
/// This region is mapped as ATTR_NORMAL_NON_CACHEABLE in page tables
static SHM_ALLOCATOR: spin::Mutex<ShmAllocator> = spin::Mutex::new(ShmAllocator::new());

struct ShmAllocator {
    next_addr: u64,
    end_addr: u64,
}

impl ShmAllocator {
    const fn new() -> Self {
        Self {
            next_addr: 0x60000000, // Start of non-cacheable shared memory region
            end_addr: 0x70000000,   // End (256MB region)
        }
    }

    fn allocate(&mut self, size: usize) -> Option<u64> {
        let aligned_size = (size + 4095) & !4095; // Align to 4KB
        if self.next_addr + aligned_size as u64 > self.end_addr {
            return None; // Out of shared memory space
        }

        let addr = self.next_addr;
        self.next_addr += aligned_size as u64;

        // Zero the memory
        unsafe {
            core::ptr::write_bytes(addr as *mut u8, 0, aligned_size);
        }

        Some(addr)
    }
}

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

    crate::kernel::uart_write_string("[SHM] Allocating from non-cacheable region (0x60000000-0x70000000)\r\n");
    // Allocate from non-cacheable shared memory region (0x60000000-0x70000000)
    // This region is mapped as ATTR_NORMAL_NON_CACHEABLE in page tables
    // NO MANUAL CACHE FLUSHES NEEDED - hardware handles coherency automatically!
    let physical_addr = match SHM_ALLOCATOR.lock().allocate(size) {
        Some(addr) => {
            crate::kernel::uart_write_string("[SHM] Allocated non-cacheable memory\r\n");
            addr
        }
        None => {
            crate::kernel::uart_write_string("[SHM] Failed - out of shared memory space\r\n");
            return SyscallError::OutOfMemory.as_i64();
        }
    };

    // Register in process table
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
    // CRITICAL: Search ALL processes for the shared memory region
    // Shared memory is meant to be shared across processes!
    if let Some(physical_addr) = crate::kernel::thread::find_shared_memory(shm_id) {
        physical_addr as i64
    } else {
        // Not found in any process
        SyscallError::InvalidArgument.as_i64()
    }
}

/// Map a shared memory region from a specific process
/// Used by WM to access per-process shared memory with same IDs
/// Returns: physical address on success, negative error code on failure
pub fn sys_shm_map_from_process(process_id: usize, shm_id: i32) -> i64 {
    if let Some(physical_addr) = crate::kernel::thread::find_shared_memory_by_process(process_id, shm_id) {
        physical_addr as i64
    } else {
        SyscallError::InvalidArgument.as_i64()
    }
}

/// Destroy a shared memory region from a specific process
/// Used by WM to clean up shared memory from dead processes
/// Returns: 0 on success, negative error code on failure
pub fn sys_shm_destroy_from_process(process_id: usize, shm_id: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] shm_destroy_from_process(pid=");
    crate::kernel::uart_write_string(", id=");
    if shm_id < 10 {
        unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + shm_id as u8); }
    }
    crate::kernel::uart_write_string(")\r\n");

    // Deallocate the shared memory region from specified process
    let success = crate::kernel::thread::with_process_mut(process_id, |process| {
        process.shm_table.dealloc(shm_id)
    });

    match success {
        Some(true) => {
            crate::kernel::uart_write_string("[SYSCALL] shm_destroy_from_process() -> SUCCESS\r\n");
            0
        }
        _ => {
            crate::kernel::uart_write_string("[SYSCALL] shm_destroy_from_process() -> FAILED (not found)\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
    }
}

/// Destroy a shared memory region and free its physical memory
/// This is critical to prevent resource leaks when resizing windows
/// Returns: 0 on success, negative error code on failure
pub fn sys_shm_destroy(shm_id: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] shm_destroy(id=");
    if shm_id < 10 {
        unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + shm_id as u8); }
    }
    crate::kernel::uart_write_string(")\r\n");

    // Get current process
    let process_id = match get_current_process() {
        Some(pid) => pid,
        None => {
            crate::kernel::uart_write_string("[SYSCALL] shm_destroy() -> no current process\r\n");
            return SyscallError::InvalidArgument.as_i64();
        }
    };

    // Deallocate the shared memory region (frees physical memory)
    let success = crate::kernel::thread::with_process_mut(process_id, |process| {
        process.shm_table.dealloc(shm_id)
    });

    match success {
        Some(true) => {
            crate::kernel::uart_write_string("[SYSCALL] shm_destroy() -> SUCCESS\r\n");
            0
        }
        _ => {
            crate::kernel::uart_write_string("[SYSCALL] shm_destroy() -> FAILED (not found)\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
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
    if data.is_null() || len == 0 || len > MAX_MESSAGE_SIZE {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get sender process ID
    let sender_pid = match get_current_process() {
        Some(pid) => pid as u32,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // DEBUG: Only for file server (PID 2) → terminal (PID 4) messages
    if sender_pid == 2 && dest_pid == 4 {
        crate::kernel::uart_write_string("[IPC] File server→terminal send_message\r\n");
    }

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

    if sender_pid == 2 && dest_pid == 4 {
        match result {
            Some(true) => crate::kernel::uart_write_string("[IPC] → pushed successfully\r\n"),
            Some(false) => crate::kernel::uart_write_string("[IPC] → QUEUE FULL\r\n"),
            None => crate::kernel::uart_write_string("[IPC] → PROCESS NOT FOUND\r\n"),
        }
    }

    match result {
        Some(true) => 0,
        Some(false) => SyscallError::OutOfMemory.as_i64(), // Queue full
        None => SyscallError::InvalidArgument.as_i64(), // Process not found
    }
}

/// Receive a message from message queue
/// Returns: number of bytes received on success, 0 if no message, negative on error
/// NOTE: timeout_ms parameter is IGNORED - syscalls must be non-blocking
/// Userspace should implement retry loops if needed
pub fn sys_recv_message(buf: *mut u8, len: usize, _timeout_ms: u32) -> i64 {
    if buf.is_null() || len == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get current process
    let process_id = match get_current_process() {
        Some(pid) => pid,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Try to receive message (non-blocking)
    let msg = crate::kernel::thread::with_process_mut(process_id, |process| {
        process.message_queue.pop()
    });

    if let Some(Some(msg)) = msg {
        // DEBUG: Terminal receiving from file server
        if process_id == 4 && msg.sender_pid == 2 {
            crate::kernel::uart_write_string("[IPC] Terminal received message from file server\r\n");
        }

        // Copy message to user buffer
        let copy_len = core::cmp::min(msg.data_len as usize, len);
        unsafe {
            core::ptr::copy_nonoverlapping(msg.data.as_ptr(), buf, copy_len);
        }

        return copy_len as i64;
    }

    // No message available
    0
}

/// Helper: Get current process ID
fn get_current_process() -> Option<usize> {
    // CRITICAL: Disable interrupts to prevent deadlock
    let daif = crate::kernel::interrupts::disable_interrupts();

    let scheduler = crate::kernel::scheduler::SCHEDULER.lock();
    let result = scheduler.current_thread.and_then(|thread_id| {
        scheduler.threads.iter()
            .find(|t| t.id == thread_id)
            .map(|t| t.process_id)
    });

    drop(scheduler);
    crate::kernel::interrupts::restore_interrupts(daif);
    result
}
