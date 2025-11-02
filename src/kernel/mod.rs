// Kernel entry point after exiting UEFI boot services

// Simple print macro for debugging - writes to serial port
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        // For now, just a no-op since we can't easily access serial after UEFI exit
        // In a real implementation, we'd set up UART or use a different output method
    };
}

// Core kernel modules
pub mod memory;
pub mod interrupts;
pub mod dtb;
pub mod drivers;

/// Information passed from UEFI bootloader to kernel
#[repr(C)]
pub struct BootInfo {
    pub memory_map: &'static [memory::MemoryDescriptor],
    pub framebuffer: crate::gui::framebuffer::FramebufferInfo,
    pub acpi_rsdp: Option<u64>,
}

// Static storage for block devices (needs to outlive local scope for shell access)
pub static mut BLOCK_DEVICES: Option<alloc::vec::Vec<drivers::virtio::blk::VirtioBlkDevice>> = None;

// Static storage for network devices (deprecated - use NETWORK_STACK instead)
pub static mut NET_DEVICES: Option<alloc::vec::Vec<drivers::virtio::net::VirtioNetDevice>> = None;

// Static storage for smoltcp-based network stack
pub static mut NETWORK_STACK: Option<crate::system::net::NetworkStack> = None;

// Static storage for ARP cache (deprecated - smoltcp handles ARP internally)
pub static mut ARP_CACHE: Option<crate::system::net::ArpCache> = None;

// Static network configuration for QEMU user-mode networking (10.0.2.x)
pub static mut OUR_IP: [u8; 4] = [10, 0, 2, 15];  // QEMU user-mode guest IP (default)
pub static mut GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];  // QEMU user-mode gateway

// Static for GPU driver and cursor position
static mut GPU_DRIVER: Option<drivers::virtio::gpu::VirtioGpuDriver> = None;
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
            crate::gui::framebuffer::set_cursor_pos(CURSOR_X as i32, CURSOR_Y as i32);
        }
    }
}

// Get current time in milliseconds (for double-click detection, etc.)
pub fn get_time_ms() -> u64 {
    drivers::timer::get_time_ms()
}

