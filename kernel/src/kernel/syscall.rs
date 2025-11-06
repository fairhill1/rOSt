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

    // Framebuffer operations
    FbInfo = 15,
    FbMap = 16,
    FbFlush = 17,

    // Input operations
    PollEvent = 18,

    // Network operations
    Socket = 19,
    Connect = 20,
    Send = 21,
    Recv = 22,
    Bind = 23,
    Listen = 24,
    Accept = 25,

    // IPC operations
    ShmCreate = 26,
    ShmMap = 27,
    ShmUnmap = 28,
    SendMessage = 29,
    RecvMessage = 30,

    // Drawing operations (TrueType font rendering)
    DrawText = 31,
    DrawRect = 32,

    // Scheduler operations
    Yield = 33,

    // Process management (for microkernel WM)
    SpawnElf = 34,
    Kill = 35,

    // Framebuffer operations (dirty regions)
    FbFlushRegion = 36,

    // IPC operations (extended)
    ShmMapFromProcess = 37,
    ShmDestroy = 38,
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
            15 => Some(Self::FbInfo),
            16 => Some(Self::FbMap),
            17 => Some(Self::FbFlush),
            18 => Some(Self::PollEvent),
            19 => Some(Self::Socket),
            20 => Some(Self::Connect),
            21 => Some(Self::Send),
            22 => Some(Self::Recv),
            23 => Some(Self::Bind),
            24 => Some(Self::Listen),
            25 => Some(Self::Accept),
            26 => Some(Self::ShmCreate),
            27 => Some(Self::ShmMap),
            28 => Some(Self::ShmUnmap),
            29 => Some(Self::SendMessage),
            30 => Some(Self::RecvMessage),
            31 => Some(Self::DrawText),
            32 => Some(Self::DrawRect),
            33 => Some(Self::Yield),
            34 => Some(Self::SpawnElf),
            35 => Some(Self::Kill),
            36 => Some(Self::FbFlushRegion),
            37 => Some(Self::ShmMapFromProcess),
            38 => Some(Self::ShmDestroy),
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

/// Framebuffer information returned to userspace
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FbInfo {
    pub width: u32,
    pub height: u32,
    pub stride: u32,        // pixels_per_scanline
    pub pixel_format: u32,  // 0=RGB, 1=BGR
}

/// Input event returned to userspace
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEventUser {
    pub event_type: u32,  // 0=None, 1=KeyPressed, 2=KeyReleased, 3=MouseMove, 4=MouseButton, 5=MouseWheel
    pub key: u8,
    pub modifiers: u8,
    pub button: u8,
    pub pressed: u8,      // boolean as u8
    pub x_delta: i8,
    pub y_delta: i8,
    pub wheel_delta: i8,
}

/// Socket types for sys_socket
pub const SOCK_STREAM: u32 = 1; // TCP
pub const SOCK_DGRAM: u32 = 2;  // UDP

/// Address family
pub const AF_INET: u32 = 2; // IPv4

/// Socket address for IPv4
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SockAddrIn {
    pub family: u16,      // AF_INET
    pub port: u16,        // Port in network byte order (big endian)
    pub addr: u32,        // IPv4 address in network byte order
    pub zero: [u8; 8],    // Padding
}

/// IPC message structure for inter-process communication
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct IpcMessage {
    pub sender_pid: u32,    // Process ID of sender
    pub data_len: u32,      // Length of message data
    pub data: [u8; 256],    // Message payload (max 256 bytes)
}

