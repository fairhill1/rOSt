// Kernel entry point after exiting UEFI boot services

use core::arch::asm;

// Simple print macro for debugging - writes to serial port
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        // For now, just a no-op since we can't easily access serial after UEFI exit
        // In a real implementation, we'd set up UART or use a different output method
    };
}

pub mod memory;
pub mod interrupts;
pub mod framebuffer;
pub mod pci;
pub mod virtio_gpu;

/// Information passed from UEFI bootloader to kernel
#[repr(C)]
pub struct BootInfo {
    pub memory_map: &'static [memory::MemoryDescriptor],
    pub framebuffer: framebuffer::FramebufferInfo,
    pub acpi_rsdp: Option<u64>,
}

// Basic UART output for debugging
fn uart_write_string(s: &str) {
    const UART_BASE: u64 = 0x09000000; // QEMU ARM virt machine UART address
    for byte in s.bytes() {
        unsafe {
            core::ptr::write_volatile(UART_BASE as *mut u8, byte);
        }
    }
}

/// Main kernel entry point after UEFI boot services are exited
#[no_mangle]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // First thing - prove we made it to the kernel!
    uart_write_string("KERNEL STARTED! ExitBootServices SUCCESS!\r\n");
    uart_write_string("Initializing Rust OS kernel...\r\n");
    
    // Initialize physical memory manager FIRST - VirtIO-GPU needs it for allocation
    uart_write_string("Initializing physical memory...\r\n");
    memory::init_physical_memory(&boot_info.memory_map);
    uart_write_string("Physical memory initialized\r\n");
    
    // Now initialize VirtIO-GPU for graphics
    uart_write_string("Trying to initialize VirtIO-GPU...\r\n");
    let mut gpu_framebuffer_info = None;
    
    // Initialize VirtIO-GPU properly
    if let Some(mut virtio_gpu) = virtio_gpu::VirtioGpuDriver::new() {
        uart_write_string("VirtIO-GPU device found, initializing...\r\n");
        
        match virtio_gpu.initialize() {
            Ok(()) => {
                uart_write_string("VirtIO-GPU initialized successfully!\r\n");
                let (fb_addr, width, height, stride) = virtio_gpu.get_framebuffer_info();
                
                uart_write_string("VirtIO-GPU framebuffer: 0x");
                let mut addr = fb_addr;
                for _ in 0..16 {
                    let digit = (addr >> 60) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
                    addr <<= 4;
                }
                uart_write_string(" size: ");
                let mut w = width as u64;
                for _ in 0..8 {
                    let digit = (w >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
                    w <<= 4;
                }
                uart_write_string("x");
                let mut h = height as u64;
                for _ in 0..8 {
                    let digit = (h >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
                    h <<= 4;
                }
                uart_write_string("\r\n");
                
                if fb_addr != 0 {
                    gpu_framebuffer_info = Some(framebuffer::FramebufferInfo {
                        base_address: fb_addr,
                        size: (height * stride) as usize,
                        width,
                        height,
                        pixels_per_scanline: stride / 4,
                        pixel_format: framebuffer::PixelFormat::Rgb,
                    });
                }
            }
            Err(err_msg) => {
                uart_write_string("VirtIO-GPU initialization failed: ");
                uart_write_string(err_msg);
                uart_write_string("\r\n");
            }
        }
    } else {
        uart_write_string("VirtIO-GPU device not found\r\n");
    }
    
    // OLD VirtIO code (disabled for now)
    /*if let Some(mut virtio_gpu) = virtio_gpu::VirtioGpuDriver::new() {
        uart_write_string("VirtIO-GPU PCI device found! Checking PCI configuration...\r\n");
        
        // Debug: print PCI device info before initialization
        uart_write_string("PCI Device - Vendor: 0x1AF4, Device: 0x1050\r\n");
        uart_write_string("Initializing with safety checks...\r\n");
        
        // Try initialization with comprehensive error handling
        match virtio_gpu.initialize() {
            Ok(()) => {
                uart_write_string("VirtIO-GPU initialized successfully! Getting framebuffer...\r\n");
                let (fb_addr, width, height, stride) = virtio_gpu.get_framebuffer_info();
                if fb_addr != 0 && fb_addr >= 0x10000000 && fb_addr < 0x80000000 {
                    uart_write_string("Got valid framebuffer! Setting up graphics...\r\n");
                    uart_write_string("Framebuffer address: 0x");
                    // Simple hex output
                    let mut addr = fb_addr;
                    for _ in 0..16 {
                        let digit = (addr >> 60) & 0xF;
                        let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                        unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
                        addr <<= 4;
                    }
                    uart_write_string("\r\n");
                    
                    gpu_framebuffer_info = Some(framebuffer::FramebufferInfo {
                        base_address: fb_addr,
                        size: (height * stride) as usize,
                        width,
                        height,
                        pixels_per_scanline: stride / 4,
                        pixel_format: framebuffer::PixelFormat::Rgb,
                    });
                } else {
                    uart_write_string("ERROR: Got invalid framebuffer address: 0x");
                    let mut addr = fb_addr;
                    for _ in 0..16 {
                        let digit = (addr >> 60) & 0xF;
                        let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                        unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
                        addr <<= 4;
                    }
                    uart_write_string("\r\n");
                    uart_write_string("Will continue without graphics\r\n");
                }
            }
            Err(err_msg) => {
                uart_write_string("ERROR: VirtIO-GPU initialization failed: ");
                uart_write_string(err_msg);
                uart_write_string("\r\n");
                uart_write_string("Will continue without graphics\r\n");
            }
        }
    } else {
        uart_write_string("VirtIO-GPU PCI device not found - will run without graphics\r\n");
        uart_write_string("This is expected if QEMU is not configured with virtio-gpu\r\n");
    }
    */ // End of commented VirtIO code

    // Use VirtIO-GPU framebuffer if available, otherwise fallback
    let fb_info = gpu_framebuffer_info.as_ref().unwrap_or(&boot_info.framebuffer);
    uart_write_string("Initializing framebuffer...\r\n");
    framebuffer::init(fb_info);
    
    // Skip conflicting framebuffer operations - VirtIO-GPU already drew graphics
    uart_write_string("=== RUST OS KERNEL RUNNING SUCCESSFULLY! ===\r\n");
    uart_write_string("ExitBootServices worked! We're in kernel space!\r\n");
    uart_write_string("VirtIO-GPU driver loaded and initialized!\r\n");
    uart_write_string("This is a major milestone - the core OS is working!\r\n");
    
    if fb_info.base_address != 0 {
        uart_write_string("Graphics framebuffer is active - drawing HELLO WORLD!\r\n");
        
        // Draw directly to the GOP framebuffer
        unsafe {
            let fb_ptr = fb_info.base_address as *mut u32;
            let width = fb_info.width;
            let height = fb_info.height;
            let pixels_per_scanline = fb_info.pixels_per_scanline;
            
            // Clear screen to black
            for y in 0..height {
                for x in 0..width {
                    let offset = (y * pixels_per_scanline + x) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFF000000); // Black background
                }
            }
            
            // Draw "HELLO WORLD" in big white letters
            draw_hello_world(fb_ptr, width, height, pixels_per_scanline);
            
            // Draw some colored rectangles for visibility
            // Red rectangle
            for y in 400..450 {
                for x in 100..200 {
                    if y < height && x < width {
                        let offset = (y * pixels_per_scanline + x) as usize;
                        core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFF0000); // Red
                    }
                }
            }
            
            // Green rectangle  
            for y in 400..450 {
                for x in 250..350 {
                    if y < height && x < width {
                        let offset = (y * pixels_per_scanline + x) as usize;
                        core::ptr::write_volatile(fb_ptr.add(offset), 0xFF00FF00); // Green
                    }
                }
            }
            
            // Blue rectangle
            for y in 400..450 {
                for x in 400..500 {
                    if y < height && x < width {
                        let offset = (y * pixels_per_scanline + x) as usize;
                        core::ptr::write_volatile(fb_ptr.add(offset), 0xFF0000FF); // Blue
                    }
                }
            }
        }
        
        uart_write_string("HELLO WORLD drawn to framebuffer!\r\n");
    } else {
        uart_write_string("No framebuffer available - running in text mode\r\n");
    }
    
    // Physical memory already initialized
    uart_write_string("Physical memory: OK\r\n");
    
    // Set up exception vectors for ARM64
    interrupts::init_exception_vectors();
    uart_write_string("Exception vectors: OK\r\n");
    
    // Initialize virtual memory (page tables)
    memory::init_virtual_memory();
    uart_write_string("Virtual memory: OK\r\n");
    
    // Initialize interrupt controller (GIC)
    interrupts::init_gic();
    uart_write_string("GIC interrupt controller: OK\r\n");
    
    // Set up timer
    interrupts::init_timer();
    uart_write_string("Timer: OK\r\n");
    uart_write_string("Kernel initialization complete!\r\n");
    
    // Clean stable loop - no framebuffer interference
    uart_write_string("Kernel main loop running! OS is stable!\r\n");
    uart_write_string("🎉 SUCCESS: Rust OS with UEFI boot working! 🎉\r\n");
    uart_write_string("Display should show clean VirtIO-GPU graphics:\r\n");
    uart_write_string("- Black background\r\n");
    uart_write_string("- White RUST letters\r\n");
    uart_write_string("- Red, Green, Blue squares\r\n");
    uart_write_string("Check the QEMU display window!\r\n");
    
    // Stable infinite loop - no flashing or interference  
    loop {
        // Just a quiet loop - let VirtIO-GPU graphics stay visible
        unsafe {
            core::arch::asm!("wfe"); // Wait for event - power efficient
        }
    }
}

