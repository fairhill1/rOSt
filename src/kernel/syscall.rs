// ARM64 syscall interface for EL0 → EL1 transitions

use bitflags::bitflags;

/// Syscall numbers for rOSt
/// Follows ARM64 convention: X8 = syscall number, X0-X6 = arguments, X0 = return value
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    // File operations
    Read = 0,
    Write = 1,
    Open = 2,
    Close = 3,

    // Filesystem operations
    Stat = 4,
    Fstat = 5,
    Lseek = 6,
    Unlink = 7,

    // Process operations
    Exit = 8,
    GetPid = 9,

    // Memory operations
    Mmap = 10,
    Munmap = 11,

    // Time operations
    GetTime = 12,
    Sleep = 13,

    // Console/Debug operations
    PrintDebug = 14,
}

impl SyscallNumber {
    /// Convert raw u64 to syscall number
    pub fn from_u64(n: u64) -> Option<Self> {
        match n {
            0 => Some(Self::Read),
            1 => Some(Self::Write),
            2 => Some(Self::Open),
            3 => Some(Self::Close),
            4 => Some(Self::Stat),
            5 => Some(Self::Fstat),
            6 => Some(Self::Lseek),
            7 => Some(Self::Unlink),
            8 => Some(Self::Exit),
            9 => Some(Self::GetPid),
            10 => Some(Self::Mmap),
            11 => Some(Self::Munmap),
            12 => Some(Self::GetTime),
            13 => Some(Self::Sleep),
            14 => Some(Self::PrintDebug),
            _ => None,
        }
    }
}

bitflags! {
    /// File open flags (for sys_open)
    #[derive(Debug, Clone, Copy)]
    pub struct OpenFlags: u32 {
        const READ   = 1 << 0;
        const WRITE  = 1 << 1;
        const CREATE = 1 << 2;
        const TRUNC  = 1 << 3;
        const APPEND = 1 << 4;
    }
}

bitflags! {
    /// Memory protection flags (for sys_mmap)
    #[derive(Debug, Clone, Copy)]
    pub struct ProtFlags: u32 {
        const READ  = 1 << 0;
        const WRITE = 1 << 1;
        const EXEC  = 1 << 2;
    }
}

/// Syscall error codes (returned as negative values in X0)
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallError {
    Success = 0,
    InvalidSyscall = -1,
    InvalidArgument = -2,
    FileNotFound = -3,
    PermissionDenied = -4,
    OutOfMemory = -5,
    BadFileDescriptor = -6,
    NotImplemented = -99,
}

impl SyscallError {
    pub fn as_i64(self) -> i64 {
        self as i64
    }
}

/// ARM64 syscall arguments extracted from registers
#[derive(Debug, Clone, Copy)]
pub struct SyscallArgs {
    pub arg0: u64,
    pub arg1: u64,
    pub arg2: u64,
    pub arg3: u64,
    pub arg4: u64,
    pub arg5: u64,
    pub arg6: u64,
}

impl SyscallArgs {
    /// Create syscall args from ARM64 registers
    pub fn new(x0: u64, x1: u64, x2: u64, x3: u64, x4: u64, x5: u64, x6: u64) -> Self {
        Self {
            arg0: x0,
            arg1: x1,
            arg2: x2,
            arg3: x3,
            arg4: x4,
            arg5: x5,
            arg6: x6,
        }
    }
}

/// Main syscall dispatcher - called from exception handler
/// Returns value to put in X0 (return value or error code)
pub fn handle_syscall(syscall_num: u64, args: SyscallArgs) -> i64 {
    let syscall = match SyscallNumber::from_u64(syscall_num) {
        Some(sc) => sc,
        None => return SyscallError::InvalidSyscall.as_i64(),
    };

    match syscall {
        SyscallNumber::Read => sys_read(args.arg0 as i32, args.arg1 as *mut u8, args.arg2 as usize),
        SyscallNumber::Write => sys_write(args.arg0 as i32, args.arg1 as *const u8, args.arg2 as usize),
        SyscallNumber::Open => sys_open(args.arg0 as *const u8, args.arg1 as u32),
        SyscallNumber::Close => sys_close(args.arg0 as i32),
        SyscallNumber::Exit => sys_exit(args.arg0 as i32),
        SyscallNumber::GetPid => sys_getpid(),
        SyscallNumber::GetTime => sys_gettime(),
        SyscallNumber::PrintDebug => sys_print_debug(args.arg0 as *const u8, args.arg1 as usize),
        _ => SyscallError::NotImplemented.as_i64(),
    }
}

// Syscall implementations (stubs for now)

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    // TODO: Implement file reading
    crate::kernel::uart_write_string("[SYSCALL] read() called\r\n");
    SyscallError::NotImplemented.as_i64()
}

fn sys_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    // TODO: Implement file writing
    // For now, if fd=1 (stdout), write to UART for debugging
    if fd == 1 && !buf.is_null() && count > 0 {
        let slice = unsafe { core::slice::from_raw_parts(buf, count) };
        if let Ok(s) = core::str::from_utf8(slice) {
            crate::kernel::uart_write_string(s);
            return count as i64;
        }
    }
    SyscallError::BadFileDescriptor.as_i64()
}

fn sys_open(path: *const u8, flags: u32) -> i64 {
    // TODO: Implement file opening
    crate::kernel::uart_write_string("[SYSCALL] open() called\r\n");
    SyscallError::NotImplemented.as_i64()
}

fn sys_close(fd: i32) -> i64 {
    // TODO: Implement file closing
    crate::kernel::uart_write_string("[SYSCALL] close() called\r\n");
    SyscallError::NotImplemented.as_i64()
}

fn sys_exit(code: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] exit() called with code: ");
    // Simple digit print
    if code >= 0 && code < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + code as u8);
        }
    }
    crate::kernel::uart_write_string("\r\n");
    // TODO: Terminate the calling process
    SyscallError::NotImplemented.as_i64()
}

fn sys_getpid() -> i64 {
    // TODO: Return actual process ID
    crate::kernel::uart_write_string("[SYSCALL] getpid() called\r\n");
    1 // Dummy PID
}

fn sys_gettime() -> i64 {
    // Return current time in milliseconds
    crate::kernel::get_time_ms() as i64
}

fn sys_print_debug(msg: *const u8, len: usize) -> i64 {
    // Debug helper - print to UART
    if !msg.is_null() && len > 0 {
        let slice = unsafe { core::slice::from_raw_parts(msg, len) };
        if let Ok(s) = core::str::from_utf8(slice) {
            crate::kernel::uart_write_string("[USER] ");
            crate::kernel::uart_write_string(s);
            crate::kernel::uart_write_string("\r\n");
            return 0;
        }
    }
    SyscallError::InvalidArgument.as_i64()
}