/// IPC constants
pub const MAX_MESSAGE_SIZE: usize = 256;
pub const MAX_MESSAGES_PER_PROCESS: usize = 16;
pub const MAX_SHM_REGIONS: usize = 32;

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
        None => {
            crate::kernel::uart_write_string("[SYSCALL] Invalid syscall number: ");
            if syscall_num < 100 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + (syscall_num / 10) as u8);
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + (syscall_num % 10) as u8);
                }
            }
            crate::kernel::uart_write_string("\r\n");
            return SyscallError::InvalidSyscall.as_i64();
        }
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
        SyscallNumber::FbInfo => sys_fb_info(args.arg0 as *mut FbInfo),
        SyscallNumber::FbMap => sys_fb_map(),
        SyscallNumber::FbFlush => sys_fb_flush(),
        SyscallNumber::PollEvent => sys_poll_event(args.arg0 as *mut InputEventUser),
        SyscallNumber::Socket => sys_socket(args.arg0 as u32, args.arg1 as u32),
        SyscallNumber::Connect => sys_connect(args.arg0 as i32, args.arg1 as *const SockAddrIn),
        SyscallNumber::Send => sys_send(args.arg0 as i32, args.arg1 as *const u8, args.arg2 as usize),
        SyscallNumber::Recv => sys_recv(args.arg0 as i32, args.arg1 as *mut u8, args.arg2 as usize),
        SyscallNumber::Bind => sys_bind(args.arg0 as i32, args.arg1 as *const SockAddrIn),
        SyscallNumber::Listen => sys_listen(args.arg0 as i32, args.arg1 as u32),
        SyscallNumber::Accept => sys_accept(args.arg0 as i32, args.arg1 as *mut SockAddrIn),
        SyscallNumber::ShmCreate => sys_shm_create(args.arg0 as usize),
        SyscallNumber::ShmMap => sys_shm_map(args.arg0 as i32),
        SyscallNumber::ShmUnmap => sys_shm_unmap(args.arg0 as i32),
        SyscallNumber::SendMessage => sys_send_message(args.arg0 as u32, args.arg1 as *const u8, args.arg2 as usize),
        SyscallNumber::RecvMessage => sys_recv_message(args.arg0 as *mut u8, args.arg1 as usize, args.arg2 as u32),
        SyscallNumber::DrawText => sys_draw_text(args.arg0 as i32, args.arg1 as i32, args.arg2 as *const u8, args.arg3 as usize, args.arg4 as u32),
        SyscallNumber::DrawRect => sys_draw_rect(args.arg0 as i32, args.arg1 as i32, args.arg2 as u32, args.arg3 as u32, args.arg4 as u32),
        SyscallNumber::Yield => sys_yield(),
        SyscallNumber::SpawnElf => sys_spawn_elf(args.arg0 as *const u8, args.arg1 as usize),
        SyscallNumber::Kill => sys_kill(args.arg0 as u64),
        SyscallNumber::FbFlushRegion => sys_fb_flush_region(args.arg0 as u32, args.arg1 as u32, args.arg2 as u32, args.arg3 as u32),
        SyscallNumber::ShmMapFromProcess => sys_shm_map_from_process(args.arg0 as usize, args.arg1 as i32),
        SyscallNumber::ShmDestroy => sys_shm_destroy(args.arg0 as i32),
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
    crate::kernel::uart_write_string("[SYSCALL] getpid() called\r\n");

    match get_current_process() {
        Some(pid) => {
            crate::kernel::uart_write_string("[SYSCALL] getpid() -> ");
            if pid < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + pid as u8);
                }
            }
            crate::kernel::uart_write_string("\r\n");
            pid as i64
        }
        None => 0, // Should never happen
    }
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

// Framebuffer syscalls

fn sys_fb_info(info_ptr: *mut FbInfo) -> i64 {
    if info_ptr.is_null() {
        return SyscallError::InvalidArgument.as_i64();
    }

    crate::kernel::uart_write_string("[SYSCALL] fb_info() called\r\n");

    // Get framebuffer info from kernel globals
    let fb_info = unsafe { crate::kernel::GPU_FRAMEBUFFER_INFO };

    match fb_info {
        Some(fb) => {
            // Create FbInfo struct to return to user
            let user_info = FbInfo {
                width: fb.width,
                height: fb.height,
                stride: fb.pixels_per_scanline,
                pixel_format: match fb.pixel_format {
                    crate::gui::framebuffer::PixelFormat::Rgb => 0,
                    crate::gui::framebuffer::PixelFormat::Bgr => 1,
                    _ => 0,
                },
            };

            // Copy to user buffer
            unsafe {
                core::ptr::write_volatile(info_ptr, user_info);
            }

            crate::kernel::uart_write_string("[SYSCALL] fb_info() -> success\r\n");
            0 // Success
        }
        None => {
            crate::kernel::uart_write_string("[SYSCALL] fb_info() -> no framebuffer\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
    }
}

fn sys_fb_map() -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] fb_map() called\r\n");

    // Get framebuffer base address from kernel globals
    let fb_info = unsafe { crate::kernel::GPU_FRAMEBUFFER_INFO };

    match fb_info {
        Some(fb) => {
            // Return the framebuffer base address as a signed i64
            // The address is already accessible in user page tables (0-4GB mapped)
            let addr = fb.base_address as i64;

            crate::kernel::uart_write_string("[SYSCALL] fb_map() -> 0x");
            // Simple hex print
            let hex_chars = b"0123456789ABCDEF";
            for i in (0..16).rev() {
                let digit = ((addr as u64) >> (i * 4)) & 0xF;
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[digit as usize]);
                }
            }
            crate::kernel::uart_write_string("\r\n");

            addr
        }
        None => {
            crate::kernel::uart_write_string("[SYSCALL] fb_map() -> no framebuffer\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
    }
}

