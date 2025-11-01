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
pub mod usb_hid;
pub mod virtio_input;
pub mod virtio_blk;
pub mod virtio_net;
pub mod network;
pub mod dns;
pub mod tcp;
pub mod filesystem;
pub mod shell;
pub mod dtb;
pub mod console;
pub mod window_manager;
pub mod editor;
pub mod file_explorer;
pub mod timer;
pub mod rtc;
pub mod snake;
pub mod html_parser;
pub mod browser;

/// Information passed from UEFI bootloader to kernel
#[repr(C)]
pub struct BootInfo {
    pub memory_map: &'static [memory::MemoryDescriptor],
    pub framebuffer: framebuffer::FramebufferInfo,
    pub acpi_rsdp: Option<u64>,
}

// Static storage for block devices (needs to outlive local scope for shell access)
static mut BLOCK_DEVICES: Option<alloc::vec::Vec<virtio_blk::VirtioBlkDevice>> = None;

// Static storage for network devices
static mut NET_DEVICES: Option<alloc::vec::Vec<virtio_net::VirtioNetDevice>> = None;

// Static storage for ARP cache
static mut ARP_CACHE: Option<network::ArpCache> = None;

// Static network configuration (vmnet-shared on macOS: 192.168.64.x)
static mut OUR_IP: [u8; 4] = [192, 168, 64, 10];  // vmnet-shared guest IP (avoid .2 conflict)
static mut GATEWAY_IP: [u8; 4] = [192, 168, 64, 1];  // vmnet-shared gateway/DNS

// Static for GPU driver and cursor position
static mut GPU_DRIVER: Option<virtio_gpu::VirtioGpuDriver> = None;
static mut CURSOR_X: u32 = 0;
static mut CURSOR_Y: u32 = 0;
static mut SCREEN_WIDTH: u32 = 0;
static mut SCREEN_HEIGHT: u32 = 0;

// Basic UART output for debugging
pub fn uart_write_string(s: &str) {
    const UART_BASE: u64 = 0x09000000; // QEMU ARM virt machine UART address
    for byte in s.bytes() {
        unsafe {
            core::ptr::write_volatile(UART_BASE as *mut u8, byte);
        }
    }
}

// Handle mouse movement and update hardware cursor
pub fn handle_mouse_movement(x_delta: i32, y_delta: i32) {
    unsafe {
        if let Some(ref mut gpu) = GPU_DRIVER {
            gpu.handle_mouse_move(
                x_delta,
                y_delta,
                SCREEN_WIDTH,
                SCREEN_HEIGHT,
                &mut CURSOR_X,
                &mut CURSOR_Y
            );

            // Sync hardware cursor position to framebuffer for click detection
            framebuffer::set_cursor_pos(CURSOR_X as i32, CURSOR_Y as i32);
        }
    }
}

// Get current time in milliseconds (for double-click detection, etc.)
pub fn get_time_ms() -> u64 {
    timer::get_time_ms()
}

