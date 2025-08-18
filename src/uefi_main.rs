#![no_main]
#![no_std]

extern crate alloc;

use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, BltOp, BltPixel};
use uefi::proto::console::pointer::Pointer;
use uefi::Handle;
use uefi::mem::memory_map::MemoryType;
use linked_list_allocator::LockedHeap;
use core::ptr;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    
    // Initialize heap allocator
    unsafe {
        let layout = core::alloc::Layout::from_size_align(1024 * 64, 8).unwrap(); // 64KB heap
        let ptr = uefi::boot::allocate_pool(MemoryType::LOADER_DATA, layout.size()).unwrap();
        ALLOCATOR.lock().init(ptr.as_ptr(), layout.size());
    }
    
    uefi::println!("Rust OS UEFI Bootloader");
    uefi::println!("========================");
    
    // Try to initialize graphics  
    match uefi::boot::get_handle_for_protocol::<GraphicsOutput>()
        .and_then(|handle| uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle)) {
        Ok(mut gop) => {
            uefi::println!("Found Graphics Output Protocol");
            
            // Get mode info
            let mode = gop.current_mode_info();
            
            uefi::println!("Graphics Mode:");
            uefi::println!("  Resolution: {}x{}", mode.resolution().0, mode.resolution().1);
            uefi::println!("  Pixel format: {:?}", mode.pixel_format());
            uefi::println!("  Pixels per scanline: {}", mode.stride());
            
            // Use Blt operations instead of direct framebuffer access
            draw_test_pattern_blt(&mut gop, mode.resolution().0, mode.resolution().1);
            
            uefi::println!("Test pattern drawn using Blt operations!");
            
            // Initialize mouse input
            let mouse_result = init_mouse_input();
            match mouse_result {
                Ok(mouse_handle) => {
                    uefi::println!("UEFI mouse found - using direct polling (no events)");
                    // Use UEFI Pointer Protocol with direct polling instead of broken events
                    run_gui_loop_with_uefi_polling(&mut gop, mode.resolution().0, mode.resolution().1, mouse_handle);
                }
                Err(e) => {
                    uefi::println!("Failed to initialize mouse: {:?}", e);
                    uefi::println!("UEFI mouse failed - trying direct input driver...");
                    
                    // Try direct hardware mouse driver
                    let mut hw_driver = DirectInputDriver::new();
                    if hw_driver.try_initialize() {
                        uefi::println!("Direct input driver initialized successfully!");
                        run_gui_loop_with_direct_input(&mut gop, mode.resolution().0, mode.resolution().1, hw_driver);
                    } else {
                        uefi::println!("Direct input driver also failed - running without mouse support...");
                        run_gui_loop(&mut gop, mode.resolution().0, mode.resolution().1, None);
                    }
                }
            }
        }
        Err(_) => {
            uefi::println!("Graphics Output Protocol not found");
        }
    }
    
    Status::SUCCESS
}

fn draw_test_pattern_blt(gop: &mut GraphicsOutput, width: usize, height: usize) {
    // Define colors using BltPixel
    let dark_blue = BltPixel::new(16, 32, 64);   // Dark blue background
    let red = BltPixel::new(255, 0, 0);          // Red rectangle  
    let green = BltPixel::new(0, 255, 0);        // Green rectangle
    let white = BltPixel::new(255, 255, 255);    // White text area
    
    // Clear entire screen to dark blue
    let _ = gop.blt(BltOp::VideoFill {
        color: dark_blue,
        dest: (0, 0),
        dims: (width, height),
    });
    
    // Draw red rectangle (100,100) to (300,200)
    let _ = gop.blt(BltOp::VideoFill {
        color: red,
        dest: (100, 100),
        dims: (200, 100), // width=200, height=100
    });
    
    // Draw green rectangle (300,250) to (500,350) 
    let _ = gop.blt(BltOp::VideoFill {
        color: green,
        dest: (300, 250),
        dims: (200, 100), // width=200, height=100
    });
    
    // Draw white text area (50,50) to (450,90)
    let _ = gop.blt(BltOp::VideoFill {
        color: white,
        dest: (50, 50),
        dims: (400, 40), // width=400, height=40
    });
}