fn sys_fb_flush() -> i64 {
    // Get GPU driver and flush the display
    let result = unsafe {
        match crate::kernel::GPU_DRIVER.as_mut() {
            Some(gpu) => {
                match gpu.flush_display() {
                    Ok(_) => 0,
                    Err(_) => SyscallError::InvalidArgument.as_i64()
                }
            }
            None => SyscallError::InvalidArgument.as_i64()
        }
    };

    result
}

// Input syscalls

fn sys_poll_event(event_ptr: *mut InputEventUser) -> i64 {
    if event_ptr.is_null() {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Get event from kernel input queue
    let kernel_event = crate::kernel::drivers::input_events::get_input_event();

    match kernel_event {
        Some(event) => {

            // Convert kernel InputEvent to user InputEventUser
            let user_event = match event {
                crate::kernel::drivers::input_events::InputEvent::KeyPressed { key, modifiers } => {
                    InputEventUser {
                        event_type: 1,
                        key,
                        modifiers,
                        button: 0,
                        pressed: 0,
                        x_delta: 0,
                        y_delta: 0,
                        wheel_delta: 0,
                    }
                }
                crate::kernel::drivers::input_events::InputEvent::KeyReleased { key, modifiers } => {
                    InputEventUser {
                        event_type: 2,
                        key,
                        modifiers,
                        button: 0,
                        pressed: 0,
                        x_delta: 0,
                        y_delta: 0,
                        wheel_delta: 0,
                    }
                }
                crate::kernel::drivers::input_events::InputEvent::MouseMove { x_delta, y_delta } => {
                    InputEventUser {
                        event_type: 3,
                        key: 0,
                        modifiers: 0,
                        button: 0,
                        pressed: 0,
                        x_delta,
                        y_delta,
                        wheel_delta: 0,
                    }
                }
                crate::kernel::drivers::input_events::InputEvent::MouseButton { button, pressed } => {
                    InputEventUser {
                        event_type: 4,
                        key: 0,
                        modifiers: 0,
                        button,
                        pressed: if pressed { 1 } else { 0 },
                        x_delta: 0,
                        y_delta: 0,
                        wheel_delta: 0,
                    }
                }
                crate::kernel::drivers::input_events::InputEvent::MouseWheel { delta } => {
                    InputEventUser {
                        event_type: 5,
                        key: 0,
                        modifiers: 0,
                        button: 0,
                        pressed: 0,
                        x_delta: 0,
                        y_delta: 0,
                        wheel_delta: delta,
                    }
                }
            };

            // Copy to user buffer
            unsafe {
                core::ptr::write_volatile(event_ptr, user_event);
            }

            1 // Event available
        }
        None => {
            // No event available - write empty event
            let empty_event = InputEventUser {
                event_type: 0,
                key: 0,
                modifiers: 0,
                button: 0,
                pressed: 0,
                x_delta: 0,
                y_delta: 0,
                wheel_delta: 0,
            };

            unsafe {
                core::ptr::write_volatile(event_ptr, empty_event);
            }

            0 // No event
        }
    }
}

// ============================================================================
// Network syscalls
// ============================================================================

/// Socket descriptor management for per-process network sockets
/// Similar to FileDescriptorTable but for network sockets
pub struct SocketDescriptorTable {
    sockets: [Option<SocketDescriptor>; MAX_SOCKETS_PER_PROCESS],
}

const MAX_SOCKETS_PER_PROCESS: usize = 32;

#[derive(Clone, Copy)]
struct SocketDescriptor {
    socket_type: u32,  // SOCK_STREAM or SOCK_DGRAM
    handle: smoltcp::iface::SocketHandle,  // Socket handle from smoltcp
    connected: bool,
}

impl SocketDescriptorTable {
    pub const fn new() -> Self {
        Self {
            sockets: [const { None }; MAX_SOCKETS_PER_PROCESS],
        }
    }

    /// Allocate a new socket descriptor
    pub fn alloc(&mut self, socket_type: u32, handle: smoltcp::iface::SocketHandle) -> Option<i32> {
        for (i, slot) in self.sockets.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(SocketDescriptor {
                    socket_type,
                    handle,
                    connected: false,
                });
                return Some(i as i32);
            }
        }
        None
    }

    /// Get socket descriptor
    pub fn get(&self, sockfd: i32) -> Option<&SocketDescriptor> {
        if sockfd < 0 || sockfd >= MAX_SOCKETS_PER_PROCESS as i32 {
            return None;
        }
        self.sockets[sockfd as usize].as_ref()
    }

    /// Get mutable socket descriptor
    pub fn get_mut(&mut self, sockfd: i32) -> Option<&mut SocketDescriptor> {
        if sockfd < 0 || sockfd >= MAX_SOCKETS_PER_PROCESS as i32 {
            return None;
        }
        self.sockets[sockfd as usize].as_mut()
    }

    /// Close a socket descriptor
    pub fn close(&mut self, sockfd: i32) -> bool {
        if sockfd < 0 || sockfd >= MAX_SOCKETS_PER_PROCESS as i32 {
            return false;
        }
        if self.sockets[sockfd as usize].is_some() {
            self.sockets[sockfd as usize] = None;
            true
        } else {
            false
        }
    }
}

