// Test user-space code that runs at EL0

use core::arch::asm;

/// Syscall wrapper - invokes SVC instruction to trap to EL1
#[inline(always)]
unsafe fn syscall(num: u64, arg0: u64, arg1: u64, arg2: u64) -> i64 {
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

/// sys_print_debug wrapper
fn print_debug(msg: &str) {
    unsafe {
        syscall(
            14, // SyscallNumber::PrintDebug
            msg.as_ptr() as u64,
            msg.len() as u64,
            0
        );
    }
}

/// sys_gettime wrapper
fn get_time() -> i64 {
    unsafe {
        syscall(
            12, // SyscallNumber::GetTime
            0,
            0,
            0
        )
    }
}

/// sys_exit wrapper
fn exit(code: i32) -> ! {
    unsafe {
        syscall(
            8, // SyscallNumber::Exit
            code as u64,
            0,
            0
        );
    }
    // Should never reach here
    loop {
        unsafe { asm!("wfe"); }
    }
}

/// sys_open wrapper
fn sys_open(path: &str, flags: u32) -> i32 {
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

/// sys_read wrapper
fn sys_read(fd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall(
            0, // SyscallNumber::Read
            fd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// sys_write wrapper (to stdout)
fn sys_write(fd: i32, buf: &[u8]) -> isize {
    unsafe {
        syscall(
            1, // SyscallNumber::Write
            fd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// sys_close wrapper
fn sys_close(fd: i32) -> i32 {
    unsafe {
        syscall(
            3, // SyscallNumber::Close
            fd as u64,
            0,
            0
        ) as i32
    }
}

/// Test user program - runs at EL0
#[no_mangle]
pub extern "C" fn user_test_program() -> ! {
    // DEBUG: Write directly to UART as first instruction
    unsafe {
        core::ptr::write_volatile(0x09000000 as *mut u8, b'U');
        core::ptr::write_volatile(0x09000000 as *mut u8, b'S');
        core::ptr::write_volatile(0x09000000 as *mut u8, b'R');
        core::ptr::write_volatile(0x09000000 as *mut u8, b'\n');
    }

    // Test 1: Print debug message
    print_debug("Hello from EL0 user space!");

    // Test 2: Get current time
    let time = get_time();
    print_debug("Got time from kernel!");

    // Test 3: File I/O - read "welcome" file
    print_debug("Testing file I/O syscalls...");

    // Open file for reading (flags=1 = READ)
    let fd = sys_open("welcome", 1);
    if fd >= 0 {
        print_debug("File opened successfully!");

        // Read file contents
        let mut buffer = [0u8; 512];
        let bytes_read = sys_read(fd, &mut buffer);

        if bytes_read > 0 {
            print_debug("Read file contents:");
            // Write to stdout (fd=1)
            sys_write(1, &buffer[..bytes_read as usize]);
            print_debug("");  // Newline
        } else {
            print_debug("Failed to read file");
        }

        // Close file
        sys_close(fd);
        print_debug("File closed");
    } else {
        print_debug("Failed to open file (does 'welcome' exist?)");
    }

    // Test 4: Multiple syscalls
    for i in 0..3 {
        print_debug("Loop iteration from user space");
    }

    // Test 5: Exit
    print_debug("User program exiting...");
    exit(0);
}
