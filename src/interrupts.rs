use core::arch::asm;

#[repr(C)]
pub struct ExceptionContext {
    gpr: [u64; 30],
    lr: u64,
    sp: u64,
    elr: u64,
    spsr: u64,
}

extern "C" {
    fn vectors();
    fn set_vbar_el1(addr: u64);
}

pub fn init() {
    unsafe {
        set_vbar_el1(vectors as *const () as u64);
        
        // Enable interrupts
        asm!("msr daifclr, #2");
    }
    
    crate::uart::Uart::new(0x0900_0000).puts("Exception vectors initialized\n");
}

#[no_mangle]
pub extern "C" fn sync_exception_handler(_ctx: *mut ExceptionContext) {
    let uart = crate::uart::Uart::new(0x0900_0000);
    uart.puts("Synchronous exception!\n");
    
    unsafe {
        let esr: u64;
        let elr: u64;
        let far: u64;
        
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, elr_el1", out(reg) elr);
        asm!("mrs {}, far_el1", out(reg) far);
        
        uart.puts("ESR: ");
        uart.put_hex(esr);
        uart.puts("\nELR: ");
        uart.put_hex(elr);
        uart.puts("\nFAR: ");
        uart.put_hex(far);
        uart.puts("\n");
    }
}

#[no_mangle]
pub extern "C" fn irq_exception_handler(_ctx: *mut ExceptionContext) {
    // Handle the IRQ
    handle_irq();
}

#[no_mangle]
pub extern "C" fn fiq_exception_handler(_ctx: *mut ExceptionContext) {
    let uart = crate::uart::Uart::new(0x0900_0000);
    uart.puts("FIQ received!\n");
}

#[no_mangle]
pub extern "C" fn serror_exception_handler(_ctx: *mut ExceptionContext) {
    let uart = crate::uart::Uart::new(0x0900_0000);
    uart.puts("SError exception!\n");
    loop {
        unsafe { asm!("wfe") };
    }
}

fn handle_irq() {
    unsafe {
        if let Some(ref gic) = crate::GIC {
            if let Some(irq) = gic.get_pending_interrupt() {
                match irq {
                    30 => crate::timer::Timer::handle_interrupt(), // Physical timer
                    27 => crate::timer::Timer::handle_interrupt(), // Virtual timer (backup)
                    33 => {
                        // UART interrupt
                        let uart = crate::uart::Uart::new(0x0900_0000);
                        crate::input::handle_uart_input(&uart);
                    }
                    _ => {
                        let uart = crate::uart::Uart::new(0x0900_0000);
                        uart.puts("Unknown IRQ: ");
                        uart.put_hex(irq as u64);
                        uart.puts("\n");
                    }
                }
                gic.end_interrupt(irq);
            }
        }
    }
}