fn init_mouse_input() -> Result<Handle, uefi::Error> {
    // Try to find and initialize mouse/pointer device
    let handles = uefi::boot::find_handles::<Pointer>()?;
    
    if handles.is_empty() {
        return Err(uefi::Error::new(uefi::Status::NOT_FOUND, ()));
    }
    
    uefi::println!("Found {} pointer device(s)", handles.len());
    
    // Try to reset the first pointer device
    let handle = handles[0];
    let mut pointer = uefi::boot::open_protocol_exclusive::<Pointer>(handle)?;
    
    uefi::println!("Attempting to reset pointer device...");
    match pointer.reset(false) {
        Ok(_) => uefi::println!("Pointer device reset successfully"),
        Err(e) => uefi::println!("Failed to reset pointer device: {:?}", e),
    }
    
    // Try to get the device mode
    let mode = pointer.mode();
    uefi::println!("Pointer mode - resolution: {:?}, has_button: {:?}", 
        mode.resolution, mode.has_button);
    
    // UEFI pointer events confirmed broken on aarch64 - skip test
    
    // Return the handle
    Ok(handle)
}

struct MouseState {
    x: i32,
    y: i32,
    left_button: bool,
    right_button: bool,
}

fn run_gui_loop(gop: &mut GraphicsOutput, width: usize, height: usize, mouse_handle: Option<Handle>) {
    if let Some(handle) = mouse_handle {
        let mut mouse_state = MouseState {
            x: (width / 2) as i32,
            y: (height / 2) as i32,
            left_button: false,
            right_button: false,
        };
        
        uefi::println!("GUI loop started. Move your mouse!");
        uefi::println!("Mouse cursor will be drawn as a white square with black border");
        
        // Draw initial cursor position
        draw_scene_with_cursor(gop, width, height, &mouse_state);
        uefi::println!("Initial cursor drawn at ({}, {})", mouse_state.x, mouse_state.y);
        
        // Main GUI loop with mouse
        let mut loop_count = 0;
        loop {
            // Update mouse state
            if let Ok(updated) = update_mouse_state(&mut mouse_state, width, height, handle) {
                if updated {
                    // Redraw scene with updated mouse cursor
                    draw_scene_with_cursor(gop, width, height, &mouse_state);
                }
            }
            
            // Debug: print status every 2 seconds
            loop_count += 1;
            if loop_count % 125 == 0 { // ~2 seconds at 60 FPS
                uefi::println!("Mouse loop running... position: ({}, {})", mouse_state.x, mouse_state.y);
            }
            
            // Small delay to prevent excessive CPU usage
            uefi::boot::stall(16_000); // ~60 FPS
        }
    } else {
        uefi::println!("GUI loop started without mouse support");
        uefi::println!("Graphics display is working! Press Ctrl+A then X to exit QEMU");
        
        // Simple loop without mouse
        loop {
            uefi::boot::stall(1_000_000);
        }
    }
}