/// Access current process socket descriptor table
fn with_current_process_sockets<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut SocketDescriptorTable) -> R,
{
    let process_id = get_current_process()?;
    crate::kernel::thread::with_process_mut(process_id, |process| {
        f(&mut process.socket_descriptors)
    })
}

fn sys_socket(domain: u32, socket_type: u32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] socket(domain=");
    crate::kernel::uart_write_string(if domain == AF_INET { "AF_INET" } else { "?" });
    crate::kernel::uart_write_string(", type=");
    crate::kernel::uart_write_string(match socket_type {
        SOCK_STREAM => "SOCK_STREAM",
        SOCK_DGRAM => "SOCK_DGRAM",
        _ => "?",
    });
    crate::kernel::uart_write_string(")\r\n");

    // Only support AF_INET (IPv4)
    if domain != AF_INET {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Create socket in network stack
    let network_stack = unsafe { crate::kernel::NETWORK_STACK.as_mut() };
    let stack = match network_stack {
        Some(s) => s,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    let handle = match socket_type {
        SOCK_STREAM => stack.create_tcp_socket(),
        SOCK_DGRAM => stack.create_udp_socket(),
        _ => return SyscallError::InvalidArgument.as_i64(),
    };

    // Allocate socket descriptor for this process
    let sockfd = with_current_process_sockets(|sockets| {
        sockets.alloc(socket_type, handle)
    });

    match sockfd {
        Some(Some(fd)) => {
            crate::kernel::uart_write_string("[SYSCALL] socket() -> sockfd=");
            if fd < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fd as u8);
                }
            }
            crate::kernel::uart_write_string("\r\n");
            fd as i64
        }
        _ => SyscallError::OutOfMemory.as_i64(),
    }
}

