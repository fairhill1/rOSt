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
const MAX_TILED_WINDOWS: usize = 4;
const TITLE_BAR_HEIGHT: u32 = 30;
const BORDER_WIDTH: u32 = 2;
const CLOSE_BUTTON_SIZE: u32 = 18;

// Menu bar constants
const MENU_BAR_HEIGHT: u32 = 32;
const MENU_ITEM_HEIGHT: u32 = 24;
const MENU_ITEM_PADDING_X: u32 = 16;
const MENU_START_X: u32 = 8;
const MENU_START_Y: u32 = 4;

// Colors
const TITLE_BAR_COLOR: u32 = 0xFF666666;
const TITLE_BAR_FOCUSED_COLOR: u32 = 0xFF2D5C88;
const BORDER_COLOR: u32 = 0xFF1A1A1A;
const TEXT_COLOR: u32 = 0xFFFFFFFF;
const CLOSE_BUTTON_COLOR: u32 = 0xFFCC3333;
const MENU_BAR_COLOR: u32 = 0xFF2B2B2B;
const MENU_ITEM_COLOR: u32 = 0xFF3D3D3D;
const MENU_ITEM_HOVER_COLOR: u32 = 0xFF5D5D5D;
const BG_COLOR: u32 = 0xFF1A1A1A;

/// Window state
#[derive(Clone, Copy)]
struct WindowState {
    id: usize,
    shm_id: i32,
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

/// App launcher definition
struct AppLauncher {
    label: &'static str,
    executable: &'static str,
}

const APPS: [AppLauncher; 5] = [
    AppLauncher { label: "Terminal", executable: "terminal" },
    AppLauncher { label: "Editor", executable: "editor" },
    AppLauncher { label: "Files", executable: "files" },
    AppLauncher { label: "Browser", executable: "browser" },
    AppLauncher { label: "Snake", executable: "snake" },
];

/// Framebuffer wrapper
struct Framebuffer {
    pixels: &'static mut [u32],
    width: u32,
    height: u32,
}

impl Framebuffer {
    fn new(ptr: *mut u32, width: u32, height: u32) -> Self {
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(ptr, (width * height) as usize)
        };
        Self { pixels, width, height }
    }

    fn clear(&mut self, color: u32) {
        for pixel in self.pixels.iter_mut() {
            *pixel = color;
        }
    }

