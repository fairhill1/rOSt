// GUI Window Manager - Desktop environment with draggable windows

use crate::kernel::framebuffer;
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

// Window decorations
const TITLE_BAR_HEIGHT: u32 = 24;
const BORDER_WIDTH: u32 = 2;
const CLOSE_BUTTON_SIZE: u32 = 16;

// Colors
const COLOR_TITLE_BAR: u32 = 0xFF2D5C88;      // Blue title bar
const COLOR_TITLE_BAR_INACTIVE: u32 = 0xFF666666; // Gray when not focused
const COLOR_BORDER: u32 = 0xFF1A1A1A;         // Dark border
const COLOR_WINDOW_BG: u32 = 0xFF000000;      // Black window background
const COLOR_TEXT: u32 = 0xFFFFFFFF;           // White text
const COLOR_CLOSE_BTN: u32 = 0xFFCC3333;      // Red close button

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowContent {
    Terminal,
    AboutDialog,
}

pub struct Window {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub content: WindowContent,
    pub is_focused: bool,
    pub visible: bool,

    // For dragging
    pub is_dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
}

impl Window {
    pub fn new(x: i32, y: i32, width: u32, height: u32, title: &str, content: WindowContent) -> Self {
        Window {
            x,
            y,
            width,
            height,
            title: String::from(title),
            content,
            is_focused: false,
            visible: true,
            is_dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Check if a point is inside the window
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width as i32 &&
        py >= self.y && py < self.y + self.height as i32
    }

    /// Check if a point is in the title bar
    pub fn is_in_title_bar(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width as i32 &&
        py >= self.y && py < self.y + TITLE_BAR_HEIGHT as i32
    }

    /// Check if close button was clicked
    pub fn is_close_button_clicked(&self, px: i32, py: i32) -> bool {
        let btn_x = self.x + self.width as i32 - CLOSE_BUTTON_SIZE as i32 - 4;
        let btn_y = self.y + 4;
        px >= btn_x && px < btn_x + CLOSE_BUTTON_SIZE as i32 &&
        py >= btn_y && py < btn_y + CLOSE_BUTTON_SIZE as i32
    }

    /// Get content area bounds (excluding title bar and border)
    pub fn get_content_bounds(&self) -> (i32, i32, u32, u32) {
        let content_x = self.x + BORDER_WIDTH as i32;
        let content_y = self.y + TITLE_BAR_HEIGHT as i32;
        let content_w = self.width.saturating_sub(BORDER_WIDTH * 2);
        let content_h = self.height.saturating_sub(TITLE_BAR_HEIGHT + BORDER_WIDTH);
        (content_x, content_y, content_w, content_h)
    }

    /// Render the window
    pub fn render(&self) {
        if !self.visible {
            return;
        }

        // Draw border
        self.draw_rect(self.x, self.y, self.width, self.height, COLOR_BORDER);

        // Draw title bar
        let title_color = if self.is_focused { COLOR_TITLE_BAR } else { COLOR_TITLE_BAR_INACTIVE };
        self.draw_rect(
            self.x + BORDER_WIDTH as i32,
            self.y + BORDER_WIDTH as i32,
            self.width - BORDER_WIDTH * 2,
            TITLE_BAR_HEIGHT - BORDER_WIDTH,
            title_color
        );

        // Draw title text
        framebuffer::draw_string(
            (self.x + 8) as u32,
            (self.y + 4) as u32,
            &self.title,
            COLOR_TEXT
        );

        // Draw close button (red square with X)
        let btn_x = self.x + self.width as i32 - CLOSE_BUTTON_SIZE as i32 - 4;
        let btn_y = self.y + 4;
        self.draw_rect(btn_x, btn_y, CLOSE_BUTTON_SIZE, CLOSE_BUTTON_SIZE, COLOR_CLOSE_BTN);

        // Draw X in close button
        framebuffer::draw_string((btn_x + 4) as u32, (btn_y + 0) as u32, "X", COLOR_TEXT);

        // Draw content area background
        let (cx, cy, cw, ch) = self.get_content_bounds();
        self.draw_rect(cx, cy, cw, ch, COLOR_WINDOW_BG);

        // Draw content based on type
        self.render_content(cx, cy, cw, ch);
    }

    fn render_content(&self, x: i32, y: i32, width: u32, height: u32) {
        match self.content {
            WindowContent::Terminal => {
                // Terminal content will be rendered by the console system
                // This is just a placeholder
                framebuffer::draw_string((x + 4) as u32, (y + 4) as u32, "Terminal", COLOR_TEXT);
            }
            WindowContent::AboutDialog => {
                framebuffer::draw_string((x + 8) as u32, (y + 8) as u32, "rOSt - Rust OS", COLOR_TEXT);
                framebuffer::draw_string((x + 8) as u32, (y + 28) as u32, "v0.1.0", COLOR_TEXT);
                framebuffer::draw_string((x + 8) as u32, (y + 48) as u32, "A Rust ARM64 OS", COLOR_TEXT);
            }
        }
    }

    fn draw_rect(&self, x: i32, y: i32, width: u32, height: u32, color: u32) {
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && py >= 0 {
                    framebuffer::draw_pixel(px as u32, py as u32, color);
                }
            }
        }
    }
}

