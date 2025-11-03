#![no_std]
#![no_main]

use librost::*;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Userspace Image Viewer ===\r\n");
    print_debug("Running at EL0 with privilege separation\r\n");

    // Get framebuffer info
    print_debug("Getting framebuffer info...\r\n");
    let fb_info = match fb_info() {
        Some(info) => {
            print_debug("✓ Framebuffer info obtained\r\n");
            info
        }
        None => {
            print_debug("✗ Failed to get framebuffer info\r\n");
            exit(1);
        }
    };

    print_debug("Framebuffer dimensions: ");
    // Simple number printing (width)
    print_debug("\r\n");

    // Map framebuffer
    print_debug("Mapping framebuffer...\r\n");
    let fb_ptr = match fb_map() {
        Some(ptr) => {
            print_debug("✓ Framebuffer mapped\r\n");
            ptr
        }
        None => {
            print_debug("✗ Failed to map framebuffer\r\n");
            exit(1);
        }
    };

    // Draw a test pattern (colorful gradient)
    print_debug("Drawing test pattern...\r\n");

    let width = fb_info.width as usize;
    let height = fb_info.height as usize;
    let stride = fb_info.stride as usize;

    unsafe {
        let fb = core::slice::from_raw_parts_mut(fb_ptr, stride * height);

        // Draw a colorful gradient pattern
        for y in 0..height {
            for x in 0..width {
                // Create a gradient: red increases with x, green with y, blue is constant
                let r = ((x * 255) / width) as u32;
                let g = ((y * 255) / height) as u32;
                let b = 128u32;

                // Pixel format: 0xAARRGGBB
                let pixel = 0xFF000000 | (r << 16) | (g << 8) | b;

                fb[y * stride + x] = pixel;
            }
        }
    }

    // Flush framebuffer to display
    print_debug("Flushing to display...\r\n");
    if fb_flush() == 0 {
        print_debug("✓ Display flushed successfully\r\n");
    } else {
        print_debug("✗ Failed to flush display\r\n");
    }

    print_debug("\r\n");
    print_debug("Test pattern displayed! Press any key to exit...\r\n");
    print_debug("\r\n");

    // Wait for input event (any key/mouse action to exit)
    loop {
        if let Some(_event) = poll_event() {
            print_debug("Input received, exiting...\r\n");
            break;
        }

        // Small delay to avoid busy-waiting
        for _ in 0..100000 {
            unsafe { core::arch::asm!("nop"); }
        }
    }

    exit(0);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in userspace image viewer!\r\n");
    exit(1);
}