fn sys_connect(sockfd: i32, addr: *const SockAddrIn) -> i64 {
    if addr.is_null() {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Read address from userspace
    let sockaddr = unsafe { core::ptr::read_volatile(addr) };

    crate::kernel::uart_write_string("[SYSCALL] connect(sockfd=");
    if sockfd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + sockfd as u8);
        }
    }
    crate::kernel::uart_write_string(")\r\n");

    // Get socket descriptor
    let (socket_type, handle) = match with_current_process_sockets(|sockets| {
        sockets.get(sockfd).map(|desc| (desc.socket_type, desc.handle))
    }) {
        Some(Some(info)) => info,
        _ => return SyscallError::BadFileDescriptor.as_i64(),
    };

    // Only TCP sockets can connect
    if socket_type != SOCK_STREAM {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Convert network byte order to host byte order
    let port = u16::from_be(sockaddr.port);
    let ip_addr = u32::from_be(sockaddr.addr);
    let ip_bytes = ip_addr.to_be_bytes();

    // Connect TCP socket
    let network_stack = unsafe { crate::kernel::NETWORK_STACK.as_mut() };
    let stack = match network_stack {
        Some(s) => s,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    let remote_endpoint = smoltcp::wire::IpEndpoint::new(
        smoltcp::wire::IpAddress::v4(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]),
        port,
    );

    // Use ephemeral port for local endpoint
    let local_port = 49152 + (sockfd as u16 * 100);

    match stack.tcp_connect(handle, remote_endpoint, local_port) {
        Ok(_) => {
            crate::kernel::uart_write_string("[SYSCALL] connect() -> initiated (non-blocking)\r\n");

            // Mark socket as "connected" (connection initiated, not established yet)
            with_current_process_sockets(|sockets| {
                if let Some(desc) = sockets.get_mut(sockfd) {
                    desc.connected = true;
                }
            });

            // Return immediately - connection will complete asynchronously
            // App must poll/retry send until it works
            0
        }
        Err(_) => {
            crate::kernel::uart_write_string("[SYSCALL] connect() -> failed\r\n");
            SyscallError::InvalidArgument.as_i64()
        }
    }
}

fn sys_send(sockfd: i32, buf: *const u8, len: usize) -> i64 {
    if buf.is_null() || len == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    crate::kernel::uart_write_string("[SYSCALL] send(sockfd=");
    if sockfd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + sockfd as u8);
        }
    }
    crate::kernel::uart_write_string(", len=");
    crate::kernel::uart_write_string("\r\n");

    // Get socket descriptor
    let (socket_type, handle) = match with_current_process_sockets(|sockets| {
        sockets.get(sockfd).map(|desc| (desc.socket_type, desc.handle))
    }) {
        Some(Some(info)) => info,
        _ => return SyscallError::BadFileDescriptor.as_i64(),
    };

    // Only TCP sockets support send
    if socket_type != SOCK_STREAM {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Copy data from userspace
    let data = unsafe { core::slice::from_raw_parts(buf, len) };

    // Send data through network stack
    let network_stack = unsafe { crate::kernel::NETWORK_STACK.as_mut() };
    let stack = match network_stack {
        Some(s) => s,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Retry send with polling until socket is ready (up to 5 seconds)
    let start_time = crate::kernel::get_time_ms();
    let mut bytes_sent = 0;

    while bytes_sent == 0 && crate::kernel::get_time_ms() - start_time < 5000 {
        // Add RX buffers and poll
        stack.add_receive_buffers(8).ok();
        stack.poll();

        // Try to send
        bytes_sent = stack.with_tcp_socket(handle, |socket| {
            match socket.send_slice(data) {
                Ok(n) => {
                    if n > 0 {
                        crate::kernel::uart_write_string("[SYSCALL] send() -> sent ");
                        if n < 10 {
                            unsafe {
                                core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + n as u8);
                            }
                        }
                        crate::kernel::uart_write_string(" bytes\r\n");
                    }
                    n
                }
                Err(_) => 0,
            }
        });

        // Small delay if send failed
        if bytes_sent == 0 {
            for _ in 0..1000 {
                unsafe { core::arch::asm!("nop"); }
            }
        }
    }

    if bytes_sent == 0 {
        crate::kernel::uart_write_string("[SYSCALL] send() -> timeout waiting for socket ready\r\n");
        return SyscallError::InvalidArgument.as_i64();
    }

    // Poll network stack multiple times to actually transmit packets
    for _ in 0..10 {
        stack.poll();
    }

    bytes_sent as i64
}