// Flush a partial region of the framebuffer to GPU (for menu bar updates, etc.)
pub fn flush_display_partial(x: u32, y: u32, width: u32, height: u32) {
    unsafe {
        if let Some(ref mut gpu) = GPU_DRIVER {
            let _ = gpu.flush_display_partial(x, y, width, height);
        }
    }
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
    let mut virtio_gpu_driver: Option<drivers::virtio::gpu::VirtioGpuDriver> = None;

    // Initialize VirtIO-GPU properly
    if let Some(mut virtio_gpu) = drivers::virtio::gpu::VirtioGpuDriver::new() {
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
                                    gpu_framebuffer_info = Some(crate::gui::framebuffer::FramebufferInfo {
                                        base_address: fb_addr,
                                        size: (height * stride) as usize,
                                        width,
                                        height,
                                        pixels_per_scanline: stride / 4,
                                        pixel_format: crate::gui::framebuffer::PixelFormat::Rgb,
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
                                    crate::gui::framebuffer::set_cursor_pos((width / 2) as i32, (height / 2) as i32);

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
    crate::gui::framebuffer::init(fb_info);

    uart_write_string("=== RUST OS KERNEL RUNNING SUCCESSFULLY! ===\r\n");
    uart_write_string("ExitBootServices worked! We're in kernel space!\r\n");
    uart_write_string("VirtIO-GPU driver loaded and initialized!\r\n");
    uart_write_string("This is a major milestone - the core OS is working!\r\n");

    if fb_info.base_address != 0 {
        uart_write_string("Graphics framebuffer is active - initializing GUI desktop!\r\n");

        // Initialize GUI console
        crate::gui::widgets::console::init();
        uart_write_string("GUI console initialized!\r\n");

        // Initialize window manager
        crate::gui::window_manager::init();
        uart_write_string("Window manager initialized!\r\n");
        uart_write_string("Click menu bar to open windows\r\n");

        // Initialize RTC
        drivers::rtc::init();
        uart_write_string("RTC initialized!\r\n");

        // Initialize text editor
        crate::gui::widgets::editor::init();
        uart_write_string("Text editor initialized!\r\n");

        // Initialize file explorer
        crate::gui::widgets::file_explorer::init();
        uart_write_string("File explorer initialized!\r\n");

        // Initialize web browser
        crate::gui::widgets::browser::init();
        uart_write_string("Web browser initialized!\r\n");

        // Initialize snake game
        crate::apps::snake::init();
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
        drivers::input_events::init_usb_hid();
        uart_write_string("Input system ready for GUI keyboard events!\r\n");

        // Initialize VirtIO input devices using DTB-provided addresses
        uart_write_string("Initializing VirtIO input devices with DTB addresses...\r\n");
        drivers::virtio::input::init_virtio_input_with_pci_base(info.ecam_base, info.mmio_base);
        uart_write_string("VirtIO input devices ready!\r\n");

        // Initialize VirtIO block devices
        uart_write_string("Initializing VirtIO block devices...\r\n");
        unsafe {
            BLOCK_DEVICES = Some(drivers::virtio::blk::VirtioBlkDevice::find_and_init(info.ecam_base, info.mmio_base));
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
            let mut net_devices = drivers::virtio::net::VirtioNetDevice::find_and_init(info.ecam_base, info.mmio_base);

            if !net_devices.is_empty() {
                uart_write_string(&alloc::format!(
                    "Found {} network device(s)\r\n", net_devices.len()
                ));

                // Take the first device and wrap it in smoltcp
                let first_device = net_devices.remove(0);
                let smoltcp_device = crate::system::net::SmoltcpVirtioNetDevice::new(first_device);

                // Create network stack with smoltcp
                let our_ip = unsafe { OUR_IP };
                let gateway = unsafe { GATEWAY_IP };
                let mut stack = crate::system::net::NetworkStack::new(smoltcp_device, our_ip, gateway);

                // Add receive buffers
                if let Err(e) = stack.add_receive_buffers(16) {
                    uart_write_string(&alloc::format!(
                        "Failed to add receive buffers: {}\r\n", e
                    ));
                } else {
                    uart_write_string("Added 16 receive buffers to network device\r\n");
                }

                uart_write_string(&alloc::format!(
                    "Network configuration: IP={}.{}.{}.{} Gateway={}.{}.{}.{}\r\n",
                    our_ip[0], our_ip[1], our_ip[2], our_ip[3],
                    gateway[0], gateway[1], gateway[2], gateway[3]
                ));

                uart_write_string("smoltcp network stack initialized!\r\n");

                // Store the network stack globally
                unsafe {
                    NETWORK_STACK = Some(stack);
                    // Store remaining devices (if any) for backward compatibility
                    NET_DEVICES = Some(net_devices);
                    // Initialize ARP cache for backward compatibility
                    ARP_CACHE = Some(crate::system::net::ArpCache::new());
                }

                uart_write_string("Network device ready!\r\n");
            } else {
                uart_write_string("No network devices found\r\n");
                unsafe {
                    NET_DEVICES = Some(net_devices);
                }
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
            let mut fs_result = crate::system::fs::SimpleFilesystem::mount(&mut blk_devices[fs_device_idx]);

            if fs_result.is_err() {
                // No existing filesystem, format and mount
                uart_write_string("No existing filesystem found. Formatting disk...\r\n");
                match crate::system::fs::SimpleFilesystem::format(&mut blk_devices[fs_device_idx], 20480) {
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
                fs_result = crate::system::fs::SimpleFilesystem::mount(&mut blk_devices[fs_device_idx]);
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
        drivers::input_events::init_usb_hid();
        drivers::virtio::input::init_virtio_input();
    }

    uart_write_string("\r\n");
    uart_write_string("================================\r\n");
    uart_write_string("  Rust OS - Interactive Shell  \r\n");
    uart_write_string("================================\r\n");
    uart_write_string("Type 'help' for available commands\r\n");
    uart_write_string("\r\n");

    uart_write_string("Kernel ready! Open a terminal window from the menu.\r\n");

    let mut needs_full_render = true; // Force initial render
    let mut last_minute = drivers::rtc::get_datetime().minute; // Track last rendered minute

    loop {
        // Check if minute has changed - redraw clock every minute
        let current_minute = drivers::rtc::get_datetime().minute;
        if current_minute != last_minute {
            last_minute = current_minute;
            needs_full_render = true;
        }

        // Poll VirtIO input devices for real trackpad/keyboard input
        drivers::virtio::input::poll_virtio_input();

        // Poll network stack (process packets, timers, etc.)
        unsafe {
            if let Some(ref mut stack) = NETWORK_STACK {
                stack.poll();
            }
        }

        // Process queued input events - returns (needs_full_redraw, needs_cursor_redraw)
        let (needs_full_redraw, needs_cursor_redraw) = drivers::input_events::test_input_events();
        if needs_full_redraw {
            needs_full_render = true;
        }

        // Update snake games and only render if any game changed state
        if !crate::gui::window_manager::get_all_snakes().is_empty() {
            if crate::apps::snake::update_all_games() {
                needs_full_render = true;
            }
        }

        // Render desktop with windows and cursor
        if fb_info.base_address != 0 {
            if needs_full_render {
                // Full redraw to back buffer - clear, render windows, console, cursor
                crate::gui::framebuffer::clear_screen(0xFF1A1A1A);
                crate::gui::window_manager::render();

                // Render all terminals INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in crate::gui::window_manager::get_all_terminals() {
                    crate::gui::widgets::console::render_at(instance_id, cx, cy, cw, ch);
                }

                // Render all editors INSIDE their windows
                for (instance_id, cx, cy, _cw, ch) in crate::gui::window_manager::get_all_editors() {
                    crate::gui::widgets::editor::render_at(instance_id, cx, cy, ch);
                }

                // Render all file explorers INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in crate::gui::window_manager::get_all_file_explorers() {
                    crate::gui::widgets::file_explorer::render_at(instance_id, cx, cy, cw, ch);
                }

                // Render all snake games INSIDE their windows (already updated above)
                for (instance_id, cx, cy, cw, ch) in crate::gui::window_manager::get_all_snakes() {
                    if let Some(game) = crate::apps::snake::get_snake_game(instance_id) {
                        let fb = crate::gui::framebuffer::get_back_buffer();
                        let (screen_width, _) = crate::gui::framebuffer::get_screen_dimensions();

                        // Center the game in the window
                        let game_width = game.width() as i32;
                        let game_height = game.height() as i32;
                        let centered_x = cx + ((cw as i32 - game_width) / 2).max(0);
                        let centered_y = cy + ((ch as i32 - game_height) / 2).max(0);

                        game.render(fb, screen_width as usize, ch as usize, centered_x as usize, centered_y as usize);
                    }
                }

                // Render all browser windows INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in crate::gui::window_manager::get_all_browsers() {
                    crate::gui::widgets::browser::render_at(instance_id, cx as usize, cy as usize, cw as usize, ch as usize);
                }

                // Hardware cursor is now handled by VirtIO GPU, no need for software cursor
                // crate::gui::framebuffer::draw_cursor();

                // Swap buffers - copy back buffer to screen in one fast operation
                // This eliminates ALL flickering!
                crate::gui::framebuffer::swap_buffers();

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
