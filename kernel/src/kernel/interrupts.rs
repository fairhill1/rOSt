// ARM64 exception handling and interrupt controller

use aarch64_cpu::{
    asm::barrier,
    registers::*,
};
use core::arch::{asm, global_asm};

/// Get the current exception level (EL0, EL1, EL2, or EL3)
pub fn get_current_exception_level() -> u8 {
    let current_el = CurrentEL.get();
    // CurrentEL register stores level in bits [3:2]
    ((current_el >> 2) & 0b11) as u8
}

// Embed the exception vector table directly using global_asm
global_asm!(include_str!("exception_vector.s"));

// Import the assembly exception vector table and transition function
extern "C" {
    static exception_vector_table: u8;
    fn drop_to_el0(entry_point: u64, stack_pointer: u64) -> !;
}

/// Saved register context for syscalls (must match assembly layout)
/// CRITICAL: Assembly stores x30 at offset 240, then has 8-byte gap, then elr/spsr at 256
/// This padding ensures the struct matches the assembly's 272-byte layout
#[repr(C)]
pub struct ExceptionContext {
    // General purpose registers X0-X30
    pub x0: u64, pub x1: u64, pub x2: u64, pub x3: u64,
    pub x4: u64, pub x5: u64, pub x6: u64, pub x7: u64,
    pub x8: u64, pub x9: u64, pub x10: u64, pub x11: u64,
    pub x12: u64, pub x13: u64, pub x14: u64, pub x15: u64,
    pub x16: u64, pub x17: u64, pub x18: u64, pub x19: u64,
    pub x20: u64, pub x21: u64, pub x22: u64, pub x23: u64,
    pub x24: u64, pub x25: u64, pub x26: u64, pub x27: u64,
    pub x28: u64, pub x29: u64, pub x30: u64,
    // PADDING: Assembly has 8-byte gap after x30 before elr_el1
    pub _padding: u64,
    // Exception link register and saved program status register
    pub elr_el1: u64,
    pub spsr_el1: u64,
}

/// Exception syndrome register value
#[repr(C)]
pub struct ExceptionSyndrome {
    pub ec: u32,     // Exception class
    pub il: bool,    // Instruction length (32-bit if true)
    pub iss: u32,    // Instruction specific syndrome
}

/// Initialize exception vectors for ARM64
pub fn init_exception_vectors() {
    unsafe {
        // Set VBAR_EL1 to point to our assembly exception vector table
        let vbar = &exception_vector_table as *const _ as u64;
        VBAR_EL1.set(vbar);

        // Ensure changes take effect
        barrier::isb(barrier::SY);
    }
}

/// Rust handler called from assembly syscall stub
/// Context pointer points to saved registers on the stack
#[no_mangle]
extern "C" fn handle_el0_syscall_rust(ctx: *mut ExceptionContext) {
    let ctx = unsafe { &mut *ctx };

    // Check ESR to verify this is actually an SVC
    let esr = ESR_EL1.get();
    let ec = (esr >> 26) & 0x3F;

    if ec != 0x15 {
        // Not a syscall, some other synchronous exception - print full fault details
        crate::kernel::uart_write_string("[EXCEPTION] EL0 sync exception (not SVC)\r\n");

        // Read fault status registers
        let esr = ESR_EL1.get();
        let elr = ELR_EL1.get();
        let far = FAR_EL1.get();

        // Print ESR_EL1 (Exception Syndrome Register)
        crate::kernel::uart_write_string("[FAULT] ESR_EL1: 0x");
        print_hex_simple(esr);
        crate::kernel::uart_write_string("\r\n");

        // Print EC (Exception Class) and ISS (Instruction Specific Syndrome)
        let ec = (esr >> 26) & 0x3F;
        let iss = esr & 0xFFFFFF;
        crate::kernel::uart_write_string(&alloc::format!("[FAULT] EC=0x{:02x}, ISS=0x{:06x}\r\n", ec, iss));

        // Print ELR_EL1 (Exception Link Register - faulting address)
        crate::kernel::uart_write_string("[FAULT] ELR_EL1 (fault address): 0x");
        print_hex_simple(elr);
        crate::kernel::uart_write_string("\r\n");

        // Print FAR_EL1 (Fault Address Register - if applicable)
        crate::kernel::uart_write_string("[FAULT] FAR_EL1: 0x");
        print_hex_simple(far);
        crate::kernel::uart_write_string("\r\n");

        // Hang the system so we can read the debug output
        crate::kernel::uart_write_string("[FAULT] Halting system for debugging\r\n");
        loop { aarch64_cpu::asm::wfe(); }
    }

    // Extract syscall number (X8) and arguments (X0-X6)
    let syscall_num = ctx.x8;
    let args = crate::kernel::syscall::SyscallArgs::new(
        ctx.x0, ctx.x1, ctx.x2, ctx.x3, ctx.x4, ctx.x5, ctx.x6
    );

    // Call syscall dispatcher
    let result = crate::kernel::syscall::handle_syscall(syscall_num, args);

    // Check if this is an exit syscall (special sentinel value)
    const EXIT_SENTINEL: u64 = 0xDEADBEEF_DEADBEEF;
    if result as u64 == EXIT_SENTINEL {
        // User program called exit() - terminate thread and switch to next thread
        crate::kernel::uart_write_string("[KERNEL] User program exiting, terminating thread\r\n");

        // Restore original kernel MMU context before switching threads
        crate::kernel::memory::restore_kernel_mmu_context();

        crate::kernel::uart_write_string("[KERNEL] Terminating thread and switching to next\r\n");

        // Terminate current thread and get next thread's context
        // CRITICAL: This must be done outside the scheduler lock to avoid deadlock
        let next_context = {
            let mut scheduler = crate::kernel::scheduler::SCHEDULER.lock();
            scheduler.terminate_current_and_yield()
        };

        // If we have a next thread, jump to it (never returns)
        if let Some(ctx) = next_context {
            crate::kernel::uart_write_string("[KERNEL] Jumping to next thread...\r\n");
            unsafe {
                crate::kernel::thread::jump_to_thread(ctx);
            }
        } else {
            // No more threads to run
            crate::kernel::uart_write_string("[KERNEL] No more threads, halting\r\n");
            loop {
                aarch64_cpu::asm::wfe();
            }
        }
    }

    // Write result back to X0 (will be restored on return)
    ctx.x0 = result as u64;

    // Syscall completed successfully - return to EL0
}

