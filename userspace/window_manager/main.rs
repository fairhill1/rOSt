#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

// Bump allocator for userspace
const HEAP_SIZE: usize = 128 * 1024; // 128KB heap

struct BumpAllocator {
    heap: UnsafeCell<[u8; HEAP_SIZE]>,
    next: UnsafeCell<usize>,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap: UnsafeCell::new([0; HEAP_SIZE]),
            next: UnsafeCell::new(0),
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        let next = *self.next.get();
        let aligned = (next + align - 1) & !(align - 1);
        let new_next = aligned + size;

        if new_next > HEAP_SIZE {
            return core::ptr::null_mut();
        }

        *self.next.get() = new_next;
        self.heap.get().cast::<u8>().add(aligned)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

// Window manager constants
const MAX_WINDOWS: usize = 16;
const TITLE_BAR_HEIGHT: u32 = 30;
const BORDER_WIDTH: u32 = 2;

// Colors
const TITLE_BAR_COLOR: u32 = 0xFF_2D_2D_30;
const TITLE_BAR_FOCUSED_COLOR: u32 = 0xFF_00_7A_CC;
const BORDER_COLOR: u32 = 0xFF_44_44_44;
const TEXT_COLOR: u32 = 0xFF_CC_CC_CC;

/// Window state tracked by WM
#[derive(Clone, Copy)]
struct WindowState {
    id: usize,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    title: [u8; 64],
    title_len: usize,
    focused: bool,
    visible: bool,
}

impl WindowState {
    const fn new() -> Self {
        Self {
            id: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            title: [0; 64],
            title_len: 0,
            focused: false,
            visible: false,
        }
    }
}

/// Global window manager state
static mut WINDOWS: [WindowState; MAX_WINDOWS] = [WindowState::new(); MAX_WINDOWS];
static WINDOW_COUNT: AtomicUsize = AtomicUsize::new(0);
static MOUSE_X: AtomicUsize = AtomicUsize::new(0);
static MOUSE_Y: AtomicUsize = AtomicUsize::new(0);
static mut FB_PTR: *mut u32 = core::ptr::null_mut();
static mut FB_WIDTH: u32 = 0;
static mut FB_HEIGHT: u32 = 0;

/// Find window at given coordinates
fn find_window_at(x: i32, y: i32) -> Option<usize> {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    // Check windows in reverse order (top to bottom in Z order)
    for i in (0..count).rev() {
        let window = unsafe { &WINDOWS[i] };
        if !window.visible {
            continue;
        }

        if x >= window.x && x < (window.x + window.width as i32) &&
           y >= window.y && y < (window.y + window.height as i32) {
            return Some(window.id);
        }
    }

    None
}

/// Check if click is in title bar
fn is_in_title_bar(window: &WindowState, x: i32, y: i32) -> bool {
    x >= window.x && x < (window.x + window.width as i32) &&
    y >= window.y && y < (window.y + TITLE_BAR_HEIGHT as i32)
}

/// Handle input event and determine routing
fn handle_input(event: InputEvent, mouse_x: i32, mouse_y: i32) -> WMToKernel {
    // Update mouse position
    if event.event_type == 3 { // MouseMove
        MOUSE_X.store(mouse_x as usize, Ordering::SeqCst);
        MOUSE_Y.store(mouse_y as usize, Ordering::SeqCst);
    }

    // Handle mouse button clicks
    if event.event_type == 4 && event.pressed != 0 { // MouseButton pressed
        if let Some(window_id) = find_window_at(mouse_x, mouse_y) {
            // Find window index
            let count = WINDOW_COUNT.load(Ordering::SeqCst);
            for i in 0..count {
                let window = unsafe { &mut WINDOWS[i] };
                if window.id == window_id {
                    // Check if click is in title bar (for focus/drag)
                    if is_in_title_bar(window, mouse_x, mouse_y) {
                        // Request focus change
                        return WMToKernel::RequestFocus { window_id };
                    }

                    // Route input to window
                    return WMToKernel::RouteInput {
                        window_id,
                        event,
                    };
                }
            }
        }
    }

    // For keyboard events, route to focused window
    if event.event_type == 1 || event.event_type == 2 { // KeyPressed or KeyReleased
        let count = WINDOW_COUNT.load(Ordering::SeqCst);
        for i in 0..count {
            let window = unsafe { &WINDOWS[i] };
            if window.focused {
                return WMToKernel::RouteInput {
                    window_id: window.id,
                    event,
                };
            }
        }
    }

    WMToKernel::NoAction
}

/// Draw a filled rectangle
fn draw_rect(x: i32, y: i32, width: u32, height: u32, color: u32) {
    unsafe {
        if FB_PTR.is_null() {
            return;
        }

        let fb_width = FB_WIDTH as i32;
        let fb_height = FB_HEIGHT as i32;

        // Clip to screen bounds
        if x >= fb_width || y >= fb_height || x + width as i32 <= 0 || y + height as i32 <= 0 {
            return;
        }

        let start_x = x.max(0) as usize;
        let start_y = y.max(0) as usize;
        let end_x = ((x + width as i32).min(fb_width)) as usize;
        let end_y = ((y + height as i32).min(fb_height)) as usize;

        for py in start_y..end_y {
            for px in start_x..end_x {
                let offset = py * fb_width as usize + px;
                core::ptr::write_volatile(FB_PTR.add(offset), color);
            }
        }
    }
}

/// Draw text (simplified - just use first 8 chars of title)
fn draw_text(x: i32, y: i32, text: &[u8], _color: u32) {
    // For Phase 1, we'll just show window IDs or first few chars
    // Proper text rendering will come later when we expose fontdue via syscalls
    let _ = (x, y, text);
    // TODO: Implement proper text rendering
}

/// Draw window chrome (title bar and borders)
fn draw_window_chrome(window: &WindowState) {
    if !window.visible {
        return;
    }

    // Title bar
    let title_color = if window.focused {
        TITLE_BAR_FOCUSED_COLOR
    } else {
        TITLE_BAR_COLOR
    };

    draw_rect(window.x, window.y, window.width, TITLE_BAR_HEIGHT, title_color);

    // Title text (simplified for now)
    let title_text = &window.title[..window.title_len.min(64)];
    draw_text(window.x + 5, window.y + 7, title_text, TEXT_COLOR);

    // Borders
    // Left
    draw_rect(window.x, window.y + TITLE_BAR_HEIGHT as i32, BORDER_WIDTH, window.height - TITLE_BAR_HEIGHT, BORDER_COLOR);
    // Right
    draw_rect(window.x + window.width as i32 - BORDER_WIDTH as i32, window.y + TITLE_BAR_HEIGHT as i32, BORDER_WIDTH, window.height - TITLE_BAR_HEIGHT, BORDER_COLOR);
    // Bottom
    draw_rect(window.x, window.y + window.height as i32 - BORDER_WIDTH as i32, window.width, BORDER_WIDTH, BORDER_COLOR);
}

/// Redraw all window chrome
fn redraw_chrome() {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);
    for i in 0..count {
        let window = unsafe { &WINDOWS[i] };
        draw_window_chrome(window);
    }
}