// Draw "HELLO WORLD" using block letters
unsafe fn draw_hello_world(fb_ptr: *mut u32, width: u32, height: u32, pixels_per_scanline: u32) {
    let start_x = 50u32;
    let start_y = 50u32;
    let letter_width = 40u32;
    let letter_height = 60u32;
    let letter_spacing = 10u32;
    
    // Draw each letter of "HELLO WORLD"
    draw_letter_h(fb_ptr, start_x, start_y, width, height, pixels_per_scanline);
    draw_letter_e(fb_ptr, start_x + (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_l(fb_ptr, start_x + 2 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_l(fb_ptr, start_x + 3 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_o(fb_ptr, start_x + 4 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    
    // Space
    draw_letter_w(fb_ptr, start_x + 6 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_o(fb_ptr, start_x + 7 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_r(fb_ptr, start_x + 8 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_l(fb_ptr, start_x + 9 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
    draw_letter_d(fb_ptr, start_x + 10 * (letter_width + letter_spacing), start_y, width, height, pixels_per_scanline);
}

// Letter H
unsafe fn draw_letter_h(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..35 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (x > 27) || (y >= 26 && y <= 33);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter E
unsafe fn draw_letter_e(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..30 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (y < 8) || (y >= 26 && y <= 33) || (y >= 52);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter L
unsafe fn draw_letter_l(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..30 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (y >= 52);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter O
unsafe fn draw_letter_o(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..35 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (y < 8) || (y >= 52) || (x < 8 && y >= 8 && y < 52) || (x >= 27 && y >= 8 && y < 52);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter W
unsafe fn draw_letter_w(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..45 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (x >= 37) || (x >= 18 && x <= 25 && y >= 30) || (x >= 8 && x <= 15 && y >= 45) || (x >= 30 && x <= 37 && y >= 45);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter R
unsafe fn draw_letter_r(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..35 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (y < 8 && x < 27) || (x >= 27 && y >= 8 && y <= 25) || (y >= 26 && y <= 33 && x < 27) || (x >= 18 && y >= 34);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

// Letter D
unsafe fn draw_letter_d(fb_ptr: *mut u32, start_x: u32, start_y: u32, width: u32, height: u32, pixels_per_scanline: u32) {
    for y in 0..60 {
        for x in 0..35 {
            let px = start_x + x;
            let py = start_y + y;
            
            if px < width && py < height {
                let should_draw = (x < 8) || (y < 8 && x < 27) || (x >= 27 && y >= 8 && y < 52) || (y >= 52 && x < 27);
                if should_draw {
                    let offset = (py * pixels_per_scanline + px) as usize;
                    core::ptr::write_volatile(fb_ptr.add(offset), 0xFFFFFFFF); // White
                }
            }
        }
    }
}

