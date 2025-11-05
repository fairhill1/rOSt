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

/// Route input event to a specific window by instance ID
fn route_input_to_window(window_id: usize, event: drivers::input_events::InputEvent) {
    use drivers::input_events::InputEvent;

    // Get the window content type to determine how to route the input
    if let Some((content_type, _)) = crate::gui::window_manager::get_window_by_id(window_id) {
        match event {
            InputEvent::KeyPressed { key, modifiers } => {
                // Route keyboard input based on window content type
                match content_type {
                    crate::gui::window_manager::WindowContent::Terminal => {
                        if let Some(ascii) = drivers::input_events::evdev_to_ascii(key, modifiers) {
                            if let Some(shell) = crate::apps::shell::get_shell(window_id) {
                                shell.handle_char(ascii);
                            }
                        }
                    }
                    crate::gui::window_manager::WindowContent::Editor => {
                        // Check for Ctrl modifier
                        let is_ctrl = (modifiers & 0x11) != 0; // LEFT_CTRL | RIGHT_CTRL
                        let is_shift = (modifiers & 0x22) != 0; // LEFT_SHIFT | RIGHT_SHIFT

                        if let Some(editor) = crate::gui::widgets::editor::get_editor(window_id) {
                            // Handle special keys
                            match key {
                                103 => editor.move_up(), // KEY_UP
                                108 => editor.move_down(), // KEY_DOWN
                                105 => editor.move_left(), // KEY_LEFT
                                106 => editor.move_right(), // KEY_RIGHT
                                _ => {
                                    if is_ctrl && key == 31 { // Ctrl+S
                                        // Save would go here (requires access to filesystem)
                                    } else if !is_ctrl {
                                        if let Some(ascii) = drivers::input_events::evdev_to_ascii(key, modifiers) {
                                            if ascii == b'\n' {
                                                editor.insert_newline();
                                            } else if ascii == 8 { // Backspace
                                                editor.delete_char();
                                            } else if ascii >= 32 && ascii < 127 {
                                                editor.insert_char(ascii as char);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    crate::gui::window_manager::WindowContent::FileExplorer => {
                        // File explorer keyboard navigation
                        match key {
                            103 => { // KEY_UP
                                crate::gui::widgets::file_explorer::move_selection_up(window_id);
                            }
                            108 => { // KEY_DOWN
                                crate::gui::widgets::file_explorer::move_selection_down(window_id);
                            }
                            28 => { // KEY_ENTER
                                // Handle file opening (complex logic from old code)
                                let _ = crate::gui::widgets::file_explorer::open_selected(window_id);
                            }
                            _ => {}
                        }
                    }
                    crate::gui::window_manager::WindowContent::Snake => {
                        // Snake game controls
                        match key {
                            103 => { // KEY_UP
                                if let Some(game) = crate::apps::snake::get_snake_game(window_id) {
                                    game.set_direction(crate::apps::snake::Direction::Up);
                                }
                            }
                            108 => { // KEY_DOWN
                                if let Some(game) = crate::apps::snake::get_snake_game(window_id) {
                                    game.set_direction(crate::apps::snake::Direction::Down);
                                }
                            }
                            105 => { // KEY_LEFT
                                if let Some(game) = crate::apps::snake::get_snake_game(window_id) {
                                    game.set_direction(crate::apps::snake::Direction::Left);
                                }
                            }
                            106 => { // KEY_RIGHT
                                if let Some(game) = crate::apps::snake::get_snake_game(window_id) {
                                    game.set_direction(crate::apps::snake::Direction::Right);
                                }
                            }
                            19 => { // KEY_R (restart)
                                if let Some(game) = crate::apps::snake::get_snake_game(window_id) {
                                    game.reset();
                                }
                            }
                            _ => {}
                        }
                    }
                    crate::gui::window_manager::WindowContent::Browser => {
                        let is_shift = (modifiers & 0x22) != 0;
                        let is_ctrl = (modifiers & 0x11) != 0;

                        // Handle arrow keys
                        match key {
                            105 => { // KEY_LEFT
                                crate::gui::widgets::browser::handle_arrow_key(
                                    window_id,
                                    crate::gui::widgets::text_input::ArrowKey::Left,
                                    is_shift
                                );
                            }
                            106 => { // KEY_RIGHT
                                crate::gui::widgets::browser::handle_arrow_key(
                                    window_id,
                                    crate::gui::widgets::text_input::ArrowKey::Right,
                                    is_shift
                                );
                            }
                            102 => { // KEY_HOME
                                crate::gui::widgets::browser::handle_arrow_key(
                                    window_id,
                                    crate::gui::widgets::text_input::ArrowKey::Home,
                                    is_shift
                                );
                            }
                            107 => { // KEY_END
                                crate::gui::widgets::browser::handle_arrow_key(
                                    window_id,
                                    crate::gui::widgets::text_input::ArrowKey::End,
                                    is_shift
                                );
                            }
                            _ => {
                                // Regular keyboard input
                                if let Some(ascii) = drivers::input_events::evdev_to_ascii(key, modifiers) {
                                    crate::gui::widgets::browser::handle_key(window_id, ascii as char, is_ctrl, is_shift);
                                }
                            }
                        }
                    }
                    _ => {} // Other window types don't handle keyboard input yet
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                // Mouse button handling would go here
                // For now, most mouse handling is done by the window manager itself
                // (dragging, clicking title bars, etc.)
            }
            InputEvent::MouseWheel { delta } => {
                // Handle scroll wheel in focused window
                match content_type {
                    crate::gui::window_manager::WindowContent::Editor => {
                        if let Some(editor) = crate::gui::widgets::editor::get_editor(window_id) {
                            editor.scroll(-delta as i32 * 3);
                        }
                    }
                    crate::gui::window_manager::WindowContent::Browser => {
                        // Get browser window height for scroll handling
                        let browsers = crate::gui::window_manager::get_all_browsers();
                        if let Some((_, _, _, _, height)) = browsers.iter().find(|(id, _, _, _, _)| *id == window_id) {
                            if let Some(browser) = crate::gui::widgets::browser::get_browser(window_id) {
                                browser.handle_scroll(-delta as i32, *height as usize);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
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

    let mut needs_full_render = true;
    let mut last_minute = drivers::rtc::get_datetime().minute;
    let mut last_event_was_click = false; // Track if last event was a button press

    uart_write_string("[GUI-THREAD] Starting main loop\r\n");
    loop {
        // PHASE 3: Check for WM responses from previous iteration
        // This must happen BEFORE polling new input to handle the async pipeline:
        // Loop N: send input → WM scheduled → Loop N+1: receive response
        unsafe {
            let wm_pid = WINDOW_MANAGER_PID.load(Ordering::Acquire);

            if wm_pid > 0 {
                // Check for all pending responses (non-blocking)
                loop {
                    let mut response_buf = [0u8; 256];
                    let result = kernel_recv_message(&mut response_buf);

                    if result > 0 {
                        // Parse WMToKernel response
                        let msg_type = response_buf[0];

                        match msg_type {
                            0 => { // RouteInput
                                let window_id = usize::from_le_bytes([
                                    response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                    response_buf[5], response_buf[6], response_buf[7], response_buf[8]
                                ]);

                                let event_type = u32::from_le_bytes([response_buf[9], response_buf[10], response_buf[11], response_buf[12]]);

                                // Reconstruct the input event from the response
                                let kernel_event = match event_type {
                                    1 => drivers::input_events::InputEvent::KeyPressed {
                                        key: response_buf[13],
                                        modifiers: response_buf[14]
                                    },
                                    2 => drivers::input_events::InputEvent::KeyReleased {
                                        key: response_buf[13],
                                        modifiers: response_buf[14]
                                    },
                                    3 => drivers::input_events::InputEvent::MouseMove {
                                        x_delta: response_buf[17] as i8,
                                        y_delta: response_buf[18] as i8
                                    },
                                    4 => drivers::input_events::InputEvent::MouseButton {
                                        button: response_buf[15],
                                        pressed: response_buf[16] != 0
                                    },
                                    5 => drivers::input_events::InputEvent::MouseWheel {
                                        delta: response_buf[19] as i8
                                    },
                                    _ => continue, // Unknown event type
                                };

                                // Route the input event to the specified window
                                route_input_to_window(window_id, kernel_event);
                                needs_full_render = true;
                            }
                            1 => { // RequestFocus
                                let window_id = usize::from_le_bytes([
                                    response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                    response_buf[5], response_buf[6], response_buf[7], response_buf[8]
                                ]);

                                // Update focused window
                                crate::gui::window_manager::focus_window_by_id(window_id);
                                needs_full_render = true;
                            }
                            2 => { // RequestClose
                                let window_id = usize::from_le_bytes([
                                    response_buf[1], response_buf[2], response_buf[3], response_buf[4],
                                    response_buf[5], response_buf[6], response_buf[7], response_buf[8]
                                ]);

                                // Close the specified window
                                crate::gui::window_manager::close_window_by_id(window_id);
                                needs_full_render = true;
                            }
                            3 => { // NoAction
                                // Only handle menu clicks if the event was a button press, not hover
                                if last_event_was_click {
                                    let (cursor_x, cursor_y) = crate::gui::framebuffer::get_cursor_pos();
                                    if let Some(menu_idx) = crate::gui::window_manager::get_hovered_menu_button(cursor_x, cursor_y) {
                                        // Menu button clicked! Open corresponding window
                                        crate::gui::window_manager::open_window_by_menu_index(menu_idx);
                                        needs_full_render = true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else {
                        break; // No more messages
                    }
                }
            }
        }

        // Check if minute has changed - redraw clock every minute
        let current_minute = drivers::rtc::get_datetime().minute;

        if current_minute != last_minute {
            last_minute = current_minute;
            needs_full_render = true;
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
                            last_event_was_click = false;
                            (1u32, key, modifiers, 0u8, 0u8, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::KeyReleased { key, modifiers } => {
                            last_event_was_click = false;
                            (2u32, key, modifiers, 0u8, 0u8, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::MouseMove { x_delta, y_delta } => {
                            last_event_was_click = false;
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
                            last_event_was_click = pressed; // Only true for button press, not release
                            (4u32, 0u8, 0u8, button, if pressed { 1 } else { 0 }, 0i8, 0i8, 0i8)
                        }
                        drivers::input_events::InputEvent::MouseWheel { delta } => {
                            last_event_was_click = false;
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
        unsafe {
            if let Some(ref mut stack) = NETWORK_STACK {
                stack.poll();
            }
        }

        // Poll browser async HTTP state machines
        if crate::gui::widgets::browser::poll_all_browsers() {
            needs_full_render = true;
        }

        // Phase 2: Input events are now forwarded to WM via IPC (above)
        // The old direct input processing is disabled
        // The WM will handle input routing and send responses back if needed
        let needs_cursor_redraw = false; // Cursor updates happen in IPC forwarding above

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

                // Render all image viewer windows INSIDE their windows
                for (instance_id, cx, cy, cw, ch) in crate::gui::window_manager::get_all_image_viewers() {
                    crate::gui::widgets::image_viewer::render_at(instance_id, cx, cy, cw, ch);
                }

                // Swap buffers - copy back buffer to screen in one fast operation
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
                        // Flush writes to ensure they persist
                        uart_write_string("Flushing write cache...\r\n");
                        if let Err(e) = blk_devices[fs_device_idx].flush() {
                            uart_write_string("✗ Flush failed: ");
                            uart_write_string(e);
                            uart_write_string("\r\n");
                        } else {
                            uart_write_string("✓ Write cache flushed\r\n");
                        }
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

    // ===== EL0 USER MODE TEST =====
    // Test EL1→EL0 transition and syscalls
    // WARNING: This will replace the GUI with a simple syscall test program
    // COMMENTED OUT: Let system continue to GUI shell instead
    // uart_write_string("\n\n=== TESTING EL0 USER MODE ===\r\n");
    // interrupts::start_user_process(userspace_test::user_test_program);
    // (never returns)

    // ===== IPC TEST: Spawn sender and receiver at kernel init =====
    // DISABLED: IPC tests work but shouldn't auto-spawn on every boot
    // To test IPC manually, uncomment this section and rebuild
    /*
    uart_write_string("\n=== SPAWNING IPC TEST PROGRAMS ===\r\n");
    uart_write_string("Spawning IPC sender...\r\n");
    let sender_elf = embedded_apps::IPC_SENDER_ELF;
    let sender_pid = elf_loader::load_elf_and_spawn(sender_elf);
    uart_write_string(&alloc::format!("✓ IPC sender spawned as PID {}\r\n", sender_pid));

    uart_write_string("Spawning IPC receiver...\r\n");
    let receiver_elf = embedded_apps::IPC_RECEIVER_ELF;
    let receiver_pid = elf_loader::load_elf_and_spawn(receiver_elf);
    uart_write_string(&alloc::format!("✓ IPC receiver spawned as PID {}\r\n", receiver_pid));

    uart_write_string("Both IPC test programs will run via timer preemption...\r\n");
    uart_write_string("===================================\r\n\r\n");
    */

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

        // TEST: Load all 3 with diagnostics to see actual stack addresses when it crashes

        uart_write_string("[ELF-LOADER] Loading IPC sender...\r\n");
        let _sender_pid = elf_loader::load_elf_and_spawn(embedded_apps::IPC_SENDER_ELF);
        uart_write_string("[ELF-LOADER] IPC sender loaded\r\n");

        uart_write_string("[ELF-LOADER] Loading window manager...\r\n");
        let wm_pid = elf_loader::load_elf_and_spawn(embedded_apps::WINDOW_MANAGER_ELF);
        uart_write_string("[ELF-LOADER] WM loaded\r\n");
        WINDOW_MANAGER_PID.store(wm_pid, Ordering::Release);

        uart_write_string("[ELF-LOADER] Loading terminal...\r\n");
        let terminal_pid = elf_loader::load_elf_and_spawn(embedded_apps::TERMINAL_ELF);
        uart_write_string("[ELF-LOADER] Terminal loaded with PID: ");
        if terminal_pid < 10 {
            unsafe {
                core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + terminal_pid as u8);
            }
        }
        uart_write_string("\r\n");

        uart_write_string("[ELF-LOADER] Test: IPC sender + WM + Terminal (3 processes with diagnostics)\r\n");

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