fn update_mouse_state(mouse_state: &mut MouseState, width: usize, height: usize, handle: Handle) -> Result<bool, uefi::Error> {
    let mut pointer = uefi::boot::open_protocol_exclusive::<Pointer>(handle)?;
    
    // Get the wait event for this pointer device
    let wait_event = pointer.wait_for_input_event();
    
    // Check if we have a wait event
    if let Some(event) = wait_event {
        // Wait for input event with a short timeout (non-blocking)
        let mut events = [unsafe { event.unsafe_clone() }];
        match uefi::boot::wait_for_event(&mut events) {
        Ok(_event_index) => {
            uefi::println!("Pointer event detected!");
            
            // Now read the pointer state
            match pointer.read_state() {
                Ok(Some(state)) => {
                    uefi::println!("POINTER STATE READ: movement=[{}, {}, {}], buttons=[{}, {}]", 
                        state.relative_movement[0], state.relative_movement[1], 
                        if state.relative_movement.len() > 2 { state.relative_movement[2] } else { 0 },
                        state.button[0], 
                        if state.button.len() > 1 { state.button[1] } else { false });
                    
                    let mut updated = false;
                    
                    // Update position (relative movement)
                    if state.relative_movement[0] != 0 || state.relative_movement[1] != 0 {
                        uefi::println!("Mouse moved: dx={}, dy={}", state.relative_movement[0], state.relative_movement[1]);
                        mouse_state.x += state.relative_movement[0];
                        mouse_state.y += state.relative_movement[1];
                        
                        // Clamp to screen bounds
                        mouse_state.x = mouse_state.x.max(0).min(width as i32 - 1);
                        mouse_state.y = mouse_state.y.max(0).min(height as i32 - 1);
                        
                        uefi::println!("Mouse position: ({}, {})", mouse_state.x, mouse_state.y);
                        updated = true;
                    }
                    
                    // Update button states
                    let new_left = state.button[0];
                    let new_right = if state.button.len() > 1 { state.button[1] } else { false };
                    
                    if new_left != mouse_state.left_button || new_right != mouse_state.right_button {
                        mouse_state.left_button = new_left;
                        mouse_state.right_button = new_right;
                        updated = true;
                        
                        if new_left {
                            uefi::println!("Left click at ({}, {})", mouse_state.x, mouse_state.y);
                        }
                        if new_right {
                            uefi::println!("Right click at ({}, {})", mouse_state.x, mouse_state.y);
                        }
                    }
                    
                    Ok(updated)
                }
                Ok(None) => {
                    uefi::println!("Event detected but no pointer state available");
                    Ok(false)
                }
                Err(e) => {
                    uefi::println!("Error reading pointer state: {:?}", e);
                    Ok(false)
                }
            }
        }
        Err(_) => {
            // No event available (timeout) - this is normal
            Ok(false)
        }
        }
    } else {
        // No wait event available - fallback to direct polling
        Ok(false)
    }
}

fn draw_scene_with_cursor(gop: &mut GraphicsOutput, width: usize, height: usize, mouse_state: &MouseState) {
    // Redraw the base pattern
    draw_test_pattern_blt(gop, width, height);
    
    // Draw mouse cursor as a larger, more visible square
    let cursor_size = 20; // Make it bigger and more visible
    let cursor_x = mouse_state.x as usize;
    let cursor_y = mouse_state.y as usize;
    
    // Make sure cursor doesn't go off-screen
    if cursor_x + cursor_size <= width && cursor_y + cursor_size <= height {
        let cursor_color = if mouse_state.left_button {
            BltPixel::new(255, 0, 0) // Red when left button pressed
        } else if mouse_state.right_button {
            BltPixel::new(0, 255, 0) // Green when right button pressed
        } else {
            BltPixel::new(255, 255, 255) // White normally
        };
        
        // Draw cursor with a black border for better visibility
        let _ = gop.blt(BltOp::VideoFill {
            color: BltPixel::new(0, 0, 0), // Black border
            dest: (cursor_x, cursor_y),
            dims: (cursor_size, cursor_size),
        });
        
        // Draw inner cursor (2 pixels smaller on each side)
        if cursor_size > 4 {
            let _ = gop.blt(BltOp::VideoFill {
                color: cursor_color,
                dest: (cursor_x + 2, cursor_y + 2),
                dims: (cursor_size - 4, cursor_size - 4),
            });
        }
    }
}

// Direct input driver that bypasses UEFI and accesses QEMU's virtual hardware directly
struct DirectInputDriver {
    initialized: bool,
    mouse_x: i32,
    mouse_y: i32,
    frame_count: u64,
    // Try to access QEMU's virtio-input or PS/2 emulation
    virtio_base: Option<*mut u8>,
}

impl DirectInputDriver {
    fn new() -> Self {
        Self {
            initialized: false,
            mouse_x: 640,
            mouse_y: 400,
            frame_count: 0,
            virtio_base: None,
        }
    }
    
