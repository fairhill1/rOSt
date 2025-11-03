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
