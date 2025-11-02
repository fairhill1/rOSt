/// Thread management for rOSt
/// Implements preemptive multitasking with kernel threads

use alloc::boxed::Box;
use alloc::vec;
use core::arch::asm;

const STACK_SIZE: usize = 64 * 1024; // 64KB per thread

/// Thread context - saved/restored during context switch
/// Contains callee-saved registers per ARM64 calling convention
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ThreadContext {
    // Callee-saved registers (x19-x29)
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // Frame pointer
    pub x30: u64, // Link register (saved as part of callee-saved)

    // Stack pointer (saved separately)
    pub sp: u64,  // Stack pointer
}

impl ThreadContext {
    /// Create a new context for a thread entry point
    pub fn new(entry_point: fn(), stack_top: u64) -> Self {
        ThreadContext {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: entry_point as u64, // Thread starts here
            sp: stack_top,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

pub struct Thread {
    pub id: usize,
    pub context: ThreadContext,
    pub state: ThreadState,
    pub stack: Box<[u8]>,
    pub kernel_thread: bool, // true = kernel thread, false = user thread (future)
}

impl Thread {
    /// Create a new thread with the given entry point
    pub fn new(id: usize, entry_point: fn()) -> Self {
        // Allocate stack
        let stack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let stack_top = stack.as_ptr() as u64 + STACK_SIZE as u64;

        // Align stack to 16 bytes (ARM64 ABI requirement)
        let stack_top = stack_top & !0xF;

        // Initialize context
        let context = ThreadContext::new(entry_point, stack_top);

        Thread {
            id,
            context,
            state: ThreadState::Ready,
            stack,
            kernel_thread: true,
        }
    }
}

/// Switch from current thread to next thread
///
/// # Safety
/// This function directly manipulates CPU registers and stack pointers.
/// Must only be called by the scheduler with interrupts disabled.
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(
    _current: *mut ThreadContext,
    _next: *const ThreadContext,
) {
    core::arch::naked_asm!(
        // Save current thread context (callee-saved registers)
        "stp x19, x20, [x0, #0]",
        "stp x21, x22, [x0, #16]",
        "stp x23, x24, [x0, #32]",
        "stp x25, x26, [x0, #48]",
        "stp x27, x28, [x0, #64]",
        "stp x29, x30, [x0, #80]",  // x29 = FP, x30 = LR
        "mov x9, sp",
        "str x9, [x0, #96]",         // Save SP

        // Restore next thread context
        "ldp x19, x20, [x1, #0]",
        "ldp x21, x22, [x1, #16]",
        "ldp x23, x24, [x1, #32]",
        "ldp x25, x26, [x1, #48]",
        "ldp x27, x28, [x1, #64]",
        "ldp x29, x30, [x1, #80]",   // x29 = FP, x30 = LR
        "ldr x9, [x1, #96]",
        "mov sp, x9",                 // Restore SP

        // Return to next thread (jumps to LR)
        "ret",
    )
}

/// Jump to a thread for the first time (no previous context to save)
///
/// # Safety
/// This function directly manipulates CPU registers and stack pointers.
#[unsafe(naked)]
pub unsafe extern "C" fn jump_to_thread(_context: *const ThreadContext) {
    core::arch::naked_asm!(
        // Restore thread context
        "ldp x19, x20, [x0, #0]",
        "ldp x21, x22, [x0, #16]",
        "ldp x23, x24, [x0, #32]",
        "ldp x25, x26, [x0, #48]",
        "ldp x27, x28, [x0, #64]",
        "ldp x29, x30, [x0, #80]",
        "ldr x9, [x0, #96]",
        "mov sp, x9",
        "ret", // Jump to LR (entry point)
    )
}

/// Public API for thread management
use crate::kernel::scheduler::SCHEDULER;

/// Spawn a new kernel thread
pub fn spawn(entry_point: fn()) -> usize {
    SCHEDULER.lock().spawn(entry_point)
}

/// Yield CPU to another thread (cooperative scheduling)
pub fn yield_now() {
    // Get context switch info while holding the lock
    let switch_info = {
        let mut sched = SCHEDULER.lock();
        sched.yield_now()
    }; // Lock is dropped here!

    // Now perform context switch outside the lock
    if let Some((current_ptr, next_ptr, is_first)) = switch_info {
        unsafe {
            if is_first {
                jump_to_thread(next_ptr);
            } else {
                context_switch(current_ptr, next_ptr);
            }
        }
    }
}

/// Exit current thread
pub fn exit() -> ! {
    // Mark thread as terminated and get next switch
    let switch_info = {
        let mut sched = SCHEDULER.lock();
        if let Some(id) = sched.current_thread {
            if let Some(thread) = sched.threads.iter_mut().find(|t| t.id == id) {
                thread.state = ThreadState::Terminated;
                crate::kernel::uart_write_string(&alloc::format!("Thread {} exited\r\n", id));
            }
        }
        sched.schedule()
    }; // Lock is dropped here!

    // Perform context switch outside the lock
    if let Some((current_ptr, next_ptr, is_first)) = switch_info {
        unsafe {
            if is_first {
                jump_to_thread(next_ptr);
            } else {
                context_switch(current_ptr, next_ptr);
            }
        }
    }

    // Should never reach here
    loop {
        unsafe { asm!("wfi") }
    }
}
