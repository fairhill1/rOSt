#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, AtomicU32, AtomicBool, Ordering};

// TEST: Re-enable one debug call to test ELF relocations
// If relocations work, string literals should no longer crash
// Keep this function here temporarily for testing, will remove later

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
const MAX_TILED_WINDOWS: usize = 4;  // Layout algorithm only supports 4 windows
const TITLE_BAR_HEIGHT: u32 = 30;
const BORDER_WIDTH: u32 = 2;
const CLOSE_BUTTON_SIZE: u32 = 18;

// Menu bar constants
const MENU_BAR_HEIGHT: u32 = 32;
const MENU_ITEM_HEIGHT: u32 = 24;
const MENU_ITEM_PADDING_X: u32 = 16;
const MENU_START_X: u32 = 8;
const MENU_START_Y: u32 = 4;

// Colors (original design restored)
const TITLE_BAR_COLOR: u32 = 0xFF666666; // Gray when not focused
const TITLE_BAR_FOCUSED_COLOR: u32 = 0xFF2D5C88; // Blue title bar
const BORDER_COLOR: u32 = 0xFF1A1A1A; // Dark border
const TEXT_COLOR: u32 = 0xFFFFFFFF; // White text
const MENU_BAR_COLOR: u32 = 0xFF2B2B2B; // Dark gray menu bar
const MENU_ITEM_COLOR: u32 = 0xFF3D3D3D; // Menu item background
const MENU_ITEM_HOVER_COLOR: u32 = 0xFF5D5D5D; // Menu item hover (brighter)

