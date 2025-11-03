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

// Network syscall constants
const AF_INET: u32 = 2;
const SOCK_STREAM: u32 = 1;

/// Socket address structure (must match kernel definition)
#[repr(C)]
struct SockAddrIn {
    family: u16,
    port: u16,    // big endian
    addr: u32,    // big endian
    zero: [u8; 8],
}

impl SockAddrIn {
    fn new(ip: [u8; 4], port: u16) -> Self {
        Self {
            family: AF_INET as u16,
            port: port.to_be(),
            addr: u32::from_be_bytes(ip),
            zero: [0; 8],
        }
    }
}

/// sys_socket wrapper - creates a socket
fn sys_socket(domain: u32, socket_type: u32) -> i32 {
    unsafe {
        syscall(
            19, // SyscallNumber::Socket
            domain as u64,
            socket_type as u64,
            0
        ) as i32
    }
}

/// sys_connect wrapper - connects to remote address
fn sys_connect(sockfd: i32, addr: &SockAddrIn) -> i32 {
    unsafe {
        syscall(
            20, // SyscallNumber::Connect
            sockfd as u64,
            addr as *const _ as u64,
            0
        ) as i32
    }
}

/// sys_send wrapper - sends data on socket
fn sys_send(sockfd: i32, buf: &[u8]) -> isize {
    unsafe {
        syscall(
            21, // SyscallNumber::Send
            sockfd as u64,
            buf.as_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// sys_recv wrapper - receives data from socket
fn sys_recv(sockfd: i32, buf: &mut [u8]) -> isize {
    unsafe {
        syscall(
            22, // SyscallNumber::Recv
            sockfd as u64,
            buf.as_mut_ptr() as u64,
            buf.len() as u64
        ) as isize
    }
}

/// Test user program - runs at EL0
#[no_mangle]
pub extern "C" fn user_test_program() -> ! {
    print_debug("=== NETWORK SYSCALL TEST ===");
    print_debug("Starting HTTP client test...");

    // Test 1: Create TCP socket
    print_debug("Creating socket...");
    let sockfd = sys_socket(AF_INET, SOCK_STREAM);
    if sockfd < 0 {
        print_debug("ERROR: Failed to create socket");
        exit(1);
    }
    print_debug("Socket created successfully!");

    // Test 2: Connect to example.org (23.215.0.133:80)
    print_debug("Connecting to example.org (23.215.0.133:80)...");
    let addr = SockAddrIn::new([23, 215, 0, 133], 80);
    let result = sys_connect(sockfd, &addr);
    if result < 0 {
        print_debug("ERROR: Failed to connect");
        exit(1);
    }
    print_debug("Connected successfully!");

    // Test 3: Send HTTP GET request
    print_debug("Sending HTTP request...");
    let request = "GET / HTTP/1.0\r\nHost: example.org\r\n\r\n";
    let bytes_sent = sys_send(sockfd, request.as_bytes());
    if bytes_sent < 0 {
        print_debug("ERROR: Failed to send request");
        exit(1);
    }
    print_debug("Request sent!");

    // Test 4: Receive HTTP response
    print_debug("Receiving response...");
    let mut response_buf = [0u8; 1024];
    let mut total_received = 0;
    let start_time = get_time();

    // Try to receive for up to 5 seconds
    while get_time() - start_time < 5000 {
        let n = sys_recv(sockfd, &mut response_buf[total_received..]);
        if n > 0 {
            total_received += n as usize;
            print_debug("Received data chunk");

            // If we got data, check if we have full response
            if total_received > 4 {
                // Check for end of headers (double newline)
                let resp_slice = &response_buf[..total_received];
                if resp_slice.windows(4).any(|w| w == b"\r\n\r\n") {
                    print_debug("Got complete HTTP response!");
                    break;
                }
            }
        }
    }

    if total_received > 0 {
        print_debug("=== HTTP RESPONSE RECEIVED ===");
        // Print first 200 bytes of response
        let to_print = core::cmp::min(200, total_received);
        if let Ok(response_str) = core::str::from_utf8(&response_buf[..to_print]) {
            print_debug(response_str);
        }
        print_debug("=== SUCCESS ===");
    } else {
        print_debug("WARNING: No response received (timeout)");
    }

    print_debug("Network test complete - exiting");
    exit(0);
}
