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
    if buf.is_null() || count == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    crate::kernel::uart_write_string("[SYSCALL] read(fd=");
    if fd >= 0 && fd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fd as u8);
        }
    }
    crate::kernel::uart_write_string(") called\r\n");

    // Get filename and current offset from FD table
    let (filename, offset) = match with_current_process_fds(|fds| {
        fds.get(fd).map(|fd_entry| (fd_entry.file_name.clone(), fd_entry.offset))
    }) {
        Some(Some(info)) => info,
        _ => return SyscallError::BadFileDescriptor.as_i64(),
    };

    // Access filesystem
    use crate::system::fs::filesystem::SimpleFilesystem;

    let block_devices = unsafe { crate::kernel::BLOCK_DEVICES.as_mut() };
    let device = match block_devices {
        Some(devs) if !devs.is_empty() => &mut devs[0],
        _ => return SyscallError::FileNotFound.as_i64(),
    };

    let fs = match SimpleFilesystem::mount(device) {
        Ok(fs) => fs,
        Err(_) => return SyscallError::FileNotFound.as_i64(),
    };

    // Allocate buffer to read entire file (SimpleFS doesn't support partial reads)
    // TODO: Optimize this to only read what's needed
    let mut file_buffer = alloc::vec![0u8; 1024 * 1024]; // 1MB max file size

    let bytes_read = match fs.read_file(device, &filename, &mut file_buffer) {
        Ok(size) => size,
        Err(_) => return SyscallError::FileNotFound.as_i64(),
    };

    // Calculate how much to copy from offset
    if offset >= bytes_read {
        return 0; // EOF
    }

    let available = bytes_read - offset;
    let to_copy = core::cmp::min(count, available);

    // Copy to user buffer
    unsafe {
        core::ptr::copy_nonoverlapping(
            file_buffer.as_ptr().add(offset),
            buf,
            to_copy
        );
    }

    // Update offset in FD table
    with_current_process_fds(|fds| {
        if let Some(fd_entry) = fds.get_mut(fd) {
            fd_entry.offset += to_copy;
        }
    });

    crate::kernel::uart_write_string("[SYSCALL] read() -> success\r\n");

    to_copy as i64
}

fn sys_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    if buf.is_null() || count == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Special case: stdout (fd=1) writes to UART
    if fd == 1 {
        let slice = unsafe { core::slice::from_raw_parts(buf, count) };
        if let Ok(s) = core::str::from_utf8(slice) {
            crate::kernel::uart_write_string(s);
            return count as i64;
        }
        return SyscallError::InvalidArgument.as_i64();
    }

    // File write: SimpleFS requires writing entire file at once
    // For now, we'll read existing content, modify it, and write back
    // This is inefficient but matches SimpleFS's API

    crate::kernel::uart_write_string("[SYSCALL] write(fd=");
    if fd >= 0 && fd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fd as u8);
        }
    }
    crate::kernel::uart_write_string(") called\r\n");

    // Get filename and offset
    let (filename, offset, flags) = match with_current_process_fds(|fds| {
        fds.get(fd).map(|fd_entry| (fd_entry.file_name.clone(), fd_entry.offset, fd_entry.flags))
    }) {
        Some(Some(info)) => info,
        _ => return SyscallError::BadFileDescriptor.as_i64(),
    };

    // Check write permission
    use crate::kernel::syscall::OpenFlags;
    if (flags & OpenFlags::WRITE.bits()) == 0 {
        return SyscallError::PermissionDenied.as_i64();
    }

    // Access filesystem
    use crate::system::fs::filesystem::SimpleFilesystem;

    let block_devices = unsafe { crate::kernel::BLOCK_DEVICES.as_mut() };
    let device = match block_devices {
        Some(devs) if !devs.is_empty() => &mut devs[0],
        _ => return SyscallError::FileNotFound.as_i64(),
    };

    let mut fs = match SimpleFilesystem::mount(device) {
        Ok(fs) => fs,
        Err(_) => return SyscallError::FileNotFound.as_i64(),
    };

    // Read existing file content
    let mut file_buffer = alloc::vec![0u8; 1024 * 1024];
    let existing_size = fs.read_file(device, &filename, &mut file_buffer).unwrap_or(0);

    // Calculate new size after write
    let new_size = core::cmp::max(existing_size, offset + count);
    if new_size > file_buffer.len() {
        return SyscallError::OutOfMemory.as_i64(); // File too large
    }

    // Copy new data at offset
    unsafe {
        core::ptr::copy_nonoverlapping(
            buf,
            file_buffer.as_mut_ptr().add(offset),
            count
        );
    }

    // Write entire file back
    match fs.write_file(device, &filename, &file_buffer[..new_size]) {
        Ok(_) => {
            // Update offset
            with_current_process_fds(|fds| {
                if let Some(fd_entry) = fds.get_mut(fd) {
                    fd_entry.offset += count;
                }
            });

            crate::kernel::uart_write_string("[SYSCALL] write() -> success\r\n");

            count as i64
        }
        Err(_) => SyscallError::FileNotFound.as_i64(),
    }
}