/// Start a user process at EL0 and return when it exits
/// This is the shell-friendly version that doesn't hang
pub fn start_user_process_returnable(entry_point: extern "C" fn() -> !) {
    crate::kernel::uart_write_string("[USER-PROCESS] Starting user process (returnable version)\r\n");

    // For now, we'll use the existing non-returning version but with a plan to return
    // The real solution requires significant architectural changes

    // TODO: Implement proper process management that can return
    // For now, this provides the interface for the shell to use
    unsafe {
        start_user_process(entry_point);
    }

    // This will never be reached with current implementation
    crate::kernel::uart_write_string("[USER-PROCESS] Unexpectedly returned from user process\r\n");
}

/// Start a user process at EL0 (non-returning version)
/// Allocates a user stack and transitions to EL0 to execute the given function
pub fn start_user_process(entry_point: extern "C" fn() -> !) -> ! {
    const USER_STACK_SIZE: usize = 64 * 1024; // 64KB user stack

    // Allocate user stack (this is a simple allocation - in production, use proper page-aligned allocation)
    let user_stack = unsafe {
        let layout = core::alloc::Layout::from_size_align_unchecked(USER_STACK_SIZE, 16);
        let ptr = alloc::alloc::alloc(layout);
        if ptr.is_null() {
            crate::kernel::uart_write_string("[ERROR] Failed to allocate user stack\r\n");
            loop { aarch64_cpu::asm::wfe(); }
        }
        ptr
    };

    // Stack grows downward, so point to the end
    let stack_top = unsafe { user_stack.add(USER_STACK_SIZE) as u64 };

    crate::kernel::uart_write_string("[KERNEL] Starting user process at EL0\r\n");
    crate::kernel::uart_write_string("  Entry point: 0x");
    print_hex_simple(entry_point as u64);
    crate::kernel::uart_write_string("\r\n  Stack top: 0x");
    print_hex_simple(stack_top);
    crate::kernel::uart_write_string("\r\n");

    // Transition to EL0 (never returns)
    unsafe {
        drop_to_el0(entry_point as u64, stack_top);
    }
}

// Helper function to print hex values
fn print_hex_simple(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    for i in (0..16).rev() {
        let digit = (n >> (i * 4)) & 0xF;
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[digit as usize]);
        }
    }
}

fn handle_sync_exception() {
    // Handle synchronous exceptions (page faults, system calls, etc.)
    let esr = ESR_EL1.get();
    let elr = ELR_EL1.get();
    let far = FAR_EL1.get();

    // In a real kernel, we'd handle different exception types
    // For now, just halt
    loop {
        aarch64_cpu::asm::wfe();
    }
}

fn handle_irq() {
    // Handle IRQ interrupts
    // Read from GIC to determine interrupt source
    unsafe {
        let intid = gic_acknowledge_interrupt();
        
        match intid {
            30 => handle_timer_interrupt(), // Physical timer
            _ => {} // Unknown interrupt
        }
        
        gic_end_interrupt(intid);
    }
}

fn handle_fiq() {
    // Handle FIQ (Fast Interrupt Request)
    // Usually not used in modern systems
}