/// Add or update window
fn handle_create_window(id: usize, x: i32, y: i32, width: u32, height: u32, title: [u8; 64], title_len: usize) {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    // Check if window already exists
    for i in 0..count {
        let window = unsafe { &mut WINDOWS[i] };
        if window.id == id {
            // Update existing window
            window.x = x;
            window.y = y;
            window.width = width;
            window.height = height;
            window.title = title;
            window.title_len = title_len;
            window.visible = true;
            return;
        }
    }

    // Add new window
    if count < MAX_WINDOWS {
        unsafe {
            WINDOWS[count] = WindowState {
                id,
                x,
                y,
                width,
                height,
                title,
                title_len,
                focused: count == 0, // First window is focused
                visible: true,
            };
        }
        WINDOW_COUNT.store(count + 1, Ordering::SeqCst);
    }
}

/// Remove window
fn handle_close_window(id: usize) {
    let mut count = WINDOW_COUNT.load(Ordering::SeqCst);

    for i in 0..count {
        let window = unsafe { &WINDOWS[i] };
        if window.id == id {
            // Shift remaining windows down
            for j in i..count-1 {
                unsafe {
                    WINDOWS[j] = WINDOWS[j + 1];
                }
            }
            count -= 1;
            WINDOW_COUNT.store(count, Ordering::SeqCst);

            // If we removed the focused window, focus the last window
            if window.focused && count > 0 {
                unsafe {
                    WINDOWS[count - 1].focused = true;
                }
            }
            return;
        }
    }
}