/// Window state tracked by WM
#[derive(Clone, Copy)]
struct WindowState {
    id: usize,          // Window ID (PID)
    shm_id: i32,        // Shared memory ID for framebuffer
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
            shm_id: 0,
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

/// Menu item definition
struct MenuItem {
    label: [u8; 16],  // Fixed-size inline array (no pointers)
}

static mut MENU_ITEMS: [MenuItem; 5] = [
    MenuItem { label: [0; 16] },
    MenuItem { label: [0; 16] },
    MenuItem { label: [0; 16] },
    MenuItem { label: [0; 16] },
    MenuItem { label: [0; 16] },
];

// Hardcoded label lengths - these never change!
const MENU_LABEL_LENS: [usize; 5] = [8, 6, 5, 7, 5]; // Terminal, Editor, Files, Browser, Snake

// App names built at runtime (avoids .rodata)
static mut APP_NAMES: [[u8; 16]; 5] = [[0; 16]; 5];
static mut APP_NAME_LENS: [usize; 5] = [0; 5];

/// Initialize menu items at runtime (avoids .rodata relocation issues)
fn init_menu_items() {
    unsafe {
        // Terminal
        MENU_ITEMS[0].label[0] = b'T';
        MENU_ITEMS[0].label[1] = b'e';
        MENU_ITEMS[0].label[2] = b'r';
        MENU_ITEMS[0].label[3] = b'm';
        MENU_ITEMS[0].label[4] = b'i';
        MENU_ITEMS[0].label[5] = b'n';
        MENU_ITEMS[0].label[6] = b'a';
        MENU_ITEMS[0].label[7] = b'l';

        // Editor
        MENU_ITEMS[1].label[0] = b'E';
        MENU_ITEMS[1].label[1] = b'd';
        MENU_ITEMS[1].label[2] = b'i';
        MENU_ITEMS[1].label[3] = b't';
        MENU_ITEMS[1].label[4] = b'o';
        MENU_ITEMS[1].label[5] = b'r';

        // Files
        MENU_ITEMS[2].label[0] = b'F';
        MENU_ITEMS[2].label[1] = b'i';
        MENU_ITEMS[2].label[2] = b'l';
        MENU_ITEMS[2].label[3] = b'e';
        MENU_ITEMS[2].label[4] = b's';

        // Browser
        MENU_ITEMS[3].label[0] = b'B';
        MENU_ITEMS[3].label[1] = b'r';
        MENU_ITEMS[3].label[2] = b'o';
        MENU_ITEMS[3].label[3] = b'w';
        MENU_ITEMS[3].label[4] = b's';
        MENU_ITEMS[3].label[5] = b'e';
        MENU_ITEMS[3].label[6] = b'r';

        // Snake
        MENU_ITEMS[4].label[0] = b'S';
        MENU_ITEMS[4].label[1] = b'n';
        MENU_ITEMS[4].label[2] = b'a';
        MENU_ITEMS[4].label[3] = b'k';
        MENU_ITEMS[4].label[4] = b'e';

        // Initialize app names
        // terminal
        APP_NAMES[0][0] = b't';
        APP_NAMES[0][1] = b'e';
        APP_NAMES[0][2] = b'r';
        APP_NAMES[0][3] = b'm';
        APP_NAMES[0][4] = b'i';
        APP_NAMES[0][5] = b'n';
        APP_NAMES[0][6] = b'a';
        APP_NAMES[0][7] = b'l';
        APP_NAME_LENS[0] = 8;

        // editor
        APP_NAMES[1][0] = b'e';
        APP_NAMES[1][1] = b'd';
        APP_NAMES[1][2] = b'i';
        APP_NAMES[1][3] = b't';
        APP_NAMES[1][4] = b'o';
        APP_NAMES[1][5] = b'r';
        APP_NAME_LENS[1] = 6;

        // files
        APP_NAMES[2][0] = b'f';
        APP_NAMES[2][1] = b'i';
        APP_NAMES[2][2] = b'l';
        APP_NAMES[2][3] = b'e';
        APP_NAMES[2][4] = b's';
        APP_NAME_LENS[2] = 5;

        // browser
        APP_NAMES[3][0] = b'b';
        APP_NAMES[3][1] = b'r';
        APP_NAMES[3][2] = b'o';
        APP_NAMES[3][3] = b'w';
        APP_NAMES[3][4] = b's';
        APP_NAMES[3][5] = b'e';
        APP_NAMES[3][6] = b'r';
        APP_NAME_LENS[3] = 7;

        // snake
        APP_NAMES[4][0] = b's';
        APP_NAMES[4][1] = b'n';
        APP_NAMES[4][2] = b'a';
        APP_NAMES[4][3] = b'k';
        APP_NAMES[4][4] = b'e';
        APP_NAME_LENS[4] = 5;
    }
}

/// Global window manager state
static mut WINDOWS: [WindowState; MAX_WINDOWS] = [WindowState::new(); MAX_WINDOWS];
static WINDOW_COUNT: AtomicUsize = AtomicUsize::new(0);
static MOUSE_X: AtomicUsize = AtomicUsize::new(0);
static MOUSE_Y: AtomicUsize = AtomicUsize::new(0);
static mut FB_PTR: *mut u32 = core::ptr::null_mut();
static FB_WIDTH: AtomicU32 = AtomicU32::new(0);
static FB_HEIGHT: AtomicU32 = AtomicU32::new(0);
static WM_INITIALIZED: AtomicBool = AtomicBool::new(false);

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

/// Check if click is on close button
fn is_in_close_button(window: &WindowState, x: i32, y: i32) -> bool {
    let btn_x = window.x + window.width as i32 - CLOSE_BUTTON_SIZE as i32 - 4;
    let btn_y = window.y + ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2) as i32;
    x >= btn_x && x < btn_x + CLOSE_BUTTON_SIZE as i32 &&
    y >= btn_y && y < btn_y + CLOSE_BUTTON_SIZE as i32
}

/// Check if click is in menu bar, return menu item index
fn check_menu_click(mouse_x: i32, mouse_y: i32) -> Option<usize> {
    // DEBUG: Check Y coordinate - MENU_ITEM_HEIGHT=24, MENU_START_Y=4
    // Valid Y range should be 4-28
    if mouse_y < 4 {
        print_debug("Y<4!");
        return None;
    }
    if mouse_y >= 28 {
        print_debug("Y>=28!");
        return None;
    }

    // DEBUG: Print first item's X range (Terminal button)
    let first_width = calculate_menu_item_width(8); // "Terminal" = 8 chars
    // Terminal should be at X=8 to X=8+first_width
    if mouse_x < 8 {
        print_debug("X<8");
    } else if mouse_x >= (8 + first_width as i32) {
        print_debug("X>terminal_end");
    } else {
        print_debug("X_IN_TERMINAL_RANGE!");
    }

    let mut current_x = MENU_START_X;
    for (idx, _item) in unsafe { MENU_ITEMS.iter() }.enumerate() {
        let label_len = MENU_LABEL_LENS[idx];
        let item_width = calculate_menu_item_width(label_len);
        let item_y = MENU_START_Y;

        if mouse_x >= current_x as i32 &&
           mouse_x < (current_x + item_width) as i32 &&
           mouse_y >= item_y as i32 &&
           mouse_y < (item_y + MENU_ITEM_HEIGHT) as i32 {
            print_debug("HIT!");
            return Some(idx);
        }

        current_x += item_width + 8; // 8px spacing
    }

    print_debug("LOOP_DONE_NO_HIT");
    None
}

