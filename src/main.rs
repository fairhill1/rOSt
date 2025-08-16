#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use core::arch::asm;
use core::panic::PanicInfo;

extern crate alloc;

mod uart;
mod interrupts;
mod gic;
mod timer;
mod input;
mod shell;
mod allocator;
mod mmu_simple;
mod scheduler;
mod filesystem;
mod graphics;
mod ramfb;
mod device_tree;
use mmu_simple as mmu;

static mut GIC: Option<gic::Gic> = None;

#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    let uart = uart::Uart::new(0x0900_0000);
    uart.init();
    
    uart.puts("Hello from Rust OS on AArch64!\n");
    uart.puts("Kernel initialized successfully.\n");
    
    // Initialize heap allocator
    allocator::init();
    
    // Initialize interrupts first
    interrupts::init();
    
    // Initialize GIC
    unsafe {
        GIC = Some(gic::Gic::new());
        if let Some(ref gic) = GIC {
            gic.init();
            // Enable interrupts
            gic.enable_interrupt(27); // Virtual timer
            gic.enable_interrupt(30); // Physical timer  
            gic.enable_interrupt(uart::UART_IRQ); // UART interrupt
            uart.puts("Interrupts enabled (timer: 27,30; uart: 33)\n");
        }
    }
    
    // TODO: MMU still has issues - disable for now to focus on other features
    // We'll come back and fix this properly later
    // mmu::init();
    uart.puts("MMU disabled for now - using physical addressing\n");
    
    // Re-initialize UART to ensure interrupts work
    uart.init();
    
    // Initialize scheduler
    scheduler::init();
    
    // Initialize filesystem
    filesystem::init();
    
    // Parse device tree for hardware discovery
    uart.puts("Scanning for device tree...\n");
    let dtb_addr = device_tree::get_dtb_address();
    if dtb_addr != 0 {
        uart.puts("Found device tree at: ");
        uart.put_hex(dtb_addr);
        uart.puts("\n");
        
        match device_tree::parse_device_tree(dtb_addr) {
            Ok(dt_info) => {
                uart.puts("Device tree parsed successfully\n");
                if let Some(fb_addr) = dt_info.framebuffer_addr {
                    uart.puts("Found framebuffer at: ");
                    uart.put_hex(fb_addr);
                    uart.puts("\n");
                }
                if let Some((width, height)) = dt_info.framebuffer_size {
                    uart.puts("Framebuffer size: ");
                    uart.put_hex(width as u64);
                    uart.puts("x");
                    uart.put_hex(height as u64);
                    uart.puts("\n");
                }
            }
            Err(e) => {
                uart.puts("Device tree parsing failed: ");
                uart.puts(e);
                uart.puts("\n");
            }
        }
    } else {
        uart.puts("No device tree found, using fallback initialization\n");
    }
    
    // Initialize ramfb first
    match ramfb::init_ramfb() {
        Ok(()) => {
            uart.puts("RAMFB initialized successfully!\n");
        }
        Err(e) => {
            uart.puts("RAMFB init failed: ");
            uart.puts(e);
            uart.puts("\n");
        }
    }
    
    // Initialize graphics
    match graphics::init() {
        Ok(()) => {
            uart.puts("Graphics initialized successfully!\n");
        }
        Err(e) => {
            uart.puts("Graphics init failed: ");
            uart.puts(e);
            uart.puts("\n");
        }
    }
    
    // Initialize timer (but make it less noisy)
    // Comment out timer init to avoid spam during shell usage
    // timer::Timer::init();
    
    uart.puts("All systems ready!\n");
    
    // Start the shell
    let mut shell = shell::Shell::new(uart::Uart::new(0x0900_0000));
    shell.run()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let uart = uart::Uart::new(0x0900_0000);
    uart.puts("Kernel panic: ");
    
    if let Some(location) = info.location() {
        uart.puts(location.file());
        uart.puts(":");
        uart.put_hex(location.line() as u64);
    }
    
    uart.puts("\n");
    
    loop {
        unsafe { asm!("wfe") };
    }
}