/// Set window focus
fn handle_set_focus(id: usize) {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    for i in 0..count {
        let window = unsafe { &mut WINDOWS[i] };
        window.focused = window.id == id;
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Userspace Window Manager ===\r\n");
    print_debug("Running at EL0\r\n");

    // Get framebuffer info
    let fb_info = match fb_info() {
        Some(info) => info,
        None => {
            print_debug("Failed to get framebuffer info\r\n");
            exit(1);
        }
    };

    // Map framebuffer
    let fb_ptr = match fb_map() {
        Some(ptr) => ptr,
        None => {
            print_debug("Failed to map framebuffer\r\n");
            exit(1);
        }
    };

    // Initialize globals
    unsafe {
        FB_PTR = fb_ptr;
        FB_WIDTH = fb_info.width;
        FB_HEIGHT = fb_info.height;
    }

    print_debug("Framebuffer mapped\r\n");

    // PHASE 2: WM receives input from kernel via IPC
    print_debug("Window manager ready - listening for IPC messages\r\n");

    // Main event loop - receive input from kernel via IPC
    loop {
        // CRITICAL: Drain ALL pending messages before processing
        // This prevents queue overflow when kernel sends many mouse move events
        let mut messages_processed = 0;
        let mut need_redraw = false;

        loop {
            let mut buf = [0u8; 256];
            let result = recv_message(&mut buf, 0); // Non-blocking

            if result <= 0 {
                break; // No more messages
            }

            messages_processed += 1;

            // Parse message
            if let Some(msg) = KernelToWM::from_bytes(&buf) {
                match msg {
                    KernelToWM::InputEvent { sender_pid, mouse_x, mouse_y, event } => {
                        // Handle input and determine routing
                        let response = handle_input(event, mouse_x, mouse_y);

                        // Send response back to kernel using sender_pid from message
                        let response_buf = response.to_bytes();
                        let result = send_message(sender_pid, &response_buf);

                        // If queue is full, stop processing and yield to let kernel drain
                        if result < 0 {
                            break; // Exit inner loop, will yield at outer loop
                        }

                        need_redraw = true;
                    }
                    KernelToWM::CreateWindow { id, x, y, width, height, title, title_len } => {
                        handle_create_window(id, x, y, width, height, title, title_len);
                        need_redraw = true;
                    }
                    KernelToWM::CloseWindow { id } => {
                        handle_close_window(id);
                        need_redraw = true;
                    }
                    KernelToWM::SetFocus { id } => {
                        handle_set_focus(id);
                        need_redraw = true;
                    }
                }
            }

            // Limit batch processing to prevent starvation
            if messages_processed >= 100 {
                break;
            }
        }

        // Redraw once after processing all messages
        if need_redraw {
            redraw_chrome();
            fb_flush();
        }

        // CRITICAL: Yield to other threads since we use cooperative multitasking
        // Without this, WM monopolizes CPU and GUI thread never runs
        yield_now();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in window manager!\r\n");
    exit(1);
}
