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
pub mod thread;
pub mod scheduler;
pub mod syscall;
pub mod syscall_ipc;
pub mod userspace_test;
pub mod filedesc;
pub mod elf_loader;
pub mod embedded_apps;

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

// Static network configuration for QEMU user-mode networking (10.0.2.x)
pub static mut OUR_IP: [u8; 4] = [10, 0, 2, 15];  // QEMU user-mode guest IP (default)
pub static mut GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];  // QEMU user-mode gateway

// Static for GPU driver and cursor position
static mut GPU_DRIVER: Option<drivers::virtio::gpu::VirtioGpuDriver> = None;
static mut CURSOR_X: u32 = 0;
static mut CURSOR_Y: u32 = 0;
static mut SCREEN_WIDTH: u32 = 0;
static mut SCREEN_HEIGHT: u32 = 0;

// Window manager PID (for IPC communication)
// CRITICAL: Must be atomic for cross-thread visibility
use core::sync::atomic::{AtomicUsize, Ordering};
static WINDOW_MANAGER_PID: AtomicUsize = AtomicUsize::new(0);

// Boot info and framebuffer for kernel_init_high_half
static mut BOOT_INFO: Option<&'static BootInfo> = None;
static mut GPU_FRAMEBUFFER_INFO: Option<crate::gui::framebuffer::FramebufferInfo> = None;

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

/// Kernel-side message send (for kernel threads to send to other processes)
/// Returns 0 on success, negative on error
fn kernel_send_message(dest_pid: u32, data: &[u8]) -> i64 {
    use crate::kernel::syscall::{IpcMessage, MAX_MESSAGE_SIZE};

    if data.len() > MAX_MESSAGE_SIZE {
        return -1; // Message too large
    }

    // Create message structure
    let mut msg = IpcMessage {
        sender_pid: 0, // Will be filled by dest process if needed
        data_len: data.len() as u32,
        data: [0u8; MAX_MESSAGE_SIZE],
    };
    msg.data[..data.len()].copy_from_slice(data);

    // Push message to destination process queue
    let result = thread::with_process_mut(dest_pid as usize, |process| {
        process.message_queue.push(msg)
    });

    match result {
        Some(true) => 0,  // Success
        Some(false) => -2, // Queue full
        None => -3,       // Process not found
    }
}

/// Kernel-side message receive (non-blocking, for kernel threads)
/// Returns number of bytes received (0 if no message available)
fn kernel_recv_message(buf: &mut [u8; 256]) -> i64 {
    // Get current thread's process ID
    let scheduler = scheduler::SCHEDULER.lock();
    let process_id = if let Some(thread_id) = scheduler.current_thread {
        scheduler.threads.iter()
            .find(|t| t.id == thread_id)
            .map(|t| t.process_id)
    } else {
        None
    };
    drop(scheduler); // Release lock before accessing process

    if let Some(pid) = process_id {
        // Try to pop a message from the current process's queue
        let msg = thread::with_process_mut(pid, |process| {
            process.message_queue.pop()
        });

        if let Some(Some(msg)) = msg {
            // Copy message data to buffer
            let copy_len = core::cmp::min(msg.data_len as usize, buf.len());
            buf[..copy_len].copy_from_slice(&msg.data[..copy_len]);
            return copy_len as i64;
        }
    }

    0 // No message available or no current thread
}

