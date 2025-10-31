// GUI Window Manager - Desktop environment with draggable windows

use crate::kernel::framebuffer;
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

// Window decorations
const TITLE_BAR_HEIGHT: u32 = 24;
const BORDER_WIDTH: u32 = 2;
const CLOSE_BUTTON_SIZE: u32 = 16;

// Menu bar
const MENU_BAR_HEIGHT: u32 = 32;
const MENU_ITEM_HEIGHT: u32 = 24;
const MENU_ITEM_SPACING: u32 = 8;
const MENU_ITEM_PADDING_X: u32 = 16; // Horizontal padding inside button
const MENU_START_X: u32 = 8;
const MENU_START_Y: u32 = 4;
const CHAR_WIDTH: u32 = 16; // Width of each character (from font)

// Colors
const COLOR_TITLE_BAR: u32 = 0xFF2D5C88;      // Blue title bar
const COLOR_TITLE_BAR_INACTIVE: u32 = 0xFF666666; // Gray when not focused
const COLOR_BORDER: u32 = 0xFF1A1A1A;         // Dark border
const COLOR_WINDOW_BG: u32 = 0xFF000000;      // Black window background
const COLOR_TEXT: u32 = 0xFFFFFFFF;           // White text
const COLOR_CLOSE_BTN: u32 = 0xFFCC3333;      // Red close button
const COLOR_MENU_BAR: u32 = 0xFF2B2B2B;       // Lighter gray menu bar
const COLOR_MENU_ITEM: u32 = 0xFF3D3D3D;      // Menu item background
const COLOR_MENU_ITEM_BORDER: u32 = 0xFF555555; // Menu item border

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

        // Draw X in close button (centered - X is 16px wide, button is 16px, so start at btn_x)
        framebuffer::draw_string(btn_x as u32, btn_y as u32, "X", COLOR_TEXT);

        // Draw content area background
        let (cx, cy, cw, ch) = self.get_content_bounds();
        self.draw_rect(cx, cy, cw, ch, COLOR_WINDOW_BG);

        // Draw content based on type
        self.render_content(cx, cy, cw, ch);
    }

    fn render_content(&self, x: i32, y: i32, width: u32, height: u32) {
        match self.content {
            WindowContent::Terminal => {
                // Terminal content is rendered by the console system directly
                // (see main rendering loop which calls console::render_at())
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

struct MenuItem {
    label: &'static str,
    window_type: WindowContent,
}

const MENU_ITEMS: &[MenuItem] = &[
    MenuItem { label: "Terminal", window_type: WindowContent::Terminal },
    MenuItem { label: "About", window_type: WindowContent::AboutDialog },
];

pub struct WindowManager {
    windows: Vec<Window>,
    next_id: usize,
    screen_width: u32,
    screen_height: u32,
}

impl WindowManager {
    pub fn new() -> Self {
        // Get screen dimensions from framebuffer
        let (width, height) = framebuffer::get_screen_dimensions();
        WindowManager {
            windows: Vec::new(),
            next_id: 0,
            screen_width: width,
            screen_height: height,
        }
    }

    /// Calculate tiling layout for all windows
    fn calculate_layout(&mut self) {
        let num_windows = self.windows.len();
        if num_windows == 0 {
            return;
        }

        let available_y = MENU_BAR_HEIGHT as i32;
        let available_height = self.screen_height - MENU_BAR_HEIGHT;

        if num_windows == 1 {
            // Single window: full screen below menu bar
            self.windows[0].x = 0;
            self.windows[0].y = available_y;
            self.windows[0].width = self.screen_width;
            self.windows[0].height = available_height;
        } else if num_windows == 2 {
            // Two windows: 50/50 horizontal split
            let half_width = self.screen_width / 2;

            self.windows[0].x = 0;
            self.windows[0].y = available_y;
            self.windows[0].width = half_width;
            self.windows[0].height = available_height;

            self.windows[1].x = half_width as i32;
            self.windows[1].y = available_y;
            self.windows[1].width = half_width;
            self.windows[1].height = available_height;
        }
        // For 3+ windows, we'll implement later
    }

    /// Calculate menu item width based on text length
    fn calculate_menu_item_width(label: &str) -> u32 {
        let text_width = label.len() as u32 * CHAR_WIDTH;
        text_width + MENU_ITEM_PADDING_X * 2 // Add padding on both sides
    }

    /// Render the menu bar
    fn render_menu_bar(&self) {
        // Draw menu bar background
        for y in 0..MENU_BAR_HEIGHT {
            for x in 0..self.screen_width {
                framebuffer::draw_pixel(x, y, COLOR_MENU_BAR);
            }
        }

        // Draw menu items with borders and backgrounds
        let mut current_x = MENU_START_X;
        for item in MENU_ITEMS.iter() {
            let item_width = Self::calculate_menu_item_width(item.label);
            let item_y = MENU_START_Y;

            // Draw menu item border
            self.draw_menu_rect(current_x, item_y, item_width, MENU_ITEM_HEIGHT, COLOR_MENU_ITEM_BORDER);

            // Draw menu item background (inset by 1 pixel for border)
            self.draw_menu_rect(current_x + 1, item_y + 1,
                               item_width - 2, MENU_ITEM_HEIGHT - 2,
                               COLOR_MENU_ITEM);

            // Draw menu item text (centered with padding)
            let text_x = current_x + MENU_ITEM_PADDING_X;
            let text_y = item_y + 4;
            framebuffer::draw_string(text_x, text_y, item.label, COLOR_TEXT);

            // Move to next position
            current_x += item_width + MENU_ITEM_SPACING;
        }
    }

    /// Helper to draw rectangles for menu items
    fn draw_menu_rect(&self, x: u32, y: u32, width: u32, height: u32, color: u32) {
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx;
                let py = y + dy;
                if px < self.screen_width && py < MENU_BAR_HEIGHT {
                    framebuffer::draw_pixel(px, py, color);
                }
            }
        }
    }

    /// Check if menu bar was clicked, return window type to create
    fn check_menu_click(&self, x: i32, y: i32) -> Option<WindowContent> {
        // Check if click is in menu bar area
        if y < 0 || y >= MENU_BAR_HEIGHT as i32 {
            return None;
        }

        // Check each menu item
        let mut current_x = MENU_START_X;
        for item in MENU_ITEMS.iter() {
            let item_width = Self::calculate_menu_item_width(item.label);
            let item_y = MENU_START_Y;
            let item_end_x = current_x + item_width;
            let item_end_y = item_y + MENU_ITEM_HEIGHT;

            if x >= current_x as i32 && x < item_end_x as i32 &&
               y >= item_y as i32 && y < item_end_y as i32 {
                // Check if window of this type already exists
                let already_exists = self.windows.iter()
                    .any(|w| w.content == item.window_type);

                if !already_exists {
                    return Some(item.window_type);
                }
            }

            // Move to next position
            current_x += item_width + MENU_ITEM_SPACING;
        }
        None
    }

    /// Add a new window
    pub fn add_window(&mut self, window: Window) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.push(window);

        // Focus the new window
        self.focus_window(self.windows.len() - 1);

        // Recalculate tiling layout
        self.calculate_layout();

        // Mark console as dirty so it redraws at new position
        crate::kernel::console::mark_dirty();

        id
    }

    /// Remove a window by index
    pub fn remove_window(&mut self, index: usize) {
        if index < self.windows.len() {
            self.windows.remove(index);
            // Recalculate tiling layout
            self.calculate_layout();
            // Mark console as dirty so it redraws at new position
            crate::kernel::console::mark_dirty();
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

            // Mark console as dirty so it redraws
            crate::kernel::console::mark_dirty();
        }
    }

    /// Handle mouse click
    pub fn handle_click(&mut self, x: i32, y: i32) -> bool {
        // First check if menu bar was clicked
        if let Some(window_type) = self.check_menu_click(x, y) {
            // Create the requested window
            let title = match window_type {
                WindowContent::Terminal => "Terminal",
                WindowContent::AboutDialog => "About rOSt",
            };
            let window = Window::new(0, 0, 640, 480, title, window_type);
            self.add_window(window);
            return true;
        }

        // Check windows in reverse order (top to bottom)
        for i in (0..self.windows.len()).rev() {
            if self.windows[i].contains_point(x, y) {
                // Check close button first
                if self.windows[i].is_close_button_clicked(x, y) {
                    self.remove_window(i);
                    return true;
                }

                // Focus this window (no more dragging)
                self.focus_window(i);
                return true;
            }
        }
        false
    }

    /// Render all windows and menu bar
    pub fn render(&self) {
        // Draw menu bar first
        self.render_menu_bar();

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

    /// Get terminal window content bounds (for rendering console inside)
    pub fn get_terminal_content_bounds(&self) -> Option<(i32, i32, u32, u32)> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Terminal && w.visible)
            .last()
            .map(|w| w.get_content_bounds())
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

pub fn get_terminal_content_bounds() -> Option<(i32, i32, u32, u32)> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_terminal_content_bounds()
        } else {
            None
        }
    }
}