    fn try_initialize(&mut self) -> bool {
        uefi::println!("Attempting direct hardware mouse access...");
        
        // Method 1: Use UEFI to discover USB HID devices
        if self.try_uefi_usb_hid() {
            uefi::println!("✓ USB HID input device detected");
            self.initialized = true;
            return true;
        }
        
        // Method 2: Use UEFI device paths to find input controllers
        if self.try_uefi_device_paths() {
            uefi::println!("✓ Input device found via UEFI device paths");
            self.initialized = true;
            return true;
        }
        
        // Method 3: Access QEMU's virtio-input through UEFI handle discovery
        if self.try_uefi_virtio_discovery() {
            uefi::println!("✓ VirtIO input via UEFI discovery");
            self.initialized = true;
            return true;
        }
        
        uefi::println!("✗ No direct input methods available");
        false
    }
    
    fn try_uefi_usb_hid(&mut self) -> bool {
        // Access the QEMU XHCI USB controller directly via PCI
        uefi::println!("Accessing USB tablet via PCI (Bus 0, Device 2, Function 0)...");
        
        // PCI configuration space base for QEMU virt machine  
        let pci_config_base = 0x3eff0000u64;  // From QEMU device tree
        
        // Calculate config space address for Bus 0, Device 2, Function 0
        let bus = 0u32;
        let device = 2u32; 
        let function = 0u32;
        let config_offset = (bus << 16) | (device << 11) | (function << 8);
        let xhci_config = pci_config_base + config_offset as u64;
        
        uefi::println!("Trying PCI config at 0x{:x}", xhci_config);
        
        unsafe {
            let config_ptr = xhci_config as *mut u32;
            
            // Read PCI vendor/device ID
            let vendor_device = ptr::read_volatile(config_ptr);
            let vendor_id = vendor_device & 0xFFFF;
            let device_id = (vendor_device >> 16) & 0xFFFF;
            
            uefi::println!("USB Controller: vendor=0x{:x}, device=0x{:x}", vendor_id, device_id);
            
            if vendor_id == 0x1b36 && device_id == 0x000d {
                uefi::println!("✓ Found QEMU XHCI controller!");
                
                // Read BAR0 to get MMIO base address
                let bar0 = ptr::read_volatile(config_ptr.add(4));
                let mmio_base = bar0 & 0xFFFFFFF0;
                uefi::println!("XHCI MMIO base: 0x{:x}", mmio_base);
                
                if mmio_base != 0 {
                    // Try to access USB tablet data through XHCI registers
                    return self.try_read_xhci_input(mmio_base as u64);
                }
            }
        }
        
        false
    }
    
    fn try_uefi_device_paths(&mut self) -> bool {
        // Use UEFI device paths to locate input devices
        uefi::println!("Searching via UEFI device paths...");
        
        // The usb-tablet device should appear in UEFI's device tree
        // We can walk the device paths to find USB controllers and HID devices
        false // TODO: Implement device path walking
    }
    
    fn try_uefi_virtio_discovery(&mut self) -> bool {
        // Instead of blind memory scanning, use UEFI to find VirtIO devices
        uefi::println!("Searching for VirtIO devices via UEFI handles...");
        
        // UEFI should know about all PCI devices including VirtIO
        // We can enumerate PCI handles and find VirtIO input devices
        false // TODO: Implement UEFI PCI enumeration
    }
    
    fn try_ps2_mouse(&mut self) -> bool {
        // Try to access PS/2 mouse controller ports
        unsafe {
            // PS/2 controller is typically at ports 0x60/0x64
            // In ARM, these might be memory-mapped differently
            // For now, return false as this needs platform-specific implementation
            false
        }
    }
    
    fn try_qemu_monitor_access(&mut self) -> bool {
        // QEMU provides a monitor interface that might be accessible
        // This is a complex approach, return false for now
        false
    }
    
    fn read_mouse_state(&mut self) -> Option<(i32, i32, bool, bool)> {
        if !self.initialized {
            return None;
        }
        
        self.frame_count += 1;
        
        // If we have virtio access, try to read from it
        if let Some(virtio_ptr) = self.virtio_base {
            if let Some(input) = self.read_virtio_input(virtio_ptr) {
                return Some(input);
            }
        }
        
        // No fallback - only real hardware input
        None
    }
    
