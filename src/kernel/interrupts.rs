// ARM64 exception handling and interrupt controller

use core::arch::asm;

/// ARM64 exception vector table
/// Must be aligned to 2KB (0x800)
#[repr(C, align(2048))]
pub struct ExceptionVectorTable {
    vectors: [u32; 512], // 128 instructions per exception level
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
        // Create exception vector table
        let vectors = create_vector_table();
        
        // Set VBAR_EL1 (Vector Base Address Register)
        let vbar = &vectors as *const _ as u64;
        asm!("msr vbar_el1, {}", in(reg) vbar);
        
        // Ensure changes take effect
        asm!("isb");
    }
}

fn create_vector_table() -> &'static ExceptionVectorTable {
    // In a real implementation, this would be a static table with proper handlers
    // For now, we'll create a simple one that just halts
    static mut VECTOR_TABLE: ExceptionVectorTable = ExceptionVectorTable {
        vectors: [0; 512],
    };
    
    unsafe {
        // Each vector is 128 bytes (32 instructions)
        // We'll just put a branch to handler for each
        
        // Current EL with SP0
        install_vector(&mut VECTOR_TABLE.vectors, 0x000, sync_handler_el1_sp0);
        install_vector(&mut VECTOR_TABLE.vectors, 0x080, irq_handler_el1_sp0);
        install_vector(&mut VECTOR_TABLE.vectors, 0x100, fiq_handler_el1_sp0);
        install_vector(&mut VECTOR_TABLE.vectors, 0x180, serror_handler_el1_sp0);
        
        // Current EL with SPx
        install_vector(&mut VECTOR_TABLE.vectors, 0x200, sync_handler_el1_spx);
        install_vector(&mut VECTOR_TABLE.vectors, 0x280, irq_handler_el1_spx);
        install_vector(&mut VECTOR_TABLE.vectors, 0x300, fiq_handler_el1_spx);
        install_vector(&mut VECTOR_TABLE.vectors, 0x380, serror_handler_el1_spx);
        
        &VECTOR_TABLE
    }
}

fn install_vector(table: &mut [u32; 512], offset: usize, handler: extern "C" fn()) {
    // Create a branch instruction to the handler
    // This is simplified - real implementation would save context
    let handler_addr = handler as usize;
    let vector_addr = table.as_ptr() as usize + offset;
    let branch_offset = ((handler_addr - vector_addr) >> 2) as u32;
    
    // ARM64 branch instruction: 0x14000000 | offset
    table[offset / 4] = 0x14000000 | (branch_offset & 0x03FFFFFF);
}

// Exception handlers
extern "C" fn sync_handler_el1_sp0() {
    handle_sync_exception();
}

extern "C" fn irq_handler_el1_sp0() {
    handle_irq();
}

extern "C" fn fiq_handler_el1_sp0() {
    handle_fiq();
}

extern "C" fn serror_handler_el1_sp0() {
    handle_serror();
}

extern "C" fn sync_handler_el1_spx() {
    handle_sync_exception();
}

extern "C" fn irq_handler_el1_spx() {
    handle_irq();
}

extern "C" fn fiq_handler_el1_spx() {
    handle_fiq();
}

extern "C" fn serror_handler_el1_spx() {
    handle_serror();
}

fn handle_sync_exception() {
    // Handle synchronous exceptions (page faults, system calls, etc.)
    unsafe {
        let esr: u64;
        let elr: u64;
        let far: u64;
        
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, elr_el1", out(reg) elr);
        asm!("mrs {}, far_el1", out(reg) far);
        
        // In a real kernel, we'd handle different exception types
        // For now, just halt
        loop {
            asm!("wfe");
        }
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
    unsafe {
        loop {
            asm!("wfe");
        }
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
        asm!("msr daifclr, #2"); // Clear interrupt mask bit
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

// Timer support
const CNTFRQ_EL0: u64 = 0x3B9ACA00; // 62.5 MHz typical for QEMU

/// Initialize the ARM generic timer
pub fn init_timer() {
    unsafe {
        // Read timer frequency
        let freq: u64;
        asm!("mrs {}, cntfrq_el0", out(reg) freq);
        
        // Set timer for 1 second from now
        let tval = freq; // 1 second worth of ticks
        asm!("msr cntp_tval_el0, {}", in(reg) tval);
        
        // Enable the timer
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
        
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
        asm!("msr cntp_ctl_el0, {}", in(reg) 0u64);

        // Reset timer for next interrupt (1 second)
        let freq: u64;
        asm!("mrs {}, cntfrq_el0", out(reg) freq);
        asm!("msr cntp_tval_el0, {}", in(reg) freq);
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);

        // Preemptive multitasking - switch threads every N ticks
        static mut TICK_COUNT: u64 = 0;
        const PREEMPT_TICKS: u64 = 10; // Preempt every 10 timer interrupts (10 seconds with current setup)

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