pub struct WindowManager {
    windows: Vec<Window>,
    next_id: usize,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            windows: Vec::new(),
            next_id: 0,
        }
    }

    /// Add a new window
    pub fn add_window(&mut self, window: Window) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.push(window);

        // Focus the new window
        self.focus_window(self.windows.len() - 1);

        id
    }

    /// Remove a window by index
    pub fn remove_window(&mut self, index: usize) {
        if index < self.windows.len() {
            self.windows.remove(index);
        }
    }

    /// Focus a window (bring to front)
    fn focus_window(&mut self, index: usize) {
        // Unfocus all windows
        for win in &mut self.windows {
            win.is_focused = false;
        }

        // Focus the selected window
        if index < self.windows.len() {
            self.windows[index].is_focused = true;

            // Move to end (top of z-order)
            let window = self.windows.remove(index);
            self.windows.push(window);
        }
    }

    /// Handle mouse click
    pub fn handle_click(&mut self, x: i32, y: i32) -> bool {
        // Check windows in reverse order (top to bottom)
        for i in (0..self.windows.len()).rev() {
            if self.windows[i].contains_point(x, y) {
                // Check close button first
                if self.windows[i].is_close_button_clicked(x, y) {
                    self.remove_window(i);
                    return true;
                }

                // Check if clicking title bar to start drag
                if self.windows[i].is_in_title_bar(x, y) {
                    self.windows[i].is_dragging = true;
                    self.windows[i].drag_offset_x = x - self.windows[i].x;
                    self.windows[i].drag_offset_y = y - self.windows[i].y;
                }

                // Focus this window
                self.focus_window(i);
                return true;
            }
        }
        false
    }

    /// Handle mouse release
    pub fn handle_release(&mut self) {
        for win in &mut self.windows {
            win.is_dragging = false;
        }
    }

    /// Handle mouse move (for dragging)
    /// Returns true if any window was moved (needs redraw)
    pub fn handle_move(&mut self, x: i32, y: i32) -> bool {
        let mut moved = false;
        for win in &mut self.windows {
            if win.is_dragging {
                win.x = x - win.drag_offset_x;
                win.y = y - win.drag_offset_y;
                moved = true;
            }
        }
        moved
    }

    /// Render all windows
    pub fn render(&self) {
        // Draw windows in order (bottom to top)
        for window in &self.windows {
            window.render();
        }
    }

    /// Get the focused terminal window (if any)
    pub fn get_focused_terminal(&self) -> Option<&Window> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Terminal && w.is_focused)
            .last()
    }
}

static mut WINDOW_MANAGER: Option<WindowManager> = None;

pub fn init() {
    unsafe {
        WINDOW_MANAGER = Some(WindowManager::new());
    }
}

pub fn add_window(window: Window) -> usize {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.add_window(window)
        } else {
            0
        }
    }
}

pub fn handle_mouse_click(x: i32, y: i32) -> bool {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_click(x, y)
        } else {
            false
        }
    }
}

pub fn handle_mouse_release() {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_release();
        }
    }
}

pub fn handle_mouse_move(x: i32, y: i32) -> bool {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_move(x, y)
        } else {
            false
        }
    }
}

pub fn render() {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.render();
        }
    }
}

pub fn has_focused_terminal() -> bool {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_terminal().is_some()
        } else {
            false
        }
    }
}