fn sys_open(path: *const u8, flags: u32) -> i64 {
    // Read filename from user space
    let filename = match read_user_string(path, 256) {
        Some(name) => name,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    crate::kernel::uart_write_string("[SYSCALL] open(\"");
    crate::kernel::uart_write_string(&filename);
    crate::kernel::uart_write_string("\") called\r\n");

    // Check if file exists in SimpleFS
    // We need to mount the filesystem to check
    use crate::system::fs::filesystem::SimpleFilesystem;

    let block_devices = unsafe { crate::kernel::BLOCK_DEVICES.as_mut() };
    let device = match block_devices {
        Some(devs) if !devs.is_empty() => &mut devs[0],
        _ => return SyscallError::FileNotFound.as_i64(),
    };

    // Try to mount filesystem
    let fs = match SimpleFilesystem::mount(device) {
        Ok(fs) => fs,
        Err(_) => return SyscallError::FileNotFound.as_i64(),
    };

    // Check if file exists
    let file_exists = fs.list_files().iter().any(|f| f.get_name() == filename);

    if !file_exists {
        return SyscallError::FileNotFound.as_i64();
    }

    // Allocate file descriptor
    let fd = with_current_process_fds(|fds| {
        fds.alloc(filename.clone(), flags)
    });

    match fd {
        Some(Some(fd_num)) => {
            crate::kernel::uart_write_string("[SYSCALL] open() -> fd=");
            // Print FD number
            if fd_num < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fd_num as u8);
                }
            }
            crate::kernel::uart_write_string("\r\n");
            fd_num as i64
        }
        _ => SyscallError::OutOfMemory.as_i64(), // FD table full
    }
}

fn sys_close(fd: i32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] close(fd=");
    if fd >= 0 && fd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fd as u8);
        }
    }
    crate::kernel::uart_write_string(") called\r\n");

    let result = with_current_process_fds(|fds| {
        fds.close(fd)
    });

    match result {
        Some(true) => 0, // Success
        Some(false) => SyscallError::BadFileDescriptor.as_i64(),
        None => SyscallError::BadFileDescriptor.as_i64(),
    }
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
    crate::kernel::uart_write_string("[SYSCALL] User program terminated - returning to kernel\r\n");

    // Special return value indicates process termination
    // This is a sentinel value that the syscall handler will check
    0xDEADBEEF_DEADBEEF_u64 as i64
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

// ============================================================================
// Helper functions for syscall implementation
// ============================================================================

/// Get the current process from the scheduler
fn get_current_process() -> Option<usize> {
    let scheduler = crate::kernel::scheduler::SCHEDULER.lock();
    scheduler.current_thread.and_then(|thread_id| {
        scheduler.threads.iter()
            .find(|t| t.id == thread_id)
            .map(|t| t.process_id)
    })
}

/// Safely read a C-style null-terminated string from user space
/// Returns None if string is invalid or too long
fn read_user_string(ptr: *const u8, max_len: usize) -> Option<alloc::string::String> {
    if ptr.is_null() {
        return None;
    }

    let mut bytes = alloc::vec::Vec::new();
    for i in 0..max_len {
        let byte = unsafe { ptr.add(i).read_volatile() };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }

    alloc::string::String::from_utf8(bytes).ok()
}

/// Get mutable access to the current process's file descriptor table
fn with_current_process_fds<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut crate::kernel::filedesc::FileDescriptorTable) -> R,
{
    let process_id = get_current_process()?;
    crate::kernel::thread::with_process_mut(process_id, |process| {
        f(&mut process.file_descriptors)
    })
}
