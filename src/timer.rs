use core::arch::asm;

pub const TIMER_IRQ: u32 = 30; // Physical timer interrupt

pub struct Timer;

impl Timer {
    pub fn init() {
        let uart = crate::uart::Uart::new(0x0900_0000);
        
        unsafe {
            // Get timer frequency
            let freq: u64;
            asm!("mrs {}, cntfrq_el0", out(reg) freq);
            
            uart.puts("Timer frequency: ");
            uart.put_hex(freq);
            uart.puts(" Hz\n");
            
            // Set timer value (1 second)
            let tval = freq / 2; // Half second for faster testing
            asm!("msr cntp_tval_el0, {}", in(reg) tval);
            
            // Clear timer interrupt mask and enable timer
            asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
            
            // Ensure timer interrupt is not masked at CPU level
            asm!("msr daifclr, #2"); // Clear interrupt mask
        }
        
        uart.puts("Timer initialized\n");
    }
    
    pub fn handle_interrupt() {
        let uart = crate::uart::Uart::new(0x0900_0000);
        uart.puts("Timer tick!\n");
        
        unsafe {
            // Get timer frequency for next interval
            let freq: u64;
            asm!("mrs {}, cntfrq_el0", out(reg) freq);
            
            // Reset timer for another half second
            asm!("msr cntp_tval_el0, {}", in(reg) freq / 2);
        }
    }
}