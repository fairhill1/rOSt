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

/// Framebuffer info structure (must match kernel definition)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct FbInfo {
    width: u32,
    height: u32,
    stride: u32,
    pixel_format: u32,
}

/// Input event structure (must match kernel definition)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct InputEvent {
    event_type: u32,  // 0=None, 1=KeyPressed, 2=KeyReleased, 3=MouseMove, 4=MouseButton, 5=MouseWheel
    key: u8,
    modifiers: u8,
    button: u8,
    pressed: u8,
    x_delta: i8,
    y_delta: i8,
    wheel_delta: i8,
}

/// sys_fb_info wrapper
fn sys_fb_info() -> Option<FbInfo> {
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

/// sys_fb_map wrapper - returns framebuffer address
fn sys_fb_map() -> Option<*mut u32> {
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

/// sys_fb_flush wrapper - flushes framebuffer to display
fn sys_fb_flush() -> i32 {
    unsafe {
        syscall(
            17, // SyscallNumber::FbFlush
            0,
            0,
            0
        ) as i32
    }
}

/// sys_poll_event wrapper - polls for input events
fn sys_poll_event() -> Option<InputEvent> {
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

    // Test 2: Get framebuffer info
    print_debug("Getting framebuffer info...");
    let fb_info = match sys_fb_info() {
        Some(info) => {
            print_debug("Framebuffer info received!");
            info
        }
        None => {
            print_debug("Failed to get framebuffer info");
            exit(1);
        }
    };

    // Test 3: Map framebuffer
    print_debug("Mapping framebuffer...");
    let fb_ptr = match sys_fb_map() {
        Some(ptr) => {
            print_debug("Framebuffer mapped successfully!");
            ptr
        }
        None => {
            print_debug("Failed to map framebuffer");
            exit(1);
        }
    };

    // Test 4: Poll for input events continuously
    print_debug("=== INPUT EVENT TEST ===");
    print_debug("Move mouse or press keys NOW!");
    print_debug("Polling for 3 seconds...");

    let start_time = get_time();
    let mut event_count = 0;
    let mut key_count = 0;
    let mut mouse_move_count = 0;
    let mut mouse_button_count = 0;

    // Poll rapidly for 3 seconds
    while get_time() - start_time < 3000 {
        if let Some(event) = sys_poll_event() {
            event_count += 1;
            match event.event_type {
                1 => {
                    key_count += 1;
                    print_debug("Event: Key pressed");
                }
                2 => print_debug("Event: Key released"),
                3 => {
                    mouse_move_count += 1;
                    print_debug("Event: Mouse moved");
                }
                4 => {
                    mouse_button_count += 1;
                    if event.pressed != 0 {
                        print_debug("Event: Mouse button pressed");
                    } else {
                        print_debug("Event: Mouse button released");
                    }
                }
                5 => print_debug("Event: Mouse wheel"),
                _ => {}
            }
        }
    }

    print_debug("=== RESULTS ===");
    if event_count > 0 {
        print_debug("SUCCESS: Input events detected!");
    } else {
        print_debug("WARNING: No events detected (try moving mouse/keys)");
    }

    print_debug("Test complete - exiting");
    exit(0);
}