// Simple hex printing for debug
fn print_hex_simple(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    if n == 0 {
        uart_write_string("0");
        return;
    }
    
    let mut buffer = [0u8; 16];
    let mut i = 0;
    let mut num = n;
    
    while num > 0 && i < 16 {
        buffer[i] = hex_chars[(num % 16) as usize];
        num /= 16;
        i += 1;
    }
    
    // Print in reverse order
    for j in 0..i {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, buffer[i - 1 - j]);
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
    let mut virtio_gpu_driver: Option<virtio_gpu::VirtioGpuDriver> = None;

    // Initialize VirtIO-GPU properly
    if let Some(mut virtio_gpu) = virtio_gpu::VirtioGpuDriver::new() {
        uart_write_string("VirtIO-GPU device found, initializing...\r\n");

        // Step 1: Initialize device
        match virtio_gpu.initialize() {
            Ok(()) => {
                uart_write_string("VirtIO-GPU device initialized!\r\n");

                // Step 2: Get display info
                match virtio_gpu.get_display_info() {
                    Ok(()) => {
                        uart_write_string("Display info retrieved\r\n");

                        // Step 3: Create framebuffer
                        match virtio_gpu.create_framebuffer() {
                            Ok(()) => {
                                uart_write_string("Framebuffer created!\r\n");

                                // Skip test pattern - let OS UI render instead
                                // The window manager will clear and draw to the framebuffer

                                // Step 5: Create hardware cursor
                                match virtio_gpu.create_default_cursor() {
                                    Ok(()) => {
                                        uart_write_string("Hardware cursor created!\r\n");
                                    }
                                    Err(e) => {
                                        uart_write_string("Cursor creation failed: ");
                                        uart_write_string(e);
                                        uart_write_string("\r\n");
                                    }
                                }

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

                                    // Store GPU driver and screen info for mouse handling
                                    unsafe {
                                        SCREEN_WIDTH = width;
                                        SCREEN_HEIGHT = height;
                                        CURSOR_X = width / 2;
                                        CURSOR_Y = height / 2;
                                        GPU_DRIVER = Some(virtio_gpu);
                                    }

                                    // Initialize framebuffer cursor position to match hardware cursor
                                    framebuffer::set_cursor_pos((width / 2) as i32, (height / 2) as i32);

                                    virtio_gpu_driver = None; // Moved to static
                                }
                            }
                            Err(e) => {
                                uart_write_string("Framebuffer creation failed: ");
                                uart_write_string(e);
                                uart_write_string("\r\n");
                            }
                        }
                    }
                    Err(e) => {
                        uart_write_string("Get display info failed: ");
                        uart_write_string(e);
                        uart_write_string("\r\n");
                    }
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

    uart_write_string("=== RUST OS KERNEL RUNNING SUCCESSFULLY! ===\r\n");
    uart_write_string("ExitBootServices worked! We're in kernel space!\r\n");
    uart_write_string("VirtIO-GPU driver loaded and initialized!\r\n");
    uart_write_string("This is a major milestone - the core OS is working!\r\n");

    if fb_info.base_address != 0 {
        uart_write_string("Graphics framebuffer is active - initializing GUI desktop!\r\n");

        // Initialize GUI console
        console::init();
        uart_write_string("GUI console initialized!\r\n");

        // Initialize window manager
        window_manager::init();
        uart_write_string("Window manager initialized!\r\n");
        uart_write_string("Click menu bar to open windows\r\n");

        // Initialize RTC
        rtc::init();
        uart_write_string("RTC initialized!\r\n");

        // Initialize text editor
        editor::init();
        uart_write_string("Text editor initialized!\r\n");

        // Initialize file explorer
        file_explorer::init();
        uart_write_string("File explorer initialized!\r\n");

        // Initialize web browser
        browser::init();
        uart_write_string("Web browser initialized!\r\n");

        // Initialize snake game
        snake::init();
        uart_write_string("Snake game initialized!\r\n");
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
    
    // Skip EHCI USB for now - focus on VirtIO input
    uart_write_string("Skipping EHCI USB (hangs on QEMU)...\r\n");
    
    // Parse Device Tree Blob to get correct PCI controller addresses
    uart_write_string("Parsing Device Tree Blob...\r\n");
    let pci_info = dtb::parse_dtb();

    if let Some(info) = pci_info {
        uart_write_string("DTB parsing successful!\r\n");
        uart_write_string("PCI ECAM base: 0x");
        print_hex_simple(info.ecam_base);
        uart_write_string("\r\n");

        // Initialize interrupt-based input system
        uart_write_string("Initializing interrupt-based input system...\r\n");
        usb_hid::init_usb_hid();
        uart_write_string("Input system ready for GUI keyboard events!\r\n");

        // Initialize VirtIO input devices using DTB-provided addresses
        uart_write_string("Initializing VirtIO input devices with DTB addresses...\r\n");
        virtio_input::init_virtio_input_with_pci_base(info.ecam_base, info.mmio_base);
        uart_write_string("VirtIO input devices ready!\r\n");

        // Initialize VirtIO block devices
        uart_write_string("Initializing VirtIO block devices...\r\n");
        unsafe {
            BLOCK_DEVICES = Some(virtio_blk::VirtioBlkDevice::find_and_init(info.ecam_base, info.mmio_base));
        }

        // Create a local reference for easier access (must borrow from static)
        let blk_devices = unsafe { BLOCK_DEVICES.as_mut().unwrap() };

        if !blk_devices.is_empty() {
            uart_write_string("VirtIO block device initialized! Running read/write tests...\r\n");

            // Test 1: Write a pattern to sector 1000 (high sector to avoid filesystem collision)
            uart_write_string("\nTest 1: Writing pattern to sector 1000...\r\n");
            let mut write_buffer = [0u8; 512];
            // Fill with a recognizable pattern
            for i in 0..512 {
                write_buffer[i] = (i % 256) as u8;
            }

            match blk_devices[0].write_sector(1000, &write_buffer) {
                Ok(()) => {
                    uart_write_string("Write successful! Pattern written to sector 1000.\r\n");
                }
                Err(e) => {
                    uart_write_string("ERROR: Failed to write sector: ");
                    uart_write_string(e);
                    uart_write_string("\r\n");
                }
            }

            // Test 2: Read it back and verify
            uart_write_string("\nTest 2: Reading back sector 1000 to verify...\r\n");
            let mut read_buffer = [0u8; 512];
            match blk_devices[0].read_sector(1000, &mut read_buffer) {
                Ok(()) => {
                    uart_write_string("Read successful! Verifying data...\r\n");

                    // Verify the data matches
                    let mut errors = 0;
                    for i in 0..512 {
                        if read_buffer[i] != write_buffer[i] {
                            errors += 1;
                        }
                    }

                    if errors == 0 {
                        uart_write_string("✓ VERIFICATION SUCCESS! All 512 bytes match!\r\n");
                        uart_write_string("First 16 bytes: ");
                        for i in 0..16 {
                            let byte = read_buffer[i];
                            let hex_chars = b"0123456789ABCDEF";
                            unsafe {
                                core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte >> 4) as usize]);
                                core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte & 0x0F) as usize]);
                                core::ptr::write_volatile(0x09000000 as *mut u8, b' ');
                            }
                        }
                        uart_write_string("\r\n");
                    } else {
                        uart_write_string(&alloc::format!(
                            "✗ VERIFICATION FAILED! {} bytes don't match!\r\n", errors
                        ));
                    }
                }
                Err(e) => {
                    uart_write_string("ERROR: Failed to read sector: ");
                    uart_write_string(e);
                    uart_write_string("\r\n");
                }
            }

            // Test 3: Read sector 0 (boot sector)
            uart_write_string("\nTest 3: Reading sector 0 (boot sector)...\r\n");
            let mut buffer = [0u8; 512];
            match blk_devices[0].read_sector(0, &mut buffer) {
                Ok(()) => {
                    uart_write_string("Sector 0 read successfully! First 16 bytes:\r\n");
                    for i in 0..16 {
                        let byte = buffer[i];
                        let hex_chars = b"0123456789ABCDEF";
                        unsafe {
                            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte >> 4) as usize]);
                            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte & 0x0F) as usize]);
                            core::ptr::write_volatile(0x09000000 as *mut u8, b' ');
                        }
                    }
                    uart_write_string("\r\n");
                }
                Err(e) => {
                    uart_write_string("ERROR: Failed to read sector 0: ");
                    uart_write_string(e);
                    uart_write_string("\r\n");
                }
            }

            uart_write_string("\n=== VirtIO Block Device Tests Complete! ===\r\n");

            // Initialize VirtIO network devices
            uart_write_string("\n=== Initializing VirtIO Network Devices ===\r\n");
            unsafe {
                NET_DEVICES = Some(virtio_net::VirtioNetDevice::find_and_init(info.ecam_base, info.mmio_base));
            }

            let net_devices = unsafe { NET_DEVICES.as_mut().unwrap() };
            if !net_devices.is_empty() {
                uart_write_string(&alloc::format!(
                    "Found {} network device(s)\r\n", net_devices.len()
                ));

                // Initialize ARP cache
                unsafe {
                    ARP_CACHE = Some(network::ArpCache::new());
                }

                // Add receive buffers to the first network device
                if let Err(e) = net_devices[0].add_receive_buffers(16) {
                    uart_write_string(&alloc::format!(
                        "Failed to add receive buffers: {}\r\n", e
                    ));
                } else {
                    uart_write_string("Added 16 receive buffers to network device\r\n");
                }

                uart_write_string(&alloc::format!(
                    "Network configuration: IP={}.{}.{}.{} Gateway={}.{}.{}.{}\r\n",
                    unsafe { OUR_IP[0] }, unsafe { OUR_IP[1] }, unsafe { OUR_IP[2] }, unsafe { OUR_IP[3] },
                    unsafe { GATEWAY_IP[0] }, unsafe { GATEWAY_IP[1] }, unsafe { GATEWAY_IP[2] }, unsafe { GATEWAY_IP[3] }
                ));

                uart_write_string("Network device ready!\r\n");
            } else {
                uart_write_string("No network devices found\r\n");
            }

            // Test filesystem
            uart_write_string("\n=== Testing SimpleFS Filesystem ===\r\n");

            // Determine which device to use for persistent storage
            // Strategy: Use the last device in the array (most likely to be the data disk)
            let fs_device_idx = blk_devices.len() - 1;

            if blk_devices.len() >= 2 {
                uart_write_string(&alloc::format!(
                    "Found {} block devices - using device {} for persistent storage\r\n",
                    blk_devices.len(), fs_device_idx
                ));
            } else {
                uart_write_string(&alloc::format!(
                    "Found only 1 block device - using device {} (assuming persistent)\r\n",
                    fs_device_idx
                ));
            }

            // Try to mount existing filesystem first
            uart_write_string("\nTrying to mount existing filesystem...\r\n");
            let mut fs_result = filesystem::SimpleFilesystem::mount(&mut blk_devices[fs_device_idx]);

            if fs_result.is_err() {
                // No existing filesystem, format and mount
                uart_write_string("No existing filesystem found. Formatting disk...\r\n");
                match filesystem::SimpleFilesystem::format(&mut blk_devices[fs_device_idx], 20480) {
                    Ok(()) => {
                        uart_write_string("✓ Disk formatted successfully!\r\n");
                    }
                    Err(e) => {
                        uart_write_string("✗ Format failed: ");
                        uart_write_string(e);
                        uart_write_string("\r\n");
                    }
                }

                // Mount the freshly formatted filesystem
                uart_write_string("\nMounting filesystem...\r\n");
                fs_result = filesystem::SimpleFilesystem::mount(&mut blk_devices[fs_device_idx]);
            }

            match fs_result {
                Ok(mut fs) => {
                    uart_write_string(&alloc::format!(
                        "✓ Filesystem mounted! {} files found\r\n",
                        fs.file_count()
                    ));

                    // List files
                    let files = fs.list_files();
                    let file_count = fs.file_count();

                    // Skip initialization tests if filesystem already has files
                    if file_count > 0 {
                        uart_write_string(&alloc::format!(
                            "Existing filesystem with {} file entries - skipping initialization tests\r\n",
                            file_count
                        ));

                        if !files.is_empty() {
                            uart_write_string("Visible files:\r\n");
                            for file in &files {
                                uart_write_string(&alloc::format!(
                                    "  - {} ({} bytes)\r\n",
                                    file.get_name(),
                                    file.get_size_bytes()
                                ));
                            }
                        } else {
                            uart_write_string("Warning: file_count > 0 but list_files() returned empty (corruption?)\r\n");
                        }
                    } else {
                        // Fresh filesystem - run initialization tests
                        uart_write_string("\n--- Testing File Operations ---\r\n");
                        let is_empty = files.is_empty();
                        if is_empty {
                            uart_write_string("✓ File list is empty (as expected on fresh format)\r\n");
                        }

                        // Create a welcome file on fresh filesystem
                        if is_empty {
                        uart_write_string("\nCreating welcome file...\r\n");
                        match fs.create_file(&mut blk_devices[fs_device_idx], "welcome", 256) {
                            Ok(()) => {
                                uart_write_string("✓ Created 'welcome' file\r\n");
                                // Write welcome message
                                let welcome_msg = b"Welcome to rOSt!\n\nThis is a Rust ARM64 Operating System.\n\nTry opening the Files menu to browse files,\nor use the Terminal to run shell commands.";
                                match fs.write_file(&mut blk_devices[fs_device_idx], "welcome", welcome_msg) {
                                    Ok(()) => uart_write_string("✓ Wrote welcome message\r\n"),
                                    Err(e) => uart_write_string(&alloc::format!("✗ Failed to write: {}\r\n", e)),
                                }
                            }
                            Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                        }
                    }

                    // List files
                    uart_write_string("\nListing files...\r\n");
                    let files = fs.list_files();
                    uart_write_string(&alloc::format!("✓ Found {} file(s):\r\n", files.len()));
                    for file in &files {
                        uart_write_string(&alloc::format!(
                            "  - '{}': {} bytes ({} sectors) at sector {}\r\n",
                            file.get_name(),
                            file.get_size_bytes(),
                            file.get_size_sectors(),
                            file.get_start_sector()
                        ));
                    }

                    // Filesystem tests removed - OS is ready for use!
                    if false { // Disabled filesystem tests
                        // Test duplicate file creation (should fail)
                        uart_write_string("\nTrying to create duplicate file...\r\n");
                        match fs.create_file(&mut blk_devices[fs_device_idx], "hello", 50) {
                            Ok(()) => uart_write_string("✗ Should have failed!\r\n"),
                            Err(e) => uart_write_string(&alloc::format!("✓ Correctly rejected: {}\r\n", e)),
                        }

                        // Delete a file
                        uart_write_string("\nDeleting 'test' file...\r\n");
                        match fs.delete_file(&mut blk_devices[fs_device_idx], "test") {
                            Ok(()) => uart_write_string("✓ File deleted\r\n"),
                            Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                        }

                        // List files again
                        uart_write_string("\nListing files after deletion...\r\n");
                        let files = fs.list_files();
                        uart_write_string(&alloc::format!("✓ Found {} file(s):\r\n", files.len()));
                        for file in &files {
                            uart_write_string(&alloc::format!(
                                "  - '{}': {} bytes\r\n",
                                file.get_name(),
                                file.get_size_bytes()
                            ));
                        }

                        // Try to delete non-existent file
                        uart_write_string("\nTrying to delete non-existent file...\r\n");
                        match fs.delete_file(&mut blk_devices[fs_device_idx], "missing") {
                            Ok(()) => uart_write_string("✗ Should have failed!\r\n"),
                            Err(e) => uart_write_string(&alloc::format!("✓ Correctly rejected: {}\r\n", e)),
                        }

                        // Test file read/write
                        uart_write_string("\n--- Testing File Read/Write ---\r\n");

                        // Write data to 'hello' file
                        uart_write_string("\nWriting data to 'hello' file...\r\n");
                        let test_data = b"Hello, World! This is a test message.";
                        match fs.write_file(&mut blk_devices[fs_device_idx], "hello", test_data) {
                            Ok(()) => uart_write_string(&alloc::format!("✓ Wrote {} bytes\r\n", test_data.len())),
                            Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                        }

                        // Read data back from 'hello' file
                        uart_write_string("\nReading data from 'hello' file...\r\n");
                        let mut read_buffer = [0u8; 100];
                        match fs.read_file(&mut blk_devices[fs_device_idx], "hello", &mut read_buffer) {
                        Ok(bytes_read) => {
                            uart_write_string(&alloc::format!("✓ Read {} bytes\r\n", bytes_read));

                            // Verify data
                            let matches = read_buffer[..test_data.len()] == test_data[..];
                            if matches {
                                uart_write_string("✓ Data verification SUCCESS! Content matches:\r\n");
                                uart_write_string("  \"");
                                uart_write_string(core::str::from_utf8(&read_buffer[..test_data.len()]).unwrap_or("???"));
                                uart_write_string("\"\r\n");
                            } else {
                                uart_write_string("✗ Data verification FAILED!\r\n");
                            }
                        }
                        Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                    }

                        // Write to 'data' file (multiple sectors)
                        uart_write_string("\nWriting 400 bytes to 'data' file...\r\n");
                        let mut big_data = [0u8; 400];
                        for i in 0..400 {
                            big_data[i] = (i % 256) as u8;
                        }
                        match fs.write_file(&mut blk_devices[fs_device_idx], "data", &big_data) {
                            Ok(()) => uart_write_string("✓ Wrote 400 bytes\r\n"),
                            Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                        }

                        // Read it back
                        uart_write_string("\nReading 400 bytes from 'data' file...\r\n");
                        let mut big_read_buffer = [0u8; 512];
                        match fs.read_file(&mut blk_devices[fs_device_idx], "data", &mut big_read_buffer) {
                        Ok(bytes_read) => {
                            uart_write_string(&alloc::format!("✓ Read {} bytes\r\n", bytes_read));

                            // Verify
                            let matches = big_read_buffer[..400] == big_data[..];
                            if matches {
                                uart_write_string("✓ Data verification SUCCESS! All 400 bytes match!\r\n");
                                uart_write_string("  First 16 bytes: ");
                                for i in 0..16 {
                                    let byte = big_read_buffer[i];
                                    let hex_chars = b"0123456789ABCDEF";
                                    unsafe {
                                        core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte >> 4) as usize]);
                                        core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte & 0x0F) as usize]);
                                        core::ptr::write_volatile(0x09000000 as *mut u8, b' ');
                                    }
                                }
                                uart_write_string("\r\n");
                            } else {
                                uart_write_string("✗ Data verification FAILED!\r\n");
                            }
                            }
                            Err(e) => uart_write_string(&alloc::format!("✗ Failed: {}\r\n", e)),
                        }
                        } // End of is_empty check
                    } // End of fresh filesystem tests / else block for file_count check

                    // Filesystem mounted successfully
                    // Shells will be created when terminal windows are opened
                    uart_write_string("Filesystem ready!\r\n");
                }
                Err(e) => {
                    uart_write_string("✗ Mount failed: ");
                    uart_write_string(e);
                    uart_write_string("\r\n");
                }
            }
        } else {
            uart_write_string("No VirtIO block devices found\r\n");
        }

    } else {
        uart_write_string("WARNING: DTB parsing failed, using fallback initialization\r\n");
        usb_hid::init_usb_hid();
        virtio_input::init_virtio_input();
    }

    uart_write_string("\r\n");
    uart_write_string("================================\r\n");
    uart_write_string("  Rust OS - Interactive Shell  \r\n");
    uart_write_string("================================\r\n");
    uart_write_string("Type 'help' for available commands\r\n");
    uart_write_string("\r\n");

    uart_write_string("Kernel ready! Open a terminal window from the menu.\r\n");

    let mut needs_full_render = true; // Force initial render
    let mut last_minute = rtc::get_datetime().minute; // Track last rendered minute

    loop {
        // Check if minute has changed - redraw clock every minute
        let current_minute = rtc::get_datetime().minute;
        if current_minute != last_minute {
            last_minute = current_minute;
            needs_full_render = true;
        }

        // Poll VirtIO input devices for real trackpad/keyboard input
        virtio_input::poll_virtio_input();

        // Process queued input events - returns (needs_full_redraw, needs_cursor_redraw)
        let (needs_full_redraw, needs_cursor_redraw) = usb_hid::test_input_events();
        if needs_full_redraw {
            needs_full_render = true;
        }

        // Update snake games and only render if any game changed state
        if !window_manager::get_all_snakes().is_empty() {
            if snake::update_all_games() {
                needs_full_render = true;
            }
        }

        // Render desktop with windows and cursor
        if fb_info.base_address != 0 {
            if needs_full_render {
                // Full redraw to back buffer - clear, render windows, console, cursor
                framebuffer::clear_screen(0xFF1A1A1A);
                window_manager::render();

                // Render all terminals INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in window_manager::get_all_terminals() {
                    console::render_at(instance_id, cx, cy, cw, ch);
                }

                // Render all editors INSIDE their windows
                for (instance_id, cx, cy, _cw, ch) in window_manager::get_all_editors() {
                    editor::render_at(instance_id, cx, cy, ch);
                }

                // Render all file explorers INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in window_manager::get_all_file_explorers() {
                    file_explorer::render_at(instance_id, cx, cy, cw, ch);
                }

                // Render all snake games INSIDE their windows (already updated above)
                for (instance_id, cx, cy, cw, ch) in window_manager::get_all_snakes() {
                    if let Some(game) = snake::get_snake_game(instance_id) {
                        let fb = framebuffer::get_back_buffer();
                        let (screen_width, _) = framebuffer::get_screen_dimensions();

                        // Center the game in the window
                        let game_width = game.width() as i32;
                        let game_height = game.height() as i32;
                        let centered_x = cx + ((cw as i32 - game_width) / 2).max(0);
                        let centered_y = cy + ((ch as i32 - game_height) / 2).max(0);

                        game.render(fb, screen_width as usize, ch as usize, centered_x as usize, centered_y as usize);
                    }
                }

                // Render all browser windows INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in window_manager::get_all_browsers() {
                    browser::render_at(instance_id, cx as usize, cy as usize, cw as usize, ch as usize);
                }

                // Hardware cursor is now handled by VirtIO GPU, no need for software cursor
                // framebuffer::draw_cursor();

                // Swap buffers - copy back buffer to screen in one fast operation
                // This eliminates ALL flickering!
                framebuffer::swap_buffers();

                // Flush to VirtIO GPU display
                unsafe {
                    if let Some(ref mut gpu) = GPU_DRIVER {
                        let _ = gpu.flush_display();
                    }
                }

                needs_full_render = false;
            } else if needs_cursor_redraw {
                // Hardware cursor is now handled by VirtIO GPU
                // No need to redraw software cursor or flush for cursor-only updates
                // The hardware cursor updates happen in handle_mouse_movement()
            }
        }

        // Small delay to prevent CPU overload
        for _ in 0..10000 {
            unsafe { core::arch::asm!("nop"); }
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

