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

/// Test user program - runs at EL0
#[no_mangle]
pub extern "C" fn user_test_program() -> ! {
    // Test 1: Print debug message
    print_debug("Hello from EL0 user space!");

    // Test 2: Get current time
    let time = get_time();
    print_debug("Got time from kernel!");

    // Test 3: Multiple syscalls
    for i in 0..5 {
        print_debug("Loop iteration from user space");
    }

    // Test 4: Exit
    print_debug("User program exiting...");
    exit(0);
}
