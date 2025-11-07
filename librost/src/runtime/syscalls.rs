//! Userspace runtime library - syscall wrappers for EL0 programs
//!
//! This module provides the interface between userspace applications (running at EL0)
//! and the kernel (running at EL1). All userspace programs link against this runtime.

use core::arch::asm;

/// Syscall wrapper - invokes SVC instruction to trap to EL1
#[inline(always)]
pub unsafe fn syscall(num: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
    let result: i64;
    asm!(
        "svc #0",
        in("x8") num,
        inout("x0") arg0 => result,
        in("x1") arg1,
        in("x2") arg2,
        // Mark unused caller-saved registers as potentially clobbered
        // X0, X1, X2, X8 are already inputs, so only mark X3-X7, X9-X18
        out("x3") _,
        out("x4") _,
        out("x5") _,
        out("x6") _,
        out("x7") _,
        out("x9") _,
        out("x10") _,
        out("x11") _,
        out("x12") _,
        out("x13") _,
        out("x14") _,
        out("x15") _,
        out("x16") _,
        out("x17") _,
        // X18 is platform reserved, don't clobber
    );
    result
}

/// Syscall wrapper with 6 arguments (for drawing syscalls)
/// ARM64 calling convention: X0-X7 for arguments, X8 for syscall number
#[inline(always)]
pub unsafe fn syscall6(num: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i64 {
    let result: i64;
    asm!(
        "svc #0",
        in("x8") num,
        inout("x0") arg0 => result,
        in("x1") arg1,
        in("x2") arg2,
        in("x3") arg3,
        in("x4") arg4,
        in("x5") arg5,
    );
    result
}

// ============================================================================
// PROCESS MANAGEMENT SYSCALLS
// ============================================================================

/// Exit the current process with the given exit code
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall(
            8, // SyscallNumber::Exit
            code as u64,
            0,
            0
        );
    }
    // Should never reach here, but loop just in case
    loop {
        unsafe { asm!("wfe"); }
    }
}

/// Get current process ID
pub fn getpid() -> u32 {
    unsafe {
        syscall(
            9, // SyscallNumber::GetPid
            0,
            0,
            0
        ) as u32
    }
}

/// Yield CPU to another thread (cooperative multitasking)
pub fn yield_now() {
    unsafe {
        syscall(
            33, // SyscallNumber::Yield
            0,
            0,
            0
        );
    }
}

// ============================================================================
// FILE I/O SYSCALLS
// ============================================================================

/// Open a file (returns file descriptor or negative error code)
pub fn open(path: &str, flags: u32) -> i32 {
    // Create null-terminated string
    let mut path_buf = [0u8; 256];
    let path_bytes = path.as_bytes();
    let len = core::cmp::min(path_bytes.len(), 255);
    path_buf[..len].copy_from_slice(&path_bytes[..len]);
    path_buf[len] = 0; // Null terminator

    unsafe {
        syscall(
            2, // SyscallNumber::Open
            path_buf.as_ptr() as u64,
            flags as u64,
            0
        ) as i32
    }
}