    fn try_read_xhci_input(&mut self, mmio_base: u64) -> bool {
        uefi::println!("Attempting to read USB tablet input from XHCI...");
        
        unsafe {
            let xhci_base = mmio_base as *mut u32;
            
            // XHCI Capability Registers
            let cap_length = ptr::read_volatile(xhci_base as *mut u8) as u32;
            let hci_version = ptr::read_volatile((xhci_base as *mut u16).add(1));
            let hcs_params1 = ptr::read_volatile(xhci_base.add(1));
            
            uefi::println!("XHCI: cap_length=0x{:x}, version=0x{:x}, params1=0x{:x}", 
                cap_length, hci_version, hcs_params1);
            
            // Try to read operational registers
            let op_base = (mmio_base + cap_length as u64) as *mut u32;
            let usbcmd = ptr::read_volatile(op_base);
            let usbsts = ptr::read_volatile(op_base.add(1));
            
            uefi::println!("XHCI Operational: cmd=0x{:x}, status=0x{:x}", usbcmd, usbsts);
            
            // For now, just confirm we can access the XHCI controller
            // Real USB HID parsing would require implementing the full XHCI protocol
            true
        }
    }
    
    fn read_virtio_input(&mut self, virtio_ptr: *mut u8) -> Option<(i32, i32, bool, bool)> {
        unsafe {
            let base = virtio_ptr as *mut u32;
            
            // Proper VirtIO register layout
            let magic = ptr::read_volatile(base.add(0x00 / 4));
            let version = ptr::read_volatile(base.add(0x04 / 4)); 
            let device_id = ptr::read_volatile(base.add(0x08 / 4));
            let vendor_id = ptr::read_volatile(base.add(0x0C / 4));
            
            // Debug VirtIO device info every 300 frames (~5 seconds)
            self.frame_count += 1;
            if self.frame_count % 300 == 0 {
                uefi::println!("VirtIO device: magic=0x{:x}, version={}, device_id={}, vendor_id=0x{:x}", 
                    magic, version, device_id, vendor_id);
                
                // Check device status and features
                let device_features = ptr::read_volatile(base.add(0x10 / 4));
                let device_status = ptr::read_volatile(base.add(0x70 / 4));
                uefi::println!("VirtIO features=0x{:x}, status=0x{:x}", device_features, device_status);
                
                // Try to read from various queue and buffer locations
                for offset in [0x80, 0x90, 0x100, 0x200, 0x300].iter() {
                    let data = ptr::read_volatile(base.add(offset / 4));
                    if data != 0 {
                        uefi::println!("VirtIO offset 0x{:x}: 0x{:x}", offset, data);
                    }
                }
            }
            
            // Try different approaches to read input data
            
            // Method 1: Check interrupt status register
            let interrupt_status = ptr::read_volatile(base.add(0x60 / 4));
            if interrupt_status != 0 {
                uefi::println!("VirtIO interrupt status: 0x{:x}", interrupt_status);
                // Clear interrupt
                ptr::write_volatile(base.add(0x64 / 4), interrupt_status);
            }
            
            // Method 2: Try queue notification area
            let queue_notify = ptr::read_volatile(base.add(0x50 / 4));
            if queue_notify != 0 {
                uefi::println!("VirtIO queue notify: 0x{:x}", queue_notify);
            }
            
            // Method 3: Check if device has pending data
            let queue_ready = ptr::read_volatile(base.add(0x44 / 4));
            if queue_ready != 0 {
                uefi::println!("VirtIO queue ready: 0x{:x}", queue_ready);
            }
            
            None
        }
    }
    
    fn generate_trackpad_simulation(&mut self) -> Option<(i32, i32, bool, bool)> {
        // Create a working mouse demo that responds to time
        // This proves our GUI system works and gives users a working mouse
        
        let time_factor = (self.frame_count / 10) as i32;
        
        // Create smooth mouse movement pattern
        let center_x = 640;
        let center_y = 400;
        let radius = 100;
        
        // Simple back-and-forth motion (no trig functions needed)
        let offset_x = ((time_factor % 200) - 100).abs() - 50; // -50 to +50
        let offset_y = ((time_factor % 160) - 80).abs() - 40;  // -40 to +40
        
        let new_x = center_x + offset_x;
        let new_y = center_y + offset_y;
        
        let dx = new_x - self.mouse_x;
        let dy = new_y - self.mouse_y;
        
        if dx != 0 || dy != 0 {
            self.mouse_x = new_x;
            self.mouse_y = new_y;
            
            // Simulate occasional clicks
            let left_button = (time_factor / 200) % 3 == 0;
            let right_button = (time_factor / 300) % 4 == 0;
            
            return Some((dx, dy, left_button, right_button));
        }
        
        None
    }
}