/// Kernel GUI thread - handles input polling, rendering, and IPC with WM
fn kernel_gui_thread() {
    uart_write_string("[GUI-THREAD] Kernel GUI thread started\r\n");

    // CRITICAL: Enable interrupts for this thread!
    // Kernel threads don't save/restore DAIF during context switches,
    // so we must explicitly enable interrupts when each thread starts.
    unsafe {
        core::arch::asm!("msr daifclr, #2"); // Clear IRQ mask
    }

    // Get framebuffer info from global
    uart_write_string("[GUI-THREAD] Getting framebuffer info...\r\n");
    let fb_info = unsafe { GPU_FRAMEBUFFER_INFO.unwrap() };
    uart_write_string("[GUI-THREAD] Got FB info\r\n");

    uart_write_string("[GUI-THREAD] Starting main loop - microkernel input forwarder\r\n");
    loop {
        // MICROKERNEL: Kernel only polls hardware and forwards to userspace WM
        // No kernel-side window management, routing, or rendering

        // FIRST: Check for WM responses from previous input events
        unsafe {
            let wm_pid = WINDOW_MANAGER_PID.load(Ordering::Acquire);
            if wm_pid > 0 {
                // Drain all pending WM responses (non-blocking)
                loop {
                    let mut response_buf = [0u8; 256];
                    let result = kernel_recv_message(&mut response_buf);

                    if result <= 0 {
                        break; // No more messages
                    }

                    // Handle WM response
                    match response_buf[0] {
                        0 => {
                            // RouteInput - WM wants us to forward input to a specific window (process)
                            let window_id = usize::from_le_bytes([
                                response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                response_buf[5], response_buf[6], response_buf[7], response_buf[8],
                            ]);

                            // Forward the event to the target process
                            // The message is already in the correct format for WMToKernel::RouteInput
                            let _ = kernel_send_message(window_id as u32, &response_buf);
                        }
                        1 => {
                            // RequestFocus - WM wants to change window focus
                            let window_id = usize::from_le_bytes([
                                response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                response_buf[5], response_buf[6], response_buf[7], response_buf[8],
                            ]);
                            uart_write_string("[KERNEL] Got RequestFocus, sending SetFocus\r\n");
                            // Send SetFocus message back to WM
                            let mut msg_buf = [0u8; 256];
                            msg_buf[0] = 3; // KernelToWM::SetFocus type
                            msg_buf[1..9].copy_from_slice(&window_id.to_le_bytes());
                            let _ = kernel_send_message(wm_pid as u32, &msg_buf);
                        }
                        2 => {
                            // RequestClose - WM wants to kill a window
                            let window_id = usize::from_le_bytes([
                                response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                response_buf[5], response_buf[6], response_buf[7], response_buf[8],
                            ]);
                            syscall::sys_kill(window_id as u64);

                            // Notify WM to remove the window from its list
                            let mut msg_buf = [0u8; 256];
                            msg_buf[0] = 2; // KernelToWM::CloseWindow type
                            msg_buf[1..9].copy_from_slice(&window_id.to_le_bytes());
                            let _ = kernel_send_message(wm_pid as u32, &msg_buf);
                        }
                        _ => {} // NoAction or unknown
                    }
                }
            }
        }

        // Poll VirtIO input devices for real trackpad/keyboard input
        drivers::virtio::input::poll_virtio_input();

        // Forward input events to window manager via IPC
        // PHASE 2: IPC-based input routing
        unsafe {
            if WINDOW_MANAGER_PID.load(Ordering::Acquire) > 0 {
                // Poll for input events and forward each one to WM
                let mut event_count = 0;
                while let Some(kernel_event) = drivers::input_events::get_input_event() {
                    event_count += 1;
                    // CRITICAL: No heap allocations in hot path (competes with ELF loading)
                    // Convert kernel InputEvent to librost InputEvent format
                    let (event_type, key, modifiers, button, pressed, x_delta, y_delta, wheel_delta) = match kernel_event {
                        drivers::input_events::InputEvent::KeyPressed { key, modifiers } => {
                            (1u32, key, modifiers, 0u8, 0u8, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::KeyReleased { key, modifiers } => {
                            (2u32, key, modifiers, 0u8, 0u8, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::MouseMove { x_delta, y_delta } => {
                            // Update cursor position
                            let new_x = (CURSOR_X as i32 + x_delta as i32).max(0).min(SCREEN_WIDTH as i32 - 1);
                            let new_y = (CURSOR_Y as i32 + y_delta as i32).max(0).min(SCREEN_HEIGHT as i32 - 1);
                            CURSOR_X = new_x as u32;
                            CURSOR_Y = new_y as u32;
                            crate::gui::framebuffer::set_cursor_pos(CURSOR_X as i32, CURSOR_Y as i32);

                            // Update VirtIO GPU hardware cursor
                            if let Some(ref mut gpu) = GPU_DRIVER {
                                let _ = gpu.update_cursor(CURSOR_X, CURSOR_Y, 0, 0);
                            }

                            (3u32, 0u8, 0u8, 0u8, 0u8, x_delta, y_delta, 0i8)
                        }
                        drivers::input_events::InputEvent::MouseButton { button, pressed } => {
                            (4u32, 0u8, 0u8, button, if pressed { 1 } else { 0 }, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::MouseWheel { delta } => {
                            (5u32, 0u8, 0u8, 0u8, 0u8, 0i8, 0i8, delta)
                        }
                    };

                    // Get my PID to include in message
                    let my_pid = {
                        let sched = scheduler::SCHEDULER.lock();
                        sched.current_thread.and_then(|tid| {
                            sched.threads.iter().find(|t| t.id == tid).map(|t| t.process_id)
                        }).unwrap_or(0)
                    };

                    // Create IPC message with current cursor position and event data
                    let mut msg_buf = [0u8; 256];
                    msg_buf[0] = 0; // KernelToWM::InputEvent type
                    msg_buf[1..5].copy_from_slice(&(my_pid as u32).to_le_bytes());
                    msg_buf[5..9].copy_from_slice(&(CURSOR_X as i32).to_le_bytes());
                    msg_buf[9..13].copy_from_slice(&(CURSOR_Y as i32).to_le_bytes());
                    msg_buf[13..17].copy_from_slice(&event_type.to_le_bytes());
                    msg_buf[17] = key;
                    msg_buf[18] = modifiers;
                    msg_buf[19] = button;
                    msg_buf[20] = pressed;
                    msg_buf[21] = x_delta as u8;
                    msg_buf[22] = y_delta as u8;
                    msg_buf[23] = wheel_delta as u8;

                    // Send to window manager (kernel-side IPC)
                    // Response will be processed at the start of next loop iteration
                    let result = kernel_send_message(WINDOW_MANAGER_PID.load(Ordering::Acquire) as u32, &msg_buf);

                    // If queue is full, stop sending and let WM catch up
                    if result == -2 {
                        break; // Queue full, skip remaining events this iteration
                    }
                }
                // CRITICAL: No debug output here - format!() competes with ELF loader
            }
        }

        // Poll network stack (process packets, timers, etc.)
        // This is kernel-level because smoltcp runs in kernel space
        unsafe {
            if let Some(ref mut stack) = NETWORK_STACK {
                stack.poll();
            }
        }

        // Small delay to prevent CPU overload
        for _ in 0..10000 {
            unsafe { core::arch::asm!("nop"); }
        }

        // Yield to scheduler so other threads can run
        thread::yield_now();
    }
}

/// Main kernel entry point after UEFI boot services are exited
#[no_mangle]
pub extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // First thing - prove we made it to the kernel!
    uart_write_string("KERNEL STARTED! ExitBootServices SUCCESS!\r\n");
    uart_write_string("Initializing Rust OS kernel...\r\n");

    // Store boot_info globally for kernel_init_high_half
    unsafe {
        BOOT_INFO = Some(boot_info);
    }
    
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
                                    let fb_info = crate::gui::framebuffer::FramebufferInfo {
                                        base_address: fb_addr,
                                        size: (height * stride) as usize,
                                        width,
                                        height,
                                        pixels_per_scanline: stride / 4,
                                        pixel_format: crate::gui::framebuffer::PixelFormat::Rgb,
                                    };
                                    gpu_framebuffer_info = Some(fb_info);

                                    // Store GPU driver, framebuffer, and screen info globally
                                    unsafe {
                                        GPU_FRAMEBUFFER_INFO = Some(fb_info);
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

    // Use VirtIO-GPU framebuffer if available, otherwise fallback
    let fb_info = gpu_framebuffer_info.as_ref().unwrap_or(&boot_info.framebuffer);
    uart_write_string("Initializing framebuffer...\r\n");
    crate::gui::framebuffer::init(fb_info);

    uart_write_string("=== RUST OS KERNEL RUNNING SUCCESSFULLY! ===\r\n");
    uart_write_string("ExitBootServices worked! We're in kernel space!\r\n");
    uart_write_string("VirtIO-GPU driver loaded and initialized!\r\n");
    uart_write_string("This is a major milestone - the core OS is working!\r\n");

    if fb_info.base_address != 0 {
        uart_write_string("Graphics framebuffer is active - userspace WM will take over!\r\n");

        // Initialize RTC (hardware driver - OK to keep in kernel)
        drivers::rtc::init();
        uart_write_string("RTC initialized!\r\n");
    } else {
        uart_write_string("No framebuffer available - running in text mode\r\n");
    }
    
    // Physical memory already initialized
    uart_write_string("Physical memory: OK\r\n");
    
    // Check current exception level
    let current_el = interrupts::get_current_exception_level();
    uart_write_string("Current Exception Level: EL");
    match current_el {
        0 => uart_write_string("0 (User Mode)\r\n"),
        1 => uart_write_string("1 (Kernel Mode)\r\n"),
        2 => uart_write_string("2 (Hypervisor Mode)\r\n"),
        3 => uart_write_string("3 (Secure Monitor Mode)\r\n"),
        _ => uart_write_string("Unknown\r\n"),
    }

    // Set up exception vectors for ARM64
    interrupts::init_exception_vectors();
    uart_write_string("Exception vectors: OK\r\n");

    // Initialize virtual memory (page tables)
    // NOTE: This function never returns! It jumps to the high-half kernel
    // The rest of initialization continues in kernel_init_high_half()
    memory::init_virtual_memory();

    // UNREACHABLE - kernel execution continues in kernel_init_high_half()
    unreachable!("init_virtual_memory() should never return!");
}

/// Kernel initialization continuation after higher-half transition
/// This function is called from the high-half (0xFFFF...) after the MMU transition
#[no_mangle]
pub extern "C" fn kernel_init_high_half() -> ! {
    uart_write_string("Virtual memory: OK\r\n");
    uart_write_string("[KERNEL] Now running in higher-half (0xFFFF...)\r\n");

    // Re-initialize exception vectors with higher-half addresses
    interrupts::init_exception_vectors();
    uart_write_string("Exception vectors re-initialized for higher-half: OK\r\n");

    // Retrieve boot_info and framebuffer info from globals
    let boot_info = unsafe { BOOT_INFO.expect("BOOT_INFO not set") };
    let gpu_framebuffer_info = unsafe { GPU_FRAMEBUFFER_INFO };
    let fb_info = gpu_framebuffer_info.as_ref().unwrap_or(&boot_info.framebuffer);

    // Initialize interrupt controller (GIC)
    interrupts::init_gic();
    uart_write_string("GIC interrupt controller: OK\r\n");
    
    // Set up timer
    interrupts::init_timer();
    uart_write_string("Timer: OK\r\n");

    // Initialize process management system
    thread::init_process_manager();
    uart_write_string("Process management: OK\r\n");
    
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
                }

                uart_write_string("Network device ready!\r\n");
            } else {
                uart_write_string("No network devices found\r\n");
                unsafe {
                    NET_DEVICES = Some(net_devices);
                }
            }

            // Filesystem handling moved to userspace file server (microkernel architecture)
            uart_write_string("\n=== Microkernel: Filesystem handled by userspace file server ===\r\n");
        } else {
            uart_write_string("No VirtIO block devices found\r\n");
        }

    } else {
        uart_write_string("WARNING: DTB parsing failed, using fallback initialization\r\n");
        drivers::input_events::init_usb_hid();
        drivers::virtio::input::init_virtio_input();
    }

    // ===== SPAWN ELF LOADER THREAD FIRST =====
    // CRITICAL: Load ELF files with ONLY this thread running to avoid allocator contention
    uart_write_string("\n=== Loading userspace applications ===\r\n");
    fn elf_loader_thread() {
        uart_write_string("[ELF-LOADER] Thread started\r\n");

        // Warm up allocator first
        for i in 0..10 {
            let _ = alloc::format!("init{}", i);
        }
        uart_write_string("[ELF-LOADER] Allocator warmed up\r\n");

        // Load ONLY the Window Manager - it will spawn apps on demand
        uart_write_string("[ELF-LOADER] Loading window manager...\r\n");
        uart_write_string("[ELF-LOADER] About to call load_elf_and_spawn...\r\n");
        let wm_pid = elf_loader::load_elf_and_spawn(embedded_apps::WINDOW_MANAGER_ELF);
        uart_write_string("[ELF-LOADER] load_elf_and_spawn returned\r\n");
        uart_write_string("[ELF-LOADER] WM loaded with PID: ");
        if wm_pid < 10 {
            unsafe {
                core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + wm_pid as u8);
            }
        }
        uart_write_string("\r\n");
        WINDOW_MANAGER_PID.store(wm_pid, Ordering::Release);

        // Load the File Server (microkernel filesystem service)
        uart_write_string("[ELF-LOADER] Loading file server...\r\n");
        let fs_pid = elf_loader::load_elf_and_spawn(embedded_apps::FILE_SERVER_ELF);
        uart_write_string("[ELF-LOADER] File server loaded with PID: ");
        if fs_pid < 10 {
            unsafe {
                core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + fs_pid as u8);
            }
        }
        uart_write_string("\r\n");

        uart_write_string("[ELF-LOADER] Microkernel boot: WM and File Server loaded, apps spawn on-demand\r\n");

        // NOW spawn the GUI thread after ELF loading completes
        let _gui_thread_id = {
            let mut sched = scheduler::SCHEDULER.lock();
            sched.spawn(kernel_gui_thread)
        };

        // This thread's job is done - yield forever
        loop {
            thread::yield_now();
        }
    }

    let _elf_loader_id = {
        let mut sched = scheduler::SCHEDULER.lock();
        sched.spawn(elf_loader_thread)
    };

    // CRITICAL: Enable interrupts so scheduler can run
    unsafe {
        core::arch::asm!("msr daifclr, #2"); // Clear IRQ mask bit
    }

    uart_write_string("✓ Boot complete, starting scheduler\r\n\r\n");

    // CRITICAL: Yield once to kickstart the scheduler
    uart_write_string("[BOOT] About to yield to scheduler for first time...\r\n");
    thread::yield_now();
    uart_write_string("[BOOT] Returned from first yield\r\n");

    // Boot thread now permanently yields to scheduler
    // Use WFI (wait for interrupt) to save power
    loop {
        unsafe {
            core::arch::asm!("wfi"); // Wait for interrupt, then immediately yield
        }
        thread::yield_now();
    }
}