/// Read from file descriptor into buffer (returns bytes read or negative error)
pub fn read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall(
            0, // SyscallNumber::Read
            fd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// Write to file descriptor from buffer (returns bytes written or negative error)
pub fn write(fd: i32, buf: &[u8]) -> isize {
    unsafe {
        syscall(
            1, // SyscallNumber::Write
            fd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// Close a file descriptor
pub fn close(fd: i32) -> i32 {
    unsafe {
        syscall(
            3, // SyscallNumber::Close
            fd as u64,
            0,
            0
        ) as i32
    }
}

// ============================================================================
// RAW BLOCK I/O SYSCALLS (for microkernel file server)
// ============================================================================

pub const SECTOR_SIZE: usize = 512;

/// Read a single 512-byte sector from block device
///
/// Args:
///   device_id: Block device index (0 = first VirtIO block device)
///   sector: Sector number to read (0-based)
///   buffer: Must be at least 512 bytes
///
/// Returns: 0 on success, negative error code on failure
pub fn read_block(device_id: u32, sector: u32, buffer: &mut [u8; SECTOR_SIZE]) -> i32 {
    unsafe {
        syscall(
            39, // SyscallNumber::ReadBlock
            device_id as u64,
            sector as u64,
            buffer.as_mut_ptr() as u64
        ) as i32
    }
}

/// Write a single 512-byte sector to block device
///
/// Args:
///   device_id: Block device index (0 = first VirtIO block device)
///   sector: Sector number to write (0-based)
///   buffer: Must be exactly 512 bytes
///
/// Returns: 0 on success, negative error code on failure
pub fn write_block(device_id: u32, sector: u32, buffer: &[u8; SECTOR_SIZE]) -> i32 {
    unsafe {
        syscall(
            40, // SyscallNumber::WriteBlock
            device_id as u64,
            sector as u64,
            buffer.as_ptr() as u64
        ) as i32
    }
}

// ============================================================================
// TIME SYSCALLS
// ============================================================================

/// Get current time in milliseconds since boot
pub fn get_time() -> i64 {
    unsafe {
        syscall(
            12, // SyscallNumber::GetTime
            0,
            0,
            0
        )
    }
}

// ============================================================================
// DEBUG SYSCALLS
// ============================================================================

/// Print debug message to serial/UART (kernel debug output)
pub fn print_debug(msg: &str) {
    unsafe {
        syscall(
            14, // SyscallNumber::PrintDebug
            msg.as_ptr() as u64,
            msg.len() as u64,
            0
        );
    }
}

// ============================================================================
// FRAMEBUFFER SYSCALLS
// ============================================================================

/// Framebuffer info structure (must match kernel definition)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FbInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: u32,
}

/// Get framebuffer information (dimensions, format)
pub fn fb_info() -> Option<FbInfo> {
    let mut info = FbInfo {
        width: 0,
        height: 0,
        stride: 0,
        pixel_format: 0,
    };

    let result = unsafe {
        syscall(
            15, // SyscallNumber::FbInfo
            &mut info as *mut _ as u64,
            0,
            0
        )
    };

    if result == 0 {
        Some(info)
    } else {
        None
    }
}

/// Map framebuffer into process address space (returns framebuffer address)
pub fn fb_map() -> Option<*mut u32> {
    let addr = unsafe {
        syscall(
            16, // SyscallNumber::FbMap
            0,
            0,
            0
        )
    };

    if addr > 0 {
        Some(addr as *mut u32)
    } else {
        None
    }
}

/// Flush framebuffer to display (trigger GPU update)
pub fn fb_flush() -> i32 {
    unsafe {
        syscall(
            17, // SyscallNumber::FbFlush
            0,
            0,
            0
        ) as i32
    }
}

// ============================================================================
// INPUT EVENT SYSCALLS
// ============================================================================

/// Input event structure (must match kernel definition)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub event_type: u32,  // 0=None, 1=KeyPressed, 2=KeyReleased, 3=MouseMove, 4=MouseButton, 5=MouseWheel
    pub key: u8,
    pub modifiers: u8,
    pub button: u8,
    pub pressed: u8,
    pub x_delta: i8,
    pub y_delta: i8,
    pub wheel_delta: i8,
}

/// Poll for input events (keyboard/mouse)
pub fn poll_event() -> Option<InputEvent> {
    let mut event = InputEvent {
        event_type: 0,
        key: 0,
        modifiers: 0,
        button: 0,
        pressed: 0,
        x_delta: 0,
        y_delta: 0,
        wheel_delta: 0,
    };

    let result = unsafe {
        syscall(
            18, // SyscallNumber::PollEvent
            &mut event as *mut _ as u64,
            0,
            0
        )
    };

    if result > 0 && event.event_type != 0 {
        Some(event)
    } else {
        None
    }
}

// ============================================================================
// NETWORK SYSCALLS
// ============================================================================

// Socket constants
pub const AF_INET: u32 = 2;
pub const SOCK_STREAM: u32 = 1;

/// Socket address structure (must match kernel definition)
#[repr(C)]
pub struct SockAddrIn {
    pub family: u16,
    pub port: u16,    // big endian
    pub addr: u32,    // big endian
    pub zero: [u8; 8],
}

impl SockAddrIn {
    pub fn new(ip: [u8; 4], port: u16) -> Self {
        Self {
            family: AF_INET as u16,
            port: port.to_be(),
            addr: u32::from_be_bytes(ip),
            zero: [0; 8],
        }
    }
}

/// Create a socket (returns socket fd or negative error code)
pub fn socket(domain: u32, socket_type: u32) -> i32 {
    unsafe {
        syscall(
            19, // SyscallNumber::Socket
            domain as u64,
            socket_type as u64,
            0
        ) as i32
    }
}

/// Connect to remote address (returns 0 on success, negative on error)
pub fn connect(sockfd: i32, addr: &SockAddrIn) -> i32 {
    unsafe {
        syscall(
            20, // SyscallNumber::Connect
            sockfd as u64,
            addr as *const _ as u64,
            0
        ) as i32
    }
}

/// Send data on socket (returns bytes sent or negative error)
pub fn send(sockfd: i32, buf: &[u8]) -> isize {
    unsafe {
        syscall(
            21, // SyscallNumber::Send
            sockfd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// Receive data from socket (returns bytes received or negative error)
pub fn recv(sockfd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall(
            22, // SyscallNumber::Recv
            sockfd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

// ============================================================================
// IPC SYSCALLS
// ============================================================================

/// Create a shared memory region
/// Returns: shared memory ID on success, negative error code on failure
pub fn shm_create(size: usize) -> i32 {
    unsafe {
        syscall(
            26, // SyscallNumber::ShmCreate
            size as u64,
            0,
            0
        ) as i32
    }
}

/// Map a shared memory region into process address space
/// Returns: pointer to mapped memory on success, null on failure
pub fn shm_map(shm_id: i32) -> *mut u8 {
    let addr = unsafe {
        syscall(
            27, // SyscallNumber::ShmMap
            shm_id as u64,
            0,
            0
        )
    };
    
    if addr < 0 {
        core::ptr::null_mut()
    } else {
        addr as *mut u8
    }
}

/// Map a shared memory region from a specific process
/// Used by WM to access per-process shared memory with same IDs
/// Returns: pointer to memory on success, null on failure
pub fn shm_map_from_process(process_id: usize, shm_id: i32) -> *mut u8 {
    let addr = unsafe {
        syscall(
            37, // SyscallNumber::ShmMapFromProcess
            process_id as u64,
            shm_id as u64,
            0
        )
    };

    if addr < 0 {
        core::ptr::null_mut()
    } else {
        addr as *mut u8
    }
}

/// Destroy a shared memory region and free its physical memory
/// CRITICAL: Call this instead of shm_unmap when you're done with a region
/// to prevent resource leaks. shm_unmap only unmaps, shm_destroy frees memory.
/// Returns: 0 on success, negative error code on failure
pub fn shm_destroy(shm_id: i32) -> i32 {
    unsafe {
        syscall(
            38, // SyscallNumber::ShmDestroy
            shm_id as u64,
            0,
            0
        ) as i32
    }
}

/// Unmap a shared memory region
/// Returns: 0 on success, negative error code on failure
pub fn shm_unmap(shm_id: i32) -> i32 {
    unsafe {
        syscall(
            28, // SyscallNumber::ShmUnmap
            shm_id as u64,
            0,
            0
        ) as i32
    }
}

/// Send a message to another process
/// Returns: 0 on success, negative error code on failure
pub fn send_message(dest_pid: u32, data: &[u8]) -> i32 {
    unsafe {
        syscall(
            29, // SyscallNumber::SendMessage
            dest_pid as u64,
            data.as_ptr() as u64,
            data.len() as u64
        ) as i32
    }
}

/// Receive a message from message queue
/// timeout_ms: parameter is IGNORED (kernel syscall is non-blocking)
/// Returns: number of bytes received on success, 0 if no message, negative on error
///
/// NOTE: This is non-blocking. If no message is available, returns 0 immediately.
/// Caller should implement retry logic if needed.
pub fn recv_message(buf: &mut [u8], _timeout_ms: u32) -> isize {
    unsafe {
        syscall(
            30, // SyscallNumber::RecvMessage
            buf.as_mut_ptr() as u64,
            buf.len() as u64,
            0
        ) as isize
    }
}

// ============================================================================
// DRAWING SYSCALLS (TrueType Font Rendering)
// ============================================================================

/// Draw text to the framebuffer using kernel's TrueType font renderer
/// x, y: Position in pixels
/// text: Text string to draw
/// color: ARGB color (0xAARRGGBB)
/// Returns: 0 on success, negative error code on failure
pub fn draw_text(x: i32, y: i32, text: &str, color: u32) -> i32 {
    unsafe {
        syscall6(
            31, // SyscallNumber::DrawText
            x as u64,
            y as u64,
            text.as_ptr() as u64,
            text.len() as u64,
            color as u64,
            0 // unused arg
        ) as i32
    }
}

/// Draw a filled rectangle to the framebuffer
/// x, y: Top-left position in pixels
/// width, height: Dimensions in pixels
/// color: ARGB color (0xAARRGGBB)
/// Returns: 0 on success, negative error code on failure
pub fn draw_rect(x: i32, y: i32, width: u32, height: u32, color: u32) -> i32 {
    unsafe {
        syscall6(
            32, // SyscallNumber::DrawRect
            x as u64,
            y as u64,
            width as u64,
            height as u64,
            color as u64,
            0 // unused arg
        ) as i32
    }
}

// ============================================================================
// SCHEDULER SYSCALLS
// ============================================================================

/// Yield CPU to scheduler - let other threads/processes run
/// This is critical for cooperative multitasking in userspace apps
pub fn sched_yield() {
    unsafe {
        syscall(
            33, // SyscallNumber::Yield
            0,
            0,
            0
        );
    }
}

// ============================================================================
// PROCESS MANAGEMENT SYSCALLS (for microkernel WM)
// ============================================================================

/// Spawn a new process from an ELF file
/// path: Path to ELF file (e.g., "/bin/terminal")
/// Returns: PID of new process, or negative error code on failure
pub fn spawn_elf(path: &str) -> i64 {
    unsafe {
        syscall(
            34, // SyscallNumber::SpawnElf
            path.as_ptr() as u64,
            path.len() as u64,
            0
        )
    }
}

/// Kill a process by PID
/// pid: Process ID to kill
/// Returns: 0 on success, negative error code on failure
pub fn kill(pid: u64) -> i32 {
    unsafe {
        syscall(
            35, // SyscallNumber::Kill
            pid,
            0,
            0
        ) as i32
    }
}

/// Flush a region of the framebuffer to the display
/// x, y: Top-left corner of region
/// width, height: Dimensions of region
/// Returns: 0 on success, negative error code on failure
pub fn fb_flush_region(x: u32, y: u32, width: u32, height: u32) -> i32 {
    unsafe {
        syscall6(
            36, // SyscallNumber::FbFlushRegion
            x as u64,
            y as u64,
            width as u64,
            height as u64,
            0, // unused
            0  // unused
        ) as i32
    }
}
