#![no_main]
#![no_std]

extern crate alloc;

use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, BltOp, BltPixel};
use uefi::proto::console::pointer::Pointer;
use uefi::Handle;
use uefi::mem::memory_map::MemoryType;
use linked_list_allocator::LockedHeap;

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
                    uefi::println!("Mouse input initialized successfully");
                    // Main event loop with mouse tracking
                    run_gui_loop(&mut gop, mode.resolution().0, mode.resolution().1, Some(mouse_handle));
                }
                Err(e) => {
                    uefi::println!("Failed to initialize mouse: {:?}", e);
                    uefi::println!("Running without mouse support...");
                    // Main event loop without mouse
                    run_gui_loop(&mut gop, mode.resolution().0, mode.resolution().1, None);
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
    
    // Return the first pointer device handle
    Ok(handles[0])
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
    
    // Read pointer state
    match pointer.read_state() {
        Ok(Some(state)) => {
            let mut updated = false;
            
            // Debug: always print state info every 60 frames (~1 second)
            static mut DEBUG_COUNTER: u32 = 0;
            unsafe {
                DEBUG_COUNTER += 1;
                if DEBUG_COUNTER % 60 == 0 {
                    uefi::println!("Mouse state: movement=[{}, {}, {}], buttons=[{}, {}]", 
                        state.relative_movement[0], state.relative_movement[1], 
                        if state.relative_movement.len() > 2 { state.relative_movement[2] } else { 0 },
                        state.button[0], 
                        if state.button.len() > 1 { state.button[1] } else { false });
                }
            }
            
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
            
            // Update button states (array of booleans)
            let new_left = state.button[0];   // Left button
            let new_right = if state.button.len() > 1 { state.button[1] } else { false }; // Right button
            
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
            // Debug: log when there's no data
            static mut NO_DATA_COUNTER: u32 = 0;
            unsafe {
                NO_DATA_COUNTER += 1;
                if NO_DATA_COUNTER % 300 == 0 { // Every 5 seconds
                    uefi::println!("No mouse data available");
                }
            }
            Ok(false)
        }
        Err(e) => {
            uefi::println!("Error reading mouse state: {:?}", e);
            Ok(false)
        }
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

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    uefi::println!("UEFI Panic: {:?}", info);
    loop {}
}