/// Handle input event and determine routing
fn handle_input(event: InputEvent, mouse_x: i32, mouse_y: i32) -> WMToKernel {
    // ALWAYS update mouse position from kernel's current coordinates
    // (Kernel sends CURSOR_X/CURSOR_Y with every event, not just MouseMove)
    MOUSE_X.store(mouse_x as usize, Ordering::SeqCst);
    MOUSE_Y.store(mouse_y as usize, Ordering::SeqCst);

    // Handle mouse button clicks
    if event.event_type == 4 && event.pressed != 0 { // MouseButton pressed
        print_debug("CLICK!");
        let click_x = mouse_x;
        let click_y = mouse_y;

        // Check if click is on menu bar first
        let menu_result = check_menu_click(click_x, click_y);

        if let Some(menu_idx) = menu_result {
            print_debug("MENU_HIT!");
            // Menu item clicked! Spawn corresponding app
            let window_count = WINDOW_COUNT.load(Ordering::SeqCst);
            if window_count >= MAX_TILED_WINDOWS {
                print_debug("MAX_WIN");
                // Max windows reached, ignore click
                return WMToKernel::NoAction;
            }

            // Get app name from runtime-built array (avoids .rodata)
            if menu_idx >= 5 {
                print_debug("BAD_IDX");
                return WMToKernel::NoAction;
            }

            print_debug("SPAWN_ELF:");

            // Build app name string from runtime array
            let app_name_bytes = unsafe { &APP_NAMES[menu_idx][..APP_NAME_LENS[menu_idx]] };
            let app_name = core::str::from_utf8(app_name_bytes).unwrap_or("");

            let pid = spawn_elf(app_name);

            if pid > 0 {
                // Successfully spawned, app will create its own window via IPC
                print_debug("WM: Spawned app, PID = ");
                // TODO: Track PID → window mapping when app creates window
            }

            return WMToKernel::NoAction;
        }

        print_debug("CHECK_WINDOW...");
        if let Some(window_id) = find_window_at(click_x, click_y) {
            print_debug("WINDOW_HIT!");
            if window_id == 0 {
                print_debug("WID=0!");
            } else if window_id == 1 {
                print_debug("WID=1!");
            } else if window_id == 2 {
                print_debug("WID=2!");
            } else {
                print_debug("WID=OTHER!");
            }

            // Find window index
            let count = WINDOW_COUNT.load(Ordering::SeqCst);
            print_debug("LOOP_START:");
            for i in 0..count {
                let window = unsafe { &mut WINDOWS[i] };
                print_debug("CHK_");
                if window.id == window_id {
                    print_debug("MATCH!");

                    // Check if click is on close button first
                    if is_in_close_button(window, click_x, click_y) {
                        print_debug("CLOSE_BTN!");
                        // Request window close
                        return WMToKernel::RequestClose { window_id };
                    }

                    // If window is not focused, focus it first (consume the click)
                    if !window.focused {
                        print_debug("NOT_FOCUSED->FOCUS!");
                        return WMToKernel::RequestFocus { window_id };
                    }

                    // Window is already focused, route input to it
                    print_debug("ROUTE_INPUT!");
                    return WMToKernel::RouteInput {
                        window_id,
                        event,
                    };
                }
            }
        } else {
            print_debug("NO_WINDOW_FOUND!");
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

        let fb_width = FB_WIDTH.load(Ordering::SeqCst) as i32;
        let fb_height = FB_HEIGHT.load(Ordering::SeqCst) as i32;

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

/// Draw a single character using bitmap font from librost
/// Uses volatile writes to framebuffer (required in release mode)
fn draw_char(x: i32, y: i32, ch: u8, color: u32) {
    let fb_width = FB_WIDTH.load(Ordering::SeqCst) as i32;
    let fb_height = FB_HEIGHT.load(Ordering::SeqCst) as i32;

    // Get font data from librost shared library
    let char_data = if (ch as usize) < librost::graphics::FONT_8X8.len() {
        librost::graphics::FONT_8X8[ch as usize]
    } else {
        librost::graphics::FONT_8X8[0]
    };

    // Render with volatile writes (prevents compiler optimization)
    unsafe {
        if FB_PTR.is_null() {
            return;
        }

        // Scale 2x - each pixel in 8x8 font becomes 2x2 block
        for (row, &byte) in char_data.iter().enumerate() {
            for col in 0..8 {
                if (byte & (0x80 >> col)) != 0 {
                    // Draw 2x2 block for each font pixel
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = x + (col * 2 + dx) as i32;
                            let py = y + (row * 2 + dy) as i32;
                            if px >= 0 && px < fb_width && py >= 0 && py < fb_height {
                                let fb_w = fb_width as usize;
                                let fb_idx = (py as usize * fb_w) + px as usize;
                                // CRITICAL: Use volatile write to prevent optimization
                                FB_PTR.add(fb_idx).write_volatile(color);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Draw text string using bitmap font with volatile framebuffer writes
fn draw_text(x: i32, y: i32, text: &[u8], color: u32) {
    let mut cur_x = x;
    for &ch in text {
        if ch == b'\n' {
            return;
        }
        draw_char(cur_x, y, ch, color);
        cur_x += 16; // 16px wide when scaled 2x
    }
}

/// Calculate menu item width (16px per char for bitmap font)
fn calculate_menu_item_width(label_len: usize) -> u32 {
    let text_width = (label_len * 16) as u32;
    text_width + MENU_ITEM_PADDING_X * 2
}

/// Draw menu bar
fn draw_menu_bar() {
    unsafe {
        if FB_PTR.is_null() {
            return;
        }

        // Draw menu bar background
        draw_rect(0, 0, FB_WIDTH.load(Ordering::SeqCst), MENU_BAR_HEIGHT, MENU_BAR_COLOR);

        // Draw menu items
        let cursor_x = MOUSE_X.load(Ordering::SeqCst) as i32;
        let cursor_y = MOUSE_Y.load(Ordering::SeqCst) as i32;
        let at_limit = WINDOW_COUNT.load(Ordering::SeqCst) >= 4;

        let mut current_x = MENU_START_X;
        for (idx, item) in MENU_ITEMS.iter().enumerate() {
            let label_len = MENU_LABEL_LENS[idx];
            let item_width = calculate_menu_item_width(label_len);
            let item_y = MENU_START_Y;

            // Check if hovered
            let is_hovered = cursor_x >= current_x as i32 &&
                           cursor_x < (current_x + item_width) as i32 &&
                           cursor_y >= item_y as i32 &&
                           cursor_y < (item_y + MENU_ITEM_HEIGHT) as i32 &&
                           cursor_y < MENU_BAR_HEIGHT as i32;

            // Dim if at window limit
            let item_color = if at_limit && idx < MENU_ITEMS.len() {
                MENU_ITEM_COLOR
            } else if is_hovered {
                MENU_ITEM_HOVER_COLOR
            } else {
                MENU_ITEM_COLOR
            };

            // Draw menu item background
            draw_rect(current_x as i32, item_y as i32, item_width, MENU_ITEM_HEIGHT, item_color);

            // Draw text centered in button (16px tall font, 24px tall button -> 4px vertical padding)
            let text_x = (current_x + MENU_ITEM_PADDING_X) as i32;
            let text_y = (item_y + (MENU_ITEM_HEIGHT - 16) / 2) as i32;
            draw_text(text_x, text_y, &item.label[..label_len], TEXT_COLOR);

            current_x += item_width + 8; // 8px spacing between items
        }
    }
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

/// Redraw all window chrome and menu bar
fn redraw_all() {
    // print_debug("[WM] redraw_all: START\r\n");

    // Don't recalculate layout on every frame - only when windows change
    // Layout is calculated in handle_create_window() and handle_close_window()

    // Clear screen with dark background
    let fb_width = FB_WIDTH.load(Ordering::SeqCst);
    let fb_height = FB_HEIGHT.load(Ordering::SeqCst);

    // print_debug("[WM] redraw_all: About to clear screen\r\n");
    unsafe {
        let total_pixels = (fb_width * fb_height) as usize;
        for i in 0..total_pixels {
            FB_PTR.add(i).write_volatile(0xFF_1A_1A_1A); // Dark gray background

            // TODO: yield_now() during syscalls corrupts stack - needs proper fix
            // // Yield every 100k pixels to prevent CPU monopolization (2M total pixels)
            // // This allows other processes (Terminal, etc.) to run during the clear
            // if i % 100000 == 0 {
            //     yield_now();
            // }
        }
    }
    // print_debug("[WM] redraw_all: Screen cleared\r\n");

    // Draw menu bar first
    draw_rect(0, 0, fb_width, MENU_BAR_HEIGHT, MENU_BAR_COLOR);
    draw_menu_bar();
    // print_debug("[WM] redraw_all: Menu bar drawn\r\n");

    // Composite window content from shared memory, then draw chrome
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    for i in 0..count {
        // print_debug("[WM] redraw_all: Compositing window\r\n");
        let mut window = unsafe { core::ptr::read_volatile(&WINDOWS[i]) };
        if !window.visible {
            // print_debug("[WM] redraw_all: Window not visible, skipping\r\n");
            continue;
        }

        // Skip windows that don't have a buffer yet (waiting for first resize)
        if window.shm_id == 0 {
            // Draw window chrome only (no content)
            draw_window_chrome(&window);
            continue;
        }

        // Composite window content from shared memory
        // WM owns all buffers, so just map with regular shm_map
        let shm_id = window.shm_id;
        let shm_ptr = shm_map(shm_id);

        // If shared memory mapping fails, hide window (process likely died)
        if shm_ptr.is_null() {
            print_debug("[WM] shm_map failed for window, hiding it (process died?)\r\n");
            window.visible = false;
            unsafe {
                core::ptr::write_volatile(core::ptr::addr_of_mut!(WINDOWS[i]), window);
            }
            continue;
        }

        if !shm_ptr.is_null() {
            // Calculate content area (inside title bar and borders)
            let content_x = window.x + BORDER_WIDTH as i32;
            let content_y = window.y + TITLE_BAR_HEIGHT as i32;
            let content_width = window.width.saturating_sub(BORDER_WIDTH * 2);
            let content_height = window.height.saturating_sub(TITLE_BAR_HEIGHT + BORDER_WIDTH);

            // Copy pixels from shared memory to framebuffer
            let src_buffer = unsafe {
                core::slice::from_raw_parts(shm_ptr as *const u32, (content_width * content_height) as usize)
            };

            let fb_w = FB_WIDTH.load(Ordering::SeqCst) as usize;
            let fb_h = FB_HEIGHT.load(Ordering::SeqCst) as usize;

            // Use memcpy-style copy instead of pixel-by-pixel volatile writes
            unsafe {
                for y in 0..content_height {
                    let screen_y = (content_y + y as i32) as usize;
                    if screen_y >= fb_h {
                        continue;
                    }

                    let screen_x = content_x as usize;
                    if screen_x >= fb_w {
                        continue;
                    }

                    let copy_width = content_width.min((fb_w - screen_x) as u32) as usize;
                    let src_offset = (y * content_width) as usize;
                    let dst_offset = screen_y * fb_w + screen_x;

                    if src_offset + copy_width <= src_buffer.len() {
                        core::ptr::copy_nonoverlapping(
                            src_buffer.as_ptr().add(src_offset),
                            FB_PTR.add(dst_offset),
                            copy_width
                        );
                    }
                }
            }
            // print_debug("[WM] redraw_all: Pixels copied\r\n");
        }

        // Draw window chrome on top
        // print_debug("[WM] redraw_all: Drawing chrome\r\n");
        draw_window_chrome(&window);
        // print_debug("[WM] redraw_all: Chrome drawn\r\n");
    }
    // print_debug("[WM] redraw_all: DONE\r\n");
}

/// Calculate tiling layout for all windows
fn calculate_layout() {
    // Don't calculate layout if WM not fully initialized
    if !WM_INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    let count = WINDOW_COUNT.load(Ordering::SeqCst);
    if count == 0 {
        return;
    }

    let screen_width = FB_WIDTH.load(Ordering::SeqCst);
    let screen_height = FB_HEIGHT.load(Ordering::SeqCst);

    unsafe {
        let available_y = MENU_BAR_HEIGHT as i32;
        let available_height = screen_height.saturating_sub(MENU_BAR_HEIGHT);

        if count == 1 {
            // Single window: full screen below menu bar
            let window = WindowState {
                id: WINDOWS[0].id,
                shm_id: WINDOWS[0].shm_id,
                x: 0,
                y: available_y,
                width: screen_width,
                height: available_height,
                title: WINDOWS[0].title,
                title_len: WINDOWS[0].title_len,
                focused: WINDOWS[0].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[0], window);
        } else if count == 2 {
            // Two windows: 50/50 horizontal split
            let half_width = screen_width / 2;

            let window0 = WindowState {
                id: WINDOWS[0].id,
                shm_id: WINDOWS[0].shm_id,
                x: 0,
                y: available_y,
                width: half_width,
                height: available_height,
                title: WINDOWS[0].title,
                title_len: WINDOWS[0].title_len,
                focused: WINDOWS[0].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[0], window0);

            let window1 = WindowState {
                id: WINDOWS[1].id,
                shm_id: WINDOWS[1].shm_id,
                x: half_width as i32,
                y: available_y,
                width: half_width,
                height: available_height,
                title: WINDOWS[1].title,
                title_len: WINDOWS[1].title_len,
                focused: WINDOWS[1].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[1], window1);
        } else if count == 3 {
            // Three windows: 2 on left (split vertically), 1 on right
            let half_width = screen_width / 2;
            let half_height = available_height / 2;

            // Window 0: top-left
            let window0 = WindowState {
                id: WINDOWS[0].id,
                shm_id: WINDOWS[0].shm_id,
                x: 0,
                y: available_y,
                width: half_width,
                height: half_height,
                title: WINDOWS[0].title,
                title_len: WINDOWS[0].title_len,
                focused: WINDOWS[0].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[0], window0);

            // Window 1: full right side
            let window1 = WindowState {
                id: WINDOWS[1].id,
                shm_id: WINDOWS[1].shm_id,
                x: half_width as i32,
                y: available_y,
                width: half_width,
                height: available_height,
                title: WINDOWS[1].title,
                title_len: WINDOWS[1].title_len,
                focused: WINDOWS[1].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[1], window1);

            // Window 2: bottom-left
            let window2 = WindowState {
                id: WINDOWS[2].id,
                shm_id: WINDOWS[2].shm_id,
                x: 0,
                y: available_y + half_height as i32,
                width: half_width,
                height: half_height,
                title: WINDOWS[2].title,
                title_len: WINDOWS[2].title_len,
                focused: WINDOWS[2].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[2], window2);
        } else if count >= 4 {
            // Four windows: 2x2 grid (max 4 windows)
            let half_width = screen_width / 2;
            let half_height = available_height / 2;

            // Top-left
            let window0 = WindowState {
                id: WINDOWS[0].id,
                shm_id: WINDOWS[0].shm_id,
                x: 0,
                y: available_y,
                width: half_width,
                height: half_height,
                title: WINDOWS[0].title,
                title_len: WINDOWS[0].title_len,
                focused: WINDOWS[0].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[0], window0);

            // Top-right
            let window1 = WindowState {
                id: WINDOWS[1].id,
                shm_id: WINDOWS[1].shm_id,
                x: half_width as i32,
                y: available_y,
                width: half_width,
                height: half_height,
                title: WINDOWS[1].title,
                title_len: WINDOWS[1].title_len,
                focused: WINDOWS[1].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[1], window1);

            // Bottom-left
            let window2 = WindowState {
                id: WINDOWS[2].id,
                shm_id: WINDOWS[2].shm_id,
                x: 0,
                y: available_y + half_height as i32,
                width: half_width,
                height: half_height,
                title: WINDOWS[2].title,
                title_len: WINDOWS[2].title_len,
                focused: WINDOWS[2].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[2], window2);

            // Bottom-right
            let window3 = WindowState {
                id: WINDOWS[3].id,
                shm_id: WINDOWS[3].shm_id,
                x: half_width as i32,
                y: available_y + half_height as i32,
                width: half_width,
                height: half_height,
                title: WINDOWS[3].title,
                title_len: WINDOWS[3].title_len,
                focused: WINDOWS[3].focused,
                visible: true,
            };
            core::ptr::write_volatile(&mut WINDOWS[3], window3);
        }
    }
}

/// Add or update window (uses automatic tiling layout)
fn handle_create_window(id: usize, _x: i32, _y: i32, _width: u32, _height: u32, title: [u8; 64], title_len: usize) {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    print_debug("WM: handle_create_window called\r\n");

    // Check if window already exists
    for i in 0..count {
        let window = unsafe { &mut WINDOWS[i] };
        if window.id == id {
            print_debug("WM: Window already exists, ignoring duplicate CreateWindow\r\n");
            return;
        }
    }

    // Add new window with placeholder (buffer will be allocated after layout calculation)
    if count < MAX_WINDOWS {
        unsafe {
            WINDOWS[count] = WindowState {
                id,
                shm_id: 0,  // Will be allocated after layout calculation
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                title,
                title_len,
                focused: count == 0, // First window is focused
                visible: true,
            };
        }
        WINDOW_COUNT.store(count + 1, Ordering::SeqCst);

        print_debug("WM: Window added to array\r\n");

        // Calculate layout to get dimensions for this window
        calculate_layout();
        print_debug("WM: Layout calculated\r\n");

        // Now allocate buffer with the calculated dimensions
        let window = unsafe { &mut WINDOWS[count] };
        let content_width = window.width.saturating_sub(BORDER_WIDTH * 2);
        let content_height = window.height.saturating_sub(TITLE_BAR_HEIGHT + BORDER_WIDTH);

        if content_width > 0 && content_height > 0 {
            let buffer_size = (content_width * content_height * 4) as usize;
            print_debug("WM: Allocating buffer for window\r\n");
            let shm_id = shm_create(buffer_size);

            if shm_id < 0 {
                print_debug("WM: Failed to allocate buffer, removing window\r\n");
                WINDOW_COUNT.store(count, Ordering::SeqCst);
                return;
            }

            window.shm_id = shm_id;
            print_debug("WM: Buffer allocated, shm_id = ");
            print_debug("\r\n");

            // Send WindowCreated message to terminal with buffer info
            let created_msg = WMToKernel::WindowCreated {
                window_id: id,
                shm_id,
                width: content_width,
                height: content_height,
            };
            let msg_buf = created_msg.to_bytes();
            let result = send_message(id as u32, &msg_buf);

            if result < 0 {
                print_debug("WM: Failed to send WindowCreated to terminal\r\n");
            } else {
                print_debug("WM: WindowCreated message sent to terminal\r\n");
            }
        } else {
            print_debug("WM: Invalid dimensions, not allocating buffer\r\n");
        }
    }
}

/// Remove window and free its buffer
fn handle_close_window(id: usize) {
    let mut count = WINDOW_COUNT.load(Ordering::SeqCst);

    for i in 0..count {
        let window = unsafe { &WINDOWS[i] };
        if window.id == id {
            let was_focused = window.focused;
            let shm_id = window.shm_id;

            // Free the buffer (WM allocated it, WM frees it)
            if shm_id > 0 {
                print_debug("WM: Destroying buffer for closed window\r\n");
                shm_destroy(shm_id);
            }

            // Shift remaining windows down
            for j in i..count-1 {
                unsafe {
                    WINDOWS[j] = WINDOWS[j + 1];
                }
            }
            count -= 1;
            WINDOW_COUNT.store(count, Ordering::SeqCst);

            // If we removed the focused window, focus the last window
            if was_focused && count > 0 {
                unsafe {
                    WINDOWS[count - 1].focused = true;
                }
            }

            // Recalculate layout for remaining windows
            calculate_layout();
            return;
        }
    }
}

/// Set window focus
fn handle_set_focus(id: usize) {
    let count = WINDOW_COUNT.load(Ordering::SeqCst);

    for i in 0..count {
        // Use volatile read/write to prevent compiler optimization
        let mut window = unsafe { core::ptr::read_volatile(&WINDOWS[i]) };
        window.focused = window.id == id;
        unsafe {
            core::ptr::write_volatile(&mut WINDOWS[i], window);
        }
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
    }
    FB_WIDTH.store(fb_info.width, Ordering::SeqCst);
    FB_HEIGHT.store(fb_info.height, Ordering::SeqCst);
    WM_INITIALIZED.store(true, Ordering::SeqCst);

    print_debug("Framebuffer mapped\r\n");
    print_debug("WM fully initialized\r\n");

    // Initialize menu items (runtime init avoids .rodata relocation issues)
    print_debug("Initializing menu items...\r\n");
    init_menu_items();
    print_debug("Menu items initialized\r\n");

    // Draw initial UI (menu bar with text)
    print_debug("Drawing initial UI...\r\n");
    redraw_all();
    print_debug("Initial UI drawn\r\n");

    // PHASE 2: WM receives input from kernel via IPC
    print_debug("Window manager ready - listening for IPC messages\r\n");

    // Main event loop - receive input from kernel via IPC
    loop {
        // CRITICAL: Drain ALL pending messages before processing
        // This prevents queue overflow when kernel sends many mouse move events
        let mut messages_processed = 0;

        // Use AtomicBool to prevent compiler optimization in release mode
        // Without this, the compiler optimizes away the variable thinking it's never read
        use core::sync::atomic::{AtomicBool, Ordering};
        let need_redraw = AtomicBool::new(false);

        loop {
            let mut buf = [0u8; 256];
            let result = recv_message(&mut buf, 0); // Non-blocking

            if result <= 0 {
                break; // No more messages
            }

            messages_processed += 1;
            // print_debug("WM: Received message, type = ");

            // Parse message
            if let Some(msg) = KernelToWM::from_bytes(&buf) {
                match msg {
                    KernelToWM::InputEvent { sender_pid, mouse_x, mouse_y, event } => {
                        // print_debug("InputEvent\r\n");
                        // Handle input and determine routing
                        let response = handle_input(event, mouse_x, mouse_y);

                        // Send response back to kernel using sender_pid from message
                        let response_buf = response.to_bytes();
                        let result = send_message(sender_pid, &response_buf);

                        // If queue is full, stop processing and yield to let kernel drain
                        if result < 0 {
                            break; // Exit inner loop, will yield at outer loop
                        }

                        // Only redraw for events that change visual state
                        // Mouse moves don't need redraw - cursor position is tracked in atomics
                        // and menu hover is calculated during render
                        if event.event_type != 3 {  // 3 = MouseMove
                            need_redraw.store(true, Ordering::SeqCst);
                        }
                    }
                    KernelToWM::CreateWindow { id, x, y, width, height, title, title_len } => {
                        handle_create_window(id, x, y, width, height, title, title_len);
                        need_redraw.store(true, Ordering::SeqCst);
                    }
                    KernelToWM::CloseWindow { id } => {
                        handle_close_window(id);
                        need_redraw.store(true, Ordering::SeqCst);
                    }
                    KernelToWM::SetFocus { id } => {
                        print_debug("WM:GOT_SETFOCUS!");
                        handle_set_focus(id);
                        print_debug("WM:FOCUS_UPDATED!");
                        need_redraw.store(true, Ordering::SeqCst);
                        print_debug("WM:REDRAW_SET!");
                    }
                    KernelToWM::RequestRedraw { id: _ } => {
                        // Terminal updated its buffer content, trigger redraw
                        need_redraw.store(true, Ordering::SeqCst);
                    }
                }
            } else {
                print_debug("Failed to parse message\r\n");
            }

            // Limit batch processing to prevent starvation
            if messages_processed >= 100 {
                break;
            }
        }

        // Redraw once after processing all messages
        if need_redraw.load(Ordering::SeqCst) {
            print_debug("WM:REDRAWING!");
            redraw_all();
            fb_flush();
            print_debug("WM:REDRAW_DONE!");
        }

        // CRITICAL: ALWAYS yield, even if no redraw
        // Without this, WM monopolizes CPU in tight loop when rendering fails
        // (e.g., when terminal dies but window still in array)
        yield_now();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in window manager!\r\n");
    exit(1);
}