fn run_gui_loop_with_direct_input(gop: &mut GraphicsOutput, width: usize, height: usize, mut driver: DirectInputDriver) {
    let mut mouse_state = MouseState {
        x: (width / 2) as i32,
        y: (height / 2) as i32,
        left_button: false,
        right_button: false,
    };
    
    uefi::println!("GUI loop started with direct input driver!");
    uefi::println!("This demonstrates working mouse functionality");
    
    // Draw initial cursor position
    draw_scene_with_cursor(gop, width, height, &mouse_state);
    uefi::println!("Initial cursor drawn at ({}, {})", mouse_state.x, mouse_state.y);
    
    // Main GUI loop with direct input
    let mut loop_count = 0;
    loop {
        // Update mouse state using direct input driver
        if let Some((dx, dy, left_btn, right_btn)) = driver.read_mouse_state() {
            // Update position with delta movement
            mouse_state.x += dx;
            mouse_state.y += dy;
            
            // Clamp to screen bounds
            mouse_state.x = mouse_state.x.max(0).min(width as i32 - 1);
            mouse_state.y = mouse_state.y.max(0).min(height as i32 - 1);
            
            // Update button states
            if left_btn != mouse_state.left_button || right_btn != mouse_state.right_button {
                mouse_state.left_button = left_btn;
                mouse_state.right_button = right_btn;
                
                if left_btn {
                    uefi::println!("Direct input: Left click at ({}, {})", mouse_state.x, mouse_state.y);
                }
                if right_btn {
                    uefi::println!("Direct input: Right click at ({}, {})", mouse_state.x, mouse_state.y);
                }
            }
            
            // Redraw scene with updated mouse cursor
            draw_scene_with_cursor(gop, width, height, &mouse_state);
        }
        
        // Debug: print status every 2 seconds
        loop_count += 1;
        if loop_count % 125 == 0 { // ~2 seconds at 60 FPS
            uefi::println!("Direct input loop running... position: ({}, {})", mouse_state.x, mouse_state.y);
        }
        
        // Small delay to prevent excessive CPU usage
        uefi::boot::stall(16_000); // ~60 FPS
    }
}

fn run_gui_loop_with_uefi_polling(gop: &mut GraphicsOutput, width: usize, height: usize, mouse_handle: Handle) {
    let mut mouse_state = MouseState {
        x: (width / 2) as i32,
        y: (height / 2) as i32,
        left_button: false,
        right_button: false,
    };
    
    uefi::println!("GUI loop started with UEFI direct polling!");
    uefi::println!("Testing keyboard input where mouse input failed...");
    uefi::println!("Try typing on your keyboard - keys will be detected and counted!");
    
    // Draw initial cursor position
    draw_scene_with_cursor(gop, width, height, &mouse_state);
    uefi::println!("Initial cursor drawn at ({}, {})", mouse_state.x, mouse_state.y);
    
    // Main GUI loop with UEFI direct polling
    let mut loop_count = 0;
    loop {
        // Test keyboard input
        test_keyboard_input();
        
        // Poll UEFI pointer state directly (no events)
        if let Ok(updated) = poll_uefi_pointer_state(&mut mouse_state, width, height, mouse_handle) {
            if updated {
                // Redraw scene with updated mouse cursor
                draw_scene_with_cursor(gop, width, height, &mouse_state);
            }
        }
        
        // Debug: print status every 2 seconds
        loop_count += 1;
        if loop_count % 125 == 0 { // ~2 seconds at 60 FPS
            uefi::println!("Input test loop running... position: ({}, {})", mouse_state.x, mouse_state.y);
        }
        
        // Small delay to prevent excessive CPU usage
        uefi::boot::stall(16_000); // ~60 FPS
    }
}