fn sys_recv(sockfd: i32, buf: *mut u8, len: usize) -> i64 {
    if buf.is_null() || len == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    crate::kernel::uart_write_string("[SYSCALL] recv(sockfd=");
    if sockfd < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + sockfd as u8);
        }
    }
    crate::kernel::uart_write_string(")\r\n");

    // Get socket descriptor
    let (socket_type, handle) = match with_current_process_sockets(|sockets| {
        sockets.get(sockfd).map(|desc| (desc.socket_type, desc.handle))
    }) {
        Some(Some(info)) => info,
        _ => return SyscallError::BadFileDescriptor.as_i64(),
    };

    // Only TCP sockets support recv
    if socket_type != SOCK_STREAM {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Poll network stack first to receive packets
    let network_stack = unsafe { crate::kernel::NETWORK_STACK.as_mut() };
    let stack = match network_stack {
        Some(s) => s,
        None => return SyscallError::InvalidArgument.as_i64(),
    };

    // Poll multiple times to ensure packets are processed
    for _ in 0..5 {
        stack.poll();
    }

    // Receive data from network stack
    let bytes_received = stack.with_tcp_socket(handle, |socket| {
        if socket.can_recv() {
            match socket.recv_slice(unsafe { core::slice::from_raw_parts_mut(buf, len) }) {
                Ok(n) => {
                    if n > 0 {
                        crate::kernel::uart_write_string("[SYSCALL] recv() -> received ");
                        if n < 10 {
                            unsafe {
                                core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + n as u8);
                            }
                        }
                        crate::kernel::uart_write_string(" bytes\r\n");
                    }
                    n
                }
                Err(_) => 0,
            }
        } else {
            0
        }
    });

    bytes_received as i64
}

fn sys_bind(sockfd: i32, addr: *const SockAddrIn) -> i64 {
    if addr.is_null() {
        return SyscallError::InvalidArgument.as_i64();
    }

    crate::kernel::uart_write_string("[SYSCALL] bind() - not implemented\r\n");

    // TODO: Implement bind for server sockets
    // For now, return not implemented
    SyscallError::NotImplemented.as_i64()
}

fn sys_listen(sockfd: i32, backlog: u32) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] listen() - not implemented\r\n");

    // TODO: Implement listen for server sockets
    // For now, return not implemented
    SyscallError::NotImplemented.as_i64()
}

fn sys_accept(sockfd: i32, addr: *mut SockAddrIn) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] accept() - not implemented\r\n");

    // TODO: Implement accept for server sockets
    // For now, return not implemented
    SyscallError::NotImplemented.as_i64()
}

// ============================================================================
// IPC syscalls
// ============================================================================

fn sys_shm_create(size: usize) -> i64 {
    crate::kernel::syscall_ipc::sys_shm_create(size)
}

fn sys_shm_map(shm_id: i32) -> i64 {
    crate::kernel::syscall_ipc::sys_shm_map(shm_id)
}

fn sys_shm_map_from_process(process_id: usize, shm_id: i32) -> i64 {
    crate::kernel::syscall_ipc::sys_shm_map_from_process(process_id, shm_id)
}

fn sys_shm_destroy(shm_id: i32) -> i64 {
    crate::kernel::syscall_ipc::sys_shm_destroy(shm_id)
}

fn sys_shm_unmap(shm_id: i32) -> i64 {
    crate::kernel::syscall_ipc::sys_shm_unmap(shm_id)
}

fn sys_send_message(dest_pid: u32, data: *const u8, len: usize) -> i64 {
    crate::kernel::syscall_ipc::sys_send_message(dest_pid, data, len)
}

fn sys_recv_message(buf: *mut u8, len: usize, timeout_ms: u32) -> i64 {
    crate::kernel::syscall_ipc::sys_recv_message(buf, len, timeout_ms)
}

// ============================================================================
// Drawing syscalls (TrueType font rendering)
// ============================================================================

fn sys_draw_text(x: i32, y: i32, text_ptr: *const u8, text_len: usize, color: u32) -> i64 {
    if text_ptr.is_null() || text_len == 0 {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Read text from userspace
    let text_slice = unsafe { core::slice::from_raw_parts(text_ptr, text_len) };
    let text = match core::str::from_utf8(text_slice) {
        Ok(s) => s,
        Err(_) => return SyscallError::InvalidArgument.as_i64(),
    };

    // Draw text using kernel framebuffer module (with TrueType font)
    crate::gui::framebuffer::draw_string(x as u32, y as u32, text, color);

    0 // Success
}

fn sys_draw_rect(x: i32, y: i32, width: u32, height: u32, color: u32) -> i64 {
    // For large rectangles (like full screen clears), use efficient bulk write
    if width > 100 && height > 100 {

        // Get direct framebuffer access
        let fb_info = unsafe { crate::kernel::GPU_FRAMEBUFFER_INFO };
        if let Some(fb) = fb_info {
            let fb_ptr = fb.base_address as *mut u32;
            let stride = fb.pixels_per_scanline as usize;

            // Fill rectangle efficiently
            for dy in 0..height as usize {
                let screen_y = y as usize + dy;
                if screen_y < fb.height as usize {
                    let row_start = screen_y * stride + x as usize;
                    unsafe {
                        for dx in 0..width as usize {
                            *fb_ptr.add(row_start + dx) = color;
                        }
                    }
                }
            }
        }
    } else {
        // Small rectangles - use pixel-by-pixel (for borders, etc.)
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && py >= 0 {
                    crate::gui::framebuffer::draw_pixel(px as u32, py as u32, color);
                }
            }
        }
    }

    0 // Success
}