fn handle_serror() {
    // Handle system error
    // This is usually fatal
    loop {
        aarch64_cpu::asm::wfe();
    }
}

// GIC (Generic Interrupt Controller) for ARM64
const GICD_BASE: u64 = 0x08000000; // GIC distributor base for virt machine
const GICC_BASE: u64 = 0x08010000; // GIC CPU interface base

const GICD_CTLR: u64 = GICD_BASE + 0x0000;
const GICD_ISENABLER: u64 = GICD_BASE + 0x0100;
const GICD_ICPENDR: u64 = GICD_BASE + 0x0280;
const GICD_IPRIORITYR: u64 = GICD_BASE + 0x0400;
const GICD_ITARGETSR: u64 = GICD_BASE + 0x0800;
const GICD_ICFGR: u64 = GICD_BASE + 0x0C00;

const GICC_CTLR: u64 = GICC_BASE + 0x0000;
const GICC_PMR: u64 = GICC_BASE + 0x0004;
const GICC_IAR: u64 = GICC_BASE + 0x000C;
const GICC_EOIR: u64 = GICC_BASE + 0x0010;

/// Initialize the GIC (Generic Interrupt Controller)
pub fn init_gic() {
    unsafe {
        // Disable the distributor
        core::ptr::write_volatile(GICD_CTLR as *mut u32, 0);
        
        // Set all interrupts to lowest priority
        for i in 0..256 {
            let addr = (GICD_IPRIORITYR + (i * 4)) as *mut u32;
            core::ptr::write_volatile(addr, 0xFFFFFFFF);
        }
        
        // Target all interrupts to CPU 0
        for i in 8..256 { // Skip first 32 (SGI/PPI)
            let addr = (GICD_ITARGETSR + i) as *mut u8;
            core::ptr::write_volatile(addr, 0x01);
        }
        
        // Enable all interrupts
        for i in 1..8 { // 32 interrupts per register, skip first 32
            let addr = (GICD_ISENABLER + (i * 4)) as *mut u32;
            core::ptr::write_volatile(addr, 0xFFFFFFFF);
        }
        
        // Enable the distributor
        core::ptr::write_volatile(GICD_CTLR as *mut u32, 1);
        
        // CPU interface configuration
        // Set priority mask to allow all priorities
        core::ptr::write_volatile(GICC_PMR as *mut u32, 0xFF);
        
        // Enable CPU interface
        core::ptr::write_volatile(GICC_CTLR as *mut u32, 1);
        
        // Enable interrupts at CPU level
        // Clear interrupt mask bit (unmask IRQ)
        unsafe {
            core::arch::asm!("msr daifclr, #2");
        }
    }
}

fn gic_acknowledge_interrupt() -> u32 {
    unsafe {
        core::ptr::read_volatile(GICC_IAR as *const u32) & 0x3FF
    }
}

fn gic_end_interrupt(intid: u32) {
    unsafe {
        core::ptr::write_volatile(GICC_EOIR as *mut u32, intid);
    }
}

/// Initialize the ARM generic timer
pub fn init_timer() {
    unsafe {
        // Read timer frequency from system register
        let freq = CNTFRQ_EL0.get();

        // Set timer for 10ms intervals (100 Hz) - standard for preemptive multitasking
        let tval = freq / 100; // 10ms worth of ticks
        CNTP_TVAL_EL0.set(tval);

        // Enable the timer (ENABLE bit)
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);

        // Enable timer interrupt (interrupt 30)
        let addr = (GICD_ISENABLER + (30 / 32) * 4) as *mut u32;
        let bit = 1u32 << (30 % 32);
        let current = core::ptr::read_volatile(addr);
        core::ptr::write_volatile(addr, current | bit);
    }
}

fn handle_timer_interrupt() {
    unsafe {
        // Acknowledge the timer interrupt
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::CLEAR);

        // Reset timer for next interrupt (10ms)
        let freq = CNTFRQ_EL0.get();
        CNTP_TVAL_EL0.set(freq / 100); // 10ms intervals
        CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);

        // Preemptive multitasking - preempt every tick (10ms time slices)
        static mut TICK_COUNT: u64 = 0;
        const PREEMPT_TICKS: u64 = 1; // Preempt every 10ms

        TICK_COUNT += 1;
        if TICK_COUNT >= PREEMPT_TICKS {
            TICK_COUNT = 0;

            // Preempt current thread - get context switch info while holding lock
            let switch_info = {
                crate::kernel::scheduler::SCHEDULER.lock().preempt()
            }; // Lock dropped here!

            // Perform context switch outside the lock
            if let Some((current_ptr, next_ptr, is_first)) = switch_info {
                if is_first {
                    crate::kernel::thread::jump_to_thread(next_ptr);
                } else {
                    crate::kernel::thread::context_switch(current_ptr, next_ptr);
                }
            }
        }
    }
}