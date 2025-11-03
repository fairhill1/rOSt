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

    // Test 4: Draw colored rectangles to screen
    print_debug("Drawing to framebuffer...");

    unsafe {
        let stride = fb_info.stride;

        // Test: Write and read back a single pixel
        let test_offset = (100 * stride + 100) as isize;
        core::ptr::write_volatile(fb_ptr.offset(test_offset), 0x00FF0000);
        let readback = core::ptr::read_volatile(fb_ptr.offset(test_offset));
        if readback == 0x00FF0000 {
            print_debug("Pixel write/read verified!");
        } else {
            print_debug("Pixel readback FAILED - writes not working!");
        }

        // Draw red rectangle (100x100) at (100, 100)
        for y in 100..200 {
            for x in 100..200 {
                let offset = (y * stride + x) as isize;
                core::ptr::write_volatile(fb_ptr.offset(offset), 0x00FF0000); // Red
            }
        }

        // Draw green rectangle (100x100) at (250, 100)
        for y in 100..200 {
            for x in 250..350 {
                let offset = (y * stride + x) as isize;
                core::ptr::write_volatile(fb_ptr.offset(offset), 0x0000FF00); // Green
            }
        }

        // Draw blue rectangle (100x100) at (400, 100)
        for y in 100..200 {
            for x in 400..500 {
                let offset = (y * stride + x) as isize;
                core::ptr::write_volatile(fb_ptr.offset(offset), 0x000000FF); // Blue
            }
        }
    }

    print_debug("Drawing complete!");

    // Flush framebuffer to display
    print_debug("Flushing framebuffer...");
    if sys_fb_flush() == 0 {
        print_debug("Flush successful!");
    } else {
        print_debug("Flush failed!");
    }

    // Wait so we can see the rectangles before shell redraws
    print_debug("Rectangles visible - waiting 10 seconds...");
    let start_time = get_time();
    let target_time = start_time + 10000; // 10 seconds in milliseconds

    loop {
        let current_time = get_time();
        if current_time >= target_time {
            break;
        }
        // Yield CPU occasionally to be cooperative
        unsafe { core::arch::asm!("wfe"); }
    }

    // Test 5: Exit
    print_debug("User program exiting...");
    exit(0);
}