// ============================================================================
// Scheduler syscalls
// ============================================================================

fn sys_yield() -> i64 {
    // For user threads: context switch will save the syscall return path
    // When resumed, execution continues here and returns to ERET
    crate::kernel::thread::yield_now();
    0 // Success
}

// ============================================================================
// Process management syscalls (for microkernel)
// ============================================================================

fn sys_spawn_elf(path: *const u8, path_len: usize) -> i64 {
    if path.is_null() || path_len == 0 || path_len > 256 {
        return SyscallError::InvalidArgument.as_i64();
    }

    // Read path from userspace
    let path_slice = unsafe { core::slice::from_raw_parts(path, path_len) };
    let path_str = match core::str::from_utf8(path_slice) {
        Ok(s) => s,
        Err(_) => return SyscallError::InvalidArgument.as_i64(),
    };

    crate::kernel::uart_write_string("[SYSCALL] spawn_elf(\"");
    crate::kernel::uart_write_string(path_str);
    crate::kernel::uart_write_string("\") called\r\n");

    // For now, we only support spawning embedded ELFs
    // Later can add filesystem support
    let elf_data = match path_str {
        "/bin/terminal" | "terminal" => crate::kernel::embedded_apps::TERMINAL_ELF,
        "/bin/editor" | "editor" => {
            crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> editor not yet ported\r\n");
            return SyscallError::NotImplemented.as_i64();
        }
        "/bin/browser" | "browser" => {
            crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> browser not yet ported\r\n");
            return SyscallError::NotImplemented.as_i64();
        }
        "/bin/files" | "files" => {
            crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> files not yet ported\r\n");
            return SyscallError::NotImplemented.as_i64();
        }
        "/bin/snake" | "snake" => {
            crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> snake not yet ported\r\n");
            return SyscallError::NotImplemented.as_i64();
        }
        _ => {
            crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> file not found\r\n");
            return SyscallError::FileNotFound.as_i64();
        }
    };

    // Load and spawn the ELF
    let pid = crate::kernel::elf_loader::load_elf_and_spawn(elf_data);
    crate::kernel::uart_write_string("[SYSCALL] spawn_elf() -> PID ");
    if pid < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + pid as u8);
        }
    }
    crate::kernel::uart_write_string("\r\n");

    pid as i64
}

pub fn sys_kill(pid: u64) -> i64 {
    crate::kernel::uart_write_string("[SYSCALL] kill(pid=");
    if pid < 10 {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + pid as u8);
        }
    }
    crate::kernel::uart_write_string(") called\r\n");

    // Check that we're not trying to kill critical system processes
    // PID 0: Kernel idle thread
    // PID 1: Shell (kernel-mode, started first)
    // PID 2: Window manager (userspace, but critical)
    // PID 3+: Userspace apps (can be killed)
    if pid < 3 {
        crate::kernel::uart_write_string("[SYSCALL] kill() -> cannot kill kernel/critical process\r\n");
        return SyscallError::PermissionDenied.as_i64();
    }

    // Terminate the process
    crate::kernel::thread::terminate_process(pid as usize);

    crate::kernel::uart_write_string("[SYSCALL] kill() -> success\r\n");
    0 // Success
}

fn sys_fb_flush_region(x: u32, y: u32, width: u32, height: u32) -> i64 {
    // For now, just flush the entire display
    // TODO: Implement dirty region tracking for performance
    let result = unsafe {
        match crate::kernel::GPU_DRIVER.as_mut() {
            Some(gpu) => {
                match gpu.flush_display() {
                    Ok(_) => 0,
                    Err(_) => SyscallError::InvalidArgument.as_i64()
                }
            }
            None => SyscallError::InvalidArgument.as_i64()
        }
    };

    result
}