    fn draw_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: u32) {
        // Clip to screen bounds
        if x >= self.width as i32 || y >= self.height as i32 ||
           x + width as i32 <= 0 || y + height as i32 <= 0 {
            return;
        }

        let start_x = x.max(0) as usize;
        let start_y = y.max(0) as usize;
        let end_x = ((x + width as i32).min(self.width as i32)) as usize;
        let end_y = ((y + height as i32).min(self.height as i32)) as usize;

        for py in start_y..end_y {
            for px in start_x..end_x {
                let offset = py * self.width as usize + px;
                self.pixels[offset] = color;
            }
        }
    }

    fn draw_char(&mut self, x: i32, y: i32, ch: u8, color: u32) {
        let char_data = if (ch as usize) < librost::graphics::FONT_8X8.len() {
            librost::graphics::FONT_8X8[ch as usize]
        } else {
            librost::graphics::FONT_8X8[0]
        };

        // Scale 2x - each pixel in 8x8 font becomes 2x2 block
        for (row, &byte) in char_data.iter().enumerate() {
            for col in 0..8 {
                if (byte & (0x80 >> col)) != 0 {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = x + (col * 2 + dx) as i32;
                            let py = y + (row * 2 + dy) as i32;
                            if px >= 0 && px < self.width as i32 &&
                               py >= 0 && py < self.height as i32 {
                                let idx = (py as usize * self.width as usize) + px as usize;
                                self.pixels[idx] = color;
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, text: &[u8], color: u32) {
        let mut cur_x = x;
        for &ch in text {
            if ch == b'\n' || ch == 0 {
                return;
            }
            self.draw_char(cur_x, y, ch, color);
            cur_x += 16; // 16px wide when scaled 2x
        }
    }
}

/// Main window manager state
struct WindowManager {
    windows: [WindowState; MAX_WINDOWS],
    window_count: AtomicUsize,  // Use atomic to prevent compiler optimization bugs
    mouse_x: i32,
    mouse_y: i32,
    framebuffer: Framebuffer,
}

impl WindowManager {
    fn new(fb_ptr: *mut u32, fb_width: u32, fb_height: u32) -> Self {
        Self {
            windows: [WindowState::new(); MAX_WINDOWS],
            window_count: AtomicUsize::new(0),
            mouse_x: 0,
            mouse_y: 0,
            framebuffer: Framebuffer::new(fb_ptr, fb_width, fb_height),
        }
    }

    /// Find window at given coordinates
    fn find_window_at(&self, x: i32, y: i32) -> Option<usize> {
        // Check windows in reverse order (top to bottom in Z order)
        let count = self.window_count.load(Ordering::SeqCst);
        for i in (0..count).rev() {
            let window = &self.windows[i];
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
    fn is_in_title_bar(&self, window: &WindowState, x: i32, y: i32) -> bool {
        x >= window.x && x < (window.x + window.width as i32) &&
        y >= window.y && y < (window.y + TITLE_BAR_HEIGHT as i32)
    }

    /// Check if click is on close button
    fn is_in_close_button(&self, window: &WindowState, x: i32, y: i32) -> bool {
        let btn_x = window.x + window.width as i32 - CLOSE_BUTTON_SIZE as i32 - 4;
        let btn_y = window.y + ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2) as i32;
        x >= btn_x && x < btn_x + CLOSE_BUTTON_SIZE as i32 &&
        y >= btn_y && y < btn_y + CLOSE_BUTTON_SIZE as i32
    }

    /// Calculate menu item width
    fn calculate_menu_item_width(label: &str) -> u32 {
        (label.len() * 16) as u32 + MENU_ITEM_PADDING_X * 2
    }

    /// Check if click is in menu bar
    fn check_menu_click(&self, mouse_x: i32, mouse_y: i32) -> Option<usize> {
        print_debug("[WM] check_menu_click: checking Y bounds\r\n");
        if mouse_y < MENU_START_Y as i32 ||
           mouse_y >= (MENU_START_Y + MENU_ITEM_HEIGHT) as i32 {
            print_debug("[WM] click outside menu Y range\r\n");
            return None;
        }

        print_debug("[WM] Y in range, checking apps\r\n");
        let mut current_x = MENU_START_X;
        for (idx, app) in APPS.iter().enumerate() {
            print_debug("[WM] Checking app index ");
            if idx < 10 {
                let idx_str = [b'0' + idx as u8];
                print_debug(core::str::from_utf8(&idx_str).unwrap());
            }
            print_debug("\r\n");

            print_debug("[WM] Getting app label...\r\n");
            let label = app.label;
            print_debug("[WM] Label: ");
            print_debug(label);
            print_debug("\r\n");

            print_debug("[WM] Calculating width...\r\n");
            let item_width = Self::calculate_menu_item_width(label);
            print_debug("[WM] Width calculated\r\n");

            if mouse_x >= current_x as i32 &&
               mouse_x < (current_x + item_width) as i32 &&
               mouse_y >= MENU_START_Y as i32 &&
               mouse_y < (MENU_START_Y + MENU_ITEM_HEIGHT) as i32 {
                print_debug("[WM] Match found!\r\n");
                return Some(idx);
            }

            current_x += item_width + 8; // 8px spacing
        }

        print_debug("[WM] No match, returning None\r\n");
        None
    }

    /// Handle input event
    fn handle_input(&mut self, event: InputEvent, mouse_x: i32, mouse_y: i32) -> WMToKernel {
        print_debug("[WM] handle_input START\r\n");

        // Debug: log all input events
        if event.event_type != 3 {  // Skip MouseMove spam
            print_debug("[WM] Input: type=");
            if event.event_type < 10 {
                let type_str = [b'0' + event.event_type as u8];
                print_debug(core::str::from_utf8(&type_str).unwrap());
            }
            print_debug(" pressed=");
            if event.pressed < 10 {
                let pressed_str = [b'0' + event.pressed as u8];
                print_debug(core::str::from_utf8(&pressed_str).unwrap());
            }
            print_debug("\r\n");
        }

        print_debug("[WM] About to update mouse pos\r\n");
        // Update mouse position
        self.mouse_x = mouse_x;
        self.mouse_y = mouse_y;
        print_debug("[WM] Mouse pos updated\r\n");

        // Handle mouse button clicks
        if event.event_type == 4 && event.pressed != 0 {
            print_debug("[WM] Mouse click detected at (");
            if mouse_x < 1000 {
                // Simple integer printing
                print_debug("x,");
            }
            print_debug("y)\r\n");

            // Check menu bar first
            print_debug("[WM] Checking menu click...\r\n");
            if let Some(menu_idx) = self.check_menu_click(mouse_x, mouse_y) {
                print_debug("[WM] Menu clicked! Index: ");
                if menu_idx < 10 {
                    let idx_str = [b'0' + menu_idx as u8];
                    print_debug(core::str::from_utf8(&idx_str).unwrap());
                }
                print_debug("\r\n");

                if self.window_count >= MAX_TILED_WINDOWS {
                    print_debug("[WM] At window limit!\r\n");
                    return WMToKernel::NoAction(NoActionMsg { msg_type: msg_types::WM_NO_ACTION });
                }

                let app = &APPS[menu_idx];
                print_debug("[WM] Spawning ELF: ");
                print_debug(app.executable);
                print_debug("\r\n");

                let result = spawn_elf(app.executable);
                print_debug("[WM] spawn_elf result: ");
                if result >= 0 {
                    print_debug("SUCCESS, PID=");
                    if result < 10 {
                        let pid_str = [b'0' + result as u8];
                        print_debug(core::str::from_utf8(&pid_str).unwrap());
                    }
                } else {
                    print_debug("FAILED!");
                }
                print_debug("\r\n");

                return WMToKernel::NoAction(NoActionMsg { msg_type: msg_types::WM_NO_ACTION });
            }

            // Check windows
            if let Some(window_id) = self.find_window_at(mouse_x, mouse_y) {
                for i in 0..self.window_count {
                    if self.windows[i].id == window_id {
                        let window = &self.windows[i];

                        // Check close button
                        if self.is_in_close_button(window, mouse_x, mouse_y) {
                            return WMToKernel::RequestClose(RequestCloseMsg {
                                msg_type: msg_types::WM_REQUEST_CLOSE,
                                _pad1: [0; 7],
                                window_id,
                            });
                        }

                        // Focus window if not focused
                        if !window.focused {
                            return WMToKernel::RequestFocus(RequestFocusMsg {
                                msg_type: msg_types::WM_REQUEST_FOCUS,
                                _pad1: [0; 7],
                                window_id,
                            });
                        }

                        // Send input to focused window
                        let msg = WMToKernel::RouteInput(RouteInputMsg {
                            msg_type: msg_types::WM_ROUTE_INPUT,
                            _pad1: [0; 7],
                            window_id,
                            event,
                        });
                        send_message(window_id as u32, &msg.to_bytes());
                        return WMToKernel::NoAction(NoActionMsg { msg_type: msg_types::WM_NO_ACTION });
                    }
                }
            }
        }

        // Handle ESC key (close focused window)
        if event.event_type == 1 && event.key == 1 {
            for window in &self.windows[..self.window_count] {
                if window.focused {
                    return WMToKernel::RequestClose(RequestCloseMsg {
                        msg_type: msg_types::WM_REQUEST_CLOSE,
                        _pad1: [0; 7],
                        window_id: window.id,
                    });
                }
            }
        }

        // Route keyboard input to focused window
        if event.event_type == 1 || event.event_type == 2 {
            for window in &self.windows[..self.window_count] {
                if window.focused {
                    let msg = WMToKernel::RouteInput(RouteInputMsg {
                        msg_type: msg_types::WM_ROUTE_INPUT,
                        _pad1: [0; 7],
                        window_id: window.id,
                        event,
                    });
                    send_message(window.id as u32, &msg.to_bytes());
                    return WMToKernel::NoAction(NoActionMsg { msg_type: msg_types::WM_NO_ACTION });
                }
            }
        }

        print_debug("[WM] handle_input END - returning NoAction\r\n");
        WMToKernel::NoAction(NoActionMsg { msg_type: msg_types::WM_NO_ACTION })
    }

    /// Draw menu bar
    fn draw_menu_bar(&mut self) {
        // Draw menu bar background
        self.framebuffer.draw_rect(0, 0, self.framebuffer.width, MENU_BAR_HEIGHT, MENU_BAR_COLOR);

        let at_limit = self.window_count >= MAX_TILED_WINDOWS;
        let mut current_x = MENU_START_X;

        for (idx, app) in APPS.iter().enumerate() {
            let item_width = Self::calculate_menu_item_width(app.label);
            let item_y = MENU_START_Y;

            // Check if hovered
            let is_hovered = self.mouse_x >= current_x as i32 &&
                           self.mouse_x < (current_x + item_width) as i32 &&
                           self.mouse_y >= item_y as i32 &&
                           self.mouse_y < (item_y + MENU_ITEM_HEIGHT) as i32 &&
                           self.mouse_y < MENU_BAR_HEIGHT as i32;

            let item_color = if at_limit && idx < APPS.len() {
                MENU_ITEM_COLOR
            } else if is_hovered {
                MENU_ITEM_HOVER_COLOR
            } else {
                MENU_ITEM_COLOR
            };

            // Draw menu item
            self.framebuffer.draw_rect(
                current_x as i32,
                item_y as i32,
                item_width,
                MENU_ITEM_HEIGHT,
                item_color
            );

            // Draw text
            let text_x = (current_x + MENU_ITEM_PADDING_X) as i32;
            let text_y = (item_y + (MENU_ITEM_HEIGHT - 16) / 2) as i32;
            self.framebuffer.draw_text(text_x, text_y, app.label.as_bytes(), TEXT_COLOR);

            current_x += item_width + 8;
        }
    }

    /// Draw window chrome
    fn draw_window_chrome(&mut self, window: &WindowState) {
        if !window.visible {
            return;
        }

        // Title bar
        let title_color = if window.focused {
            TITLE_BAR_FOCUSED_COLOR
        } else {
            TITLE_BAR_COLOR
        };

        self.framebuffer.draw_rect(window.x, window.y, window.width, TITLE_BAR_HEIGHT, title_color);

        // Title text
        let title_len = window.title_len.min(64);
        let title_text = &window.title[..title_len];

        // DEBUG: Print title bytes
        print_debug("Drawing title: len=");
        print_debug(&alloc::format!("{}, bytes=", title_len));
        for i in 0..title_len.min(16) {
            print_debug(&alloc::format!("{:02x} ", title_text[i]));
        }
        print_debug("\r\n");

        self.framebuffer.draw_text(window.x + 5, window.y + 7, title_text, TEXT_COLOR);

        // Close button
        let btn_x = window.x + window.width as i32 - CLOSE_BUTTON_SIZE as i32 - 4;
        let btn_y = window.y + ((TITLE_BAR_HEIGHT - CLOSE_BUTTON_SIZE) / 2) as i32;
        self.framebuffer.draw_rect(btn_x, btn_y, CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE, CLOSE_BUTTON_COLOR);

        // Draw X in close button
        let x_x = btn_x + 1;
        let x_y = btn_y + 1;
        self.framebuffer.draw_text(x_x, x_y, b"X", TEXT_COLOR);

        // Borders
        let border_y = window.y + TITLE_BAR_HEIGHT as i32;
        let border_height = window.height - TITLE_BAR_HEIGHT;

        // Left
        self.framebuffer.draw_rect(window.x, border_y, BORDER_WIDTH, border_height, BORDER_COLOR);
        // Right
        self.framebuffer.draw_rect(
            window.x + window.width as i32 - BORDER_WIDTH as i32,
            border_y,
            BORDER_WIDTH,
            border_height,
            BORDER_COLOR
        );
        // Bottom
        self.framebuffer.draw_rect(
            window.x,
            window.y + window.height as i32 - BORDER_WIDTH as i32,
            window.width,
            BORDER_WIDTH,
            BORDER_COLOR
        );
    }

    /// Calculate tiling layout
    fn calculate_layout(&mut self) {
        if self.window_count == 0 {
            return;
        }

        let screen_width = self.framebuffer.width;
        let screen_height = self.framebuffer.height;

        print_debug("calculate_layout: FB dims = ");
        print_debug(&alloc::format!("{}x{}\r\n", screen_width, screen_height));
        let available_y = MENU_BAR_HEIGHT as i32;
        let available_height = screen_height.saturating_sub(MENU_BAR_HEIGHT);

        match self.window_count {
            1 => {
                // Single window: full screen
                self.windows[0].x = 0;
                self.windows[0].y = available_y;
                self.windows[0].width = screen_width;
                self.windows[0].height = available_height;
                self.windows[0].visible = true;
            }
            2 => {
                // Two windows: 50/50 horizontal split
                let half_width = screen_width / 2;

                self.windows[0].x = 0;
                self.windows[0].y = available_y;
                self.windows[0].width = half_width;
                self.windows[0].height = available_height;
                self.windows[0].visible = true;

                self.windows[1].x = half_width as i32;
                self.windows[1].y = available_y;
                self.windows[1].width = half_width;
                self.windows[1].height = available_height;
                self.windows[1].visible = true;
            }
            3 => {
                // Three windows: 2 on left, 1 on right
                let half_width = screen_width / 2;
                let half_height = available_height / 2;

                self.windows[0].x = 0;
                self.windows[0].y = available_y;
                self.windows[0].width = half_width;
                self.windows[0].height = half_height;
                self.windows[0].visible = true;

                self.windows[1].x = half_width as i32;
                self.windows[1].y = available_y;
                self.windows[1].width = half_width;
                self.windows[1].height = available_height;
                self.windows[1].visible = true;

                self.windows[2].x = 0;
                self.windows[2].y = available_y + half_height as i32;
                self.windows[2].width = half_width;
                self.windows[2].height = half_height;
                self.windows[2].visible = true;
            }
            _ => {
                // Four+ windows: 2x2 grid
                let half_width = screen_width / 2;
                let half_height = available_height / 2;

                for i in 0..4.min(self.window_count) {
                    let (x_mult, y_mult) = match i {
                        0 => (0, 0), // Top-left
                        1 => (1, 0), // Top-right
                        2 => (0, 1), // Bottom-left
                        _ => (1, 1), // Bottom-right
                    };

                    self.windows[i].x = (half_width * x_mult) as i32;
                    self.windows[i].y = available_y + (half_height * y_mult) as i32;
                    self.windows[i].width = half_width;
                    self.windows[i].height = half_height;
                    self.windows[i].visible = true;
                }
            }
        }
    }

    /// Redraw everything
    fn redraw_all(&mut self) {
        // Clear screen
        self.framebuffer.clear(BG_COLOR);

        // Draw menu bar
        self.draw_menu_bar();

        // Composite window content and draw chrome
        for i in 0..self.window_count {
            let window = self.windows[i];

            if window.visible && window.shm_id != 0 {
                // Map shared memory
                let shm_ptr = shm_map(window.shm_id);

                if shm_ptr.is_null() {
                    self.windows[i].visible = false;
                    continue;
                }

                // Calculate content area
                let content_x = window.x + BORDER_WIDTH as i32;
                let content_y = window.y + TITLE_BAR_HEIGHT as i32;
                let content_width = window.width.saturating_sub(BORDER_WIDTH * 2);
                let content_height = window.height.saturating_sub(TITLE_BAR_HEIGHT + BORDER_WIDTH);

                // Copy pixels from shared memory
                let src_buffer = unsafe {
                    core::slice::from_raw_parts(
                        shm_ptr as *const u32,
                        (content_width * content_height) as usize
                    )
                };

                // Composite to framebuffer (row-by-row for better performance)
                for y in 0..content_height {
                    let screen_y = (content_y + y as i32) as usize;
                    if screen_y >= self.framebuffer.height as usize {
                        continue;
                    }

                    let screen_x = content_x as usize;
                    if screen_x >= self.framebuffer.width as usize {
                        continue;
                    }

                    let copy_width = content_width.min((self.framebuffer.width as usize - screen_x) as u32) as usize;
                    let src_offset = (y * content_width) as usize;
                    let dst_offset = screen_y * self.framebuffer.width as usize + screen_x;

                    if src_offset + copy_width <= src_buffer.len() &&
                       dst_offset + copy_width <= self.framebuffer.pixels.len() {
                        let src_row = &src_buffer[src_offset..src_offset + copy_width];
                        let dst_row = &mut self.framebuffer.pixels[dst_offset..dst_offset + copy_width];
                        dst_row.copy_from_slice(src_row);
                    }
                }
            }

            // Draw window chrome
            self.draw_window_chrome(&window);
        }
    }

    /// Handle CreateWindow message
    fn handle_create_window(&mut self, id: usize, _x: i32, _y: i32, _width: u32, _height: u32, title: [u8; 64], title_len: usize) {
        print_debug("WM: handle_create_window called\r\n");

        // DEBUG: Print received title
        print_debug("Received title: len=");
        print_debug(&alloc::format!("{}, bytes=[", title_len));
        for i in 0..title_len.min(16) {
            print_debug(&alloc::format!("{:02x} ", title[i]));
        }
        print_debug("]\r\n");

        // DEBUG: Check FB dims at start of function
        print_debug("WM: FB dims at handler start: ");
        print_debug(&alloc::format!("{}x{}\r\n", self.framebuffer.width, self.framebuffer.height));

        // Check if window already exists
        for window in &self.windows[..self.window_count] {
            if window.id == id {
                print_debug("WM: Window already exists, ignoring\r\n");
                return;
            }
        }

        // Add new window
        if self.window_count < MAX_WINDOWS {
            self.windows[self.window_count] = WindowState {
                id,
                shm_id: 0,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                title,
                title_len,
                focused: self.window_count == 0,
                visible: true,
            };
            self.window_count += 1;
            print_debug("WM: Window added to array\r\n");

            // Calculate layout
            self.calculate_layout();
            print_debug("WM: Layout calculated\r\n");

            // Allocate buffer
            let window = &mut self.windows[self.window_count - 1];
            print_debug("Window dims: ");
            print_debug(&alloc::format!("{}x{}\r\n", window.width, window.height));
            let content_width = window.width.saturating_sub(BORDER_WIDTH * 2);
            let content_height = window.height.saturating_sub(TITLE_BAR_HEIGHT + BORDER_WIDTH);
            print_debug("Content dims: ");
            print_debug(&alloc::format!("{}x{}\r\n", content_width, content_height));

            if content_width > 0 && content_height > 0 {
                let buffer_size = (content_width * content_height * 4) as usize;
                print_debug("WM: Allocating buffer\r\n");
                let shm_id = shm_create(buffer_size);

                if shm_id >= 0 {
                    window.shm_id = shm_id;
                    print_debug("WM: Buffer allocated, sending WindowCreated\r\n");

                    // Send WindowCreated message
                    let msg = WMToKernel::WindowCreated(WindowCreatedMsg {
                        msg_type: msg_types::WM_WINDOW_CREATED,
                        _pad1: [0; 7],
                        window_id: id,
                        shm_id,
                        _pad2: [0; 4],
                        width: content_width,
                        height: content_height,
                    });
                    send_message(id as u32, &msg.to_bytes());
                    print_debug("WM: WindowCreated message sent\r\n");
                } else {
                    print_debug("WM: Failed to allocate buffer\r\n");
                    self.window_count -= 1;
                }
            } else {
                print_debug("WM: Invalid dimensions\r\n");
            }
        } else {
            print_debug("WM: MAX_WINDOWS reached\r\n");
        }
    }

    /// Handle CloseWindow message
    fn handle_close_window(&mut self, id: usize) {
        for i in 0..self.window_count {
            if self.windows[i].id == id {
                let was_focused = self.windows[i].focused;
                let shm_id = self.windows[i].shm_id;

                // Free buffer
                if shm_id > 0 {
                    shm_destroy_from_process(id, shm_id);
                }

                // Shift windows down
                for j in i..self.window_count - 1 {
                    self.windows[j] = self.windows[j + 1];
                }
                self.window_count -= 1;

                // Focus last window if needed
                if was_focused && self.window_count > 0 {
                    self.windows[self.window_count - 1].focused = true;
                }

                // Recalculate layout
                self.calculate_layout();
                return;
            }
        }
    }

    /// Handle SetFocus message
    fn handle_set_focus(&mut self, id: usize) {
        for window in &mut self.windows[..self.window_count] {
            window.focused = window.id == id;
        }
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Userspace Window Manager ===\r\n");

    // Get framebuffer
    print_debug("WM: Calling fb_info() syscall\r\n");
    let fb_info = match fb_info() {
        Some(info) => {
            print_debug("WM: fb_info() returned successfully\r\n");
            info
        }
        None => {
            print_debug("Failed to get framebuffer info\r\n");
            exit(1);
        }
    };

    let fb_ptr = match fb_map() {
        Some(ptr) => ptr,
        None => {
            print_debug("Failed to map framebuffer\r\n");
            exit(1);
        }
    };

    // Create window manager (on heap to avoid stack overflow)
    print_debug("WM: Creating WindowManager struct\r\n");
    let mut wm = alloc::boxed::Box::new(WindowManager::new(fb_ptr, fb_info.width, fb_info.height));

    // CRITICAL: Verify window_count is initialized correctly
    use core::hint::black_box;
    let count = black_box(wm.window_count);
    if count != 0 {
        print_debug("[ERROR] window_count corrupted at init!\r\n");
        exit(1);
    }
    print_debug("WM: window_count verified = 0\r\n");

    print_debug("WM initialized\r\n");

    // Draw initial UI
    wm.redraw_all();
    fb_flush();

    print_debug("WM ready\r\n");

    // Main event loop
    loop {
        let mut need_redraw = false;
        let mut messages_processed = 0;

        // Drain message queue
        loop {
            let mut buf = [0u8; 256];
            let result = recv_message(&mut buf, 0);

            if result <= 0 {
                break;
            }

            messages_processed += 1;

            if let Some(msg) = KernelToWM::from_bytes(&buf) {
                match msg {
                    KernelToWM::InputEvent(msg) => {
                        if msg.event.event_type != 3 {  // Skip MouseMove spam
                            print_debug("[WM] Received InputEvent message\r\n");
                        }
                        let response = wm.handle_input(msg.event, msg.mouse_x, msg.mouse_y);
                        print_debug("[WM] Returned from handle_input, calling to_bytes\r\n");
                        let bytes = response.to_bytes();
                        print_debug("[WM] to_bytes done, calling send_message\r\n");
                        send_message(msg.sender_pid, &bytes);
                        print_debug("[WM] send_message done\r\n");

                        if msg.event.event_type != 3 { // Not MouseMove
                            need_redraw = true;
                        }
                    }
                    KernelToWM::CreateWindow(msg) => {
                        print_debug("[WM] Received CreateWindow from PID ");
                        if msg.id < 10 {
                            let pid_str = [b'0' + msg.id as u8];
                            print_debug(core::str::from_utf8(&pid_str).unwrap());
                        }
                        print_debug("\r\n");
                        wm.handle_create_window(msg.id, msg.x, msg.y, msg.width, msg.height, msg.title, msg.title_len);
                        need_redraw = true;
                    }
                    KernelToWM::CloseWindow(msg) => {
                        wm.handle_close_window(msg.id);
                        need_redraw = true;
                    }
                    KernelToWM::SetFocus(msg) => {
                        wm.handle_set_focus(msg.id);
                        need_redraw = true;
                    }
                    KernelToWM::RequestRedraw(_) => {
                        need_redraw = true;
                    }
                }
            }

            if messages_processed >= 100 {
                break;
            }
        }

        // Redraw once
        if need_redraw {
            wm.redraw_all();
            fb_flush();
        }

        yield_now();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use alloc::format;

    print_debug("PANIC in window manager!\r\n");

    // Print location if available
    if let Some(location) = info.location() {
        let msg = format!("Location: {}:{}\r\n", location.file(), location.line());
        print_debug(&msg);
    }

    // Print panic message if available
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        let msg = format!("Message: {}\r\n", s);
        print_debug(&msg);
    }

    exit(1);
}