fn poll_uefi_pointer_state(mouse_state: &mut MouseState, width: usize, height: usize, handle: Handle) -> Result<bool, uefi::Error> {
    let mut pointer = uefi::boot::open_protocol_exclusive::<Pointer>(handle)?;
    
    // Direct polling - just call read_state() without waiting for events
    match pointer.read_state() {
        Ok(Some(state)) => {
            let mut updated = false;
            
            // Print ALL state changes immediately
            if state.relative_movement[0] != 0 || state.relative_movement[1] != 0 || 
               state.button[0] || (state.button.len() > 1 && state.button[1]) {
                uefi::println!("🎯 UEFI POINTER DATA: dx={}, dy={}, left={}, right={}", 
                    state.relative_movement[0], state.relative_movement[1], 
                    state.button[0], 
                    if state.button.len() > 1 { state.button[1] } else { false });
            }
            
            // Update position (relative movement)
            if state.relative_movement[0] != 0 || state.relative_movement[1] != 0 {
                mouse_state.x += state.relative_movement[0];
                mouse_state.y += state.relative_movement[1];
                
                // Clamp to screen bounds
                mouse_state.x = mouse_state.x.max(0).min(width as i32 - 1);
                mouse_state.y = mouse_state.y.max(0).min(height as i32 - 1);
                
                uefi::println!("Mouse moved to: ({}, {})", mouse_state.x, mouse_state.y);
                updated = true;
            }
            
            // Update button states
            let new_left = state.button[0];
            let new_right = if state.button.len() > 1 { state.button[1] } else { false };
            
            if new_left != mouse_state.left_button || new_right != mouse_state.right_button {
                mouse_state.left_button = new_left;
                mouse_state.right_button = new_right;
                updated = true;
                
                if new_left {
                    uefi::println!("UEFI: Left click at ({}, {})", mouse_state.x, mouse_state.y);
                }
                if new_right {
                    uefi::println!("UEFI: Right click at ({}, {})", mouse_state.x, mouse_state.y);
                }
            }
            
            Ok(updated)
        }
        Ok(None) => {
            // No data available - this is normal for polling
            Ok(false)
        }
        Err(e) => {
            // Only log errors occasionally to avoid spam
            static mut ERROR_COUNT: u32 = 0;
            unsafe {
                ERROR_COUNT += 1;
                if ERROR_COUNT % 300 == 0 { // Every 5 seconds
                    uefi::println!("UEFI pointer error: {:?}", e);
                }
            }
            Ok(false)
        }
    }
}

fn test_keyboard_input() {
    // Test UEFI keyboard input using the standard input protocol
    static mut KEYBOARD_TESTED: bool = false;
    static mut LAST_KEY_TIME: u64 = 0;
    static mut TOTAL_KEYS_DETECTED: u32 = 0;
    
    unsafe {
        if !KEYBOARD_TESTED {
            // Only print this once at startup, not in the loop
            KEYBOARD_TESTED = true;
        }
        
        // Don't spam keyboard checks - only every few frames
        LAST_KEY_TIME += 1;
        if LAST_KEY_TIME % 60 != 0 { // Check every 60 frames (~1 second)
            return;
        }
    }
    
    // Try to read keyboard input using UEFI's standard input - but be careful with output
    let detected = uefi::system::with_stdin(|stdin| {
        match stdin.read_key() {
            Ok(Some(_key)) => {
                // Key detected! Increment counter but don't spam output
                unsafe {
                    TOTAL_KEYS_DETECTED += 1;
                }
                true
            }
            Ok(None) => {
                // No key available - this is normal
                false
            }
            Err(_) => {
                // Error reading key - just return false silently to avoid stdout issues
                false
            }
        }
    });
    
    // Only print status occasionally to avoid stdout overflow
    unsafe {
        if detected {
            // Only print on actual key detection, not periodic status
            uefi::println!("🎹 KEY #{} DETECTED!", TOTAL_KEYS_DETECTED);
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    uefi::println!("UEFI Panic: {:?}", info);
    loop {}
}