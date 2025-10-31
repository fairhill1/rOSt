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
const COLOR_MENU_ITEM_HOVER: u32 = 0xFF5D5D5D; // Menu item background on hover (brighter)
const COLOR_MENU_ITEM_BORDER: u32 = 0xFF555555; // Menu item border

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowContent {
    Terminal,
    AboutDialog,
    Editor,
    FileExplorer,
    Snake,
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
    pub instance_id: usize, // ID for the console/editor instance
}

impl Window {
    pub fn new(x: i32, y: i32, width: u32, height: u32, title: &str, content: WindowContent, instance_id: usize) -> Self {
        Window {
            x,
            y,
            width,
            height,
            title: String::from(title),
            content,
            is_focused: false,
            visible: true,
            instance_id,
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
            WindowContent::Editor => {
                // Editor content is rendered by the editor system directly
                // (see main rendering loop which calls editor::render_at())
            }
            WindowContent::FileExplorer => {
                // File explorer content is rendered by the file_explorer system directly
                // (see main rendering loop which calls file_explorer::render_at())
            }
            WindowContent::Snake => {
                // Snake game content is rendered by the snake system directly
                // (see main rendering loop which calls snake::render_at())
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
    MenuItem { label: "Editor", window_type: WindowContent::Editor },
    MenuItem { label: "Files", window_type: WindowContent::FileExplorer },
    MenuItem { label: "Snake", window_type: WindowContent::Snake },
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
        } else if num_windows == 3 {
            // Three windows: split left side vertically
            // Layout:
            // +----------+----------+
            // |    0     |          |
            // |----------|    1     |
            // |    2     |          |
            // +----------+----------+
            let half_width = self.screen_width / 2;
            let half_height = available_height / 2;

            // Window 0: top-left
            self.windows[0].x = 0;
            self.windows[0].y = available_y;
            self.windows[0].width = half_width;
            self.windows[0].height = half_height;

            // Window 1: full right side
            self.windows[1].x = half_width as i32;
            self.windows[1].y = available_y;
            self.windows[1].width = half_width;
            self.windows[1].height = available_height;

            // Window 2: bottom-left
            self.windows[2].x = 0;
            self.windows[2].y = available_y + half_height as i32;
            self.windows[2].width = half_width;
            self.windows[2].height = half_height;
        } else if num_windows >= 4 {
            // Four windows: 2x2 grid
            // Layout:
            // +----------+----------+
            // |    0     |    1     |
            // |----------|----------|
            // |    2     |    3     |
            // +----------+----------+
            let half_width = self.screen_width / 2;
            let half_height = available_height / 2;

            // Window 0: top-left
            self.windows[0].x = 0;
            self.windows[0].y = available_y;
            self.windows[0].width = half_width;
            self.windows[0].height = half_height;

            // Window 1: top-right
            self.windows[1].x = half_width as i32;
            self.windows[1].y = available_y;
            self.windows[1].width = half_width;
            self.windows[1].height = half_height;

            // Window 2: bottom-left
            self.windows[2].x = 0;
            self.windows[2].y = available_y + half_height as i32;
            self.windows[2].width = half_width;
            self.windows[2].height = half_height;

            // Window 3: bottom-right
            self.windows[3].x = half_width as i32;
            self.windows[3].y = available_y + half_height as i32;
            self.windows[3].width = half_width;
            self.windows[3].height = half_height;

            // If there are more than 4 windows, only show the first 4
            // (hide the extras)
            for i in 4..num_windows {
                self.windows[i].visible = false;
            }
        }
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

        // Draw time in top right corner
        let datetime = crate::kernel::rtc::get_datetime();
        let time_str = datetime.format_time();
        let time_width = time_str.len() as u32 * CHAR_WIDTH;
        let time_x = self.screen_width.saturating_sub(time_width + 8); // 8px padding from right edge
        let time_y = MENU_START_Y + 4;
        framebuffer::draw_string(time_x, time_y, &time_str, COLOR_TEXT);

        // Check if we're in delete confirmation mode
        if crate::kernel::usb_hid::is_confirming_delete() {
            // Show delete confirmation prompt
            if let Some(filename) = crate::kernel::usb_hid::get_delete_confirm_filename() {
                let prompt_text = alloc::format!("Delete '{}'? (y/n)", filename);
                framebuffer::draw_string(MENU_START_X, MENU_START_Y + 4, &prompt_text, COLOR_TEXT);
            }
        } else if crate::kernel::usb_hid::is_prompting_filename() {
            // Show filename prompt instead of menu items
            let is_rename = crate::kernel::usb_hid::is_renaming();
            let prompt_label = if is_rename { "Rename to: " } else { "Enter filename: " };

            if let Some(filename) = crate::kernel::usb_hid::get_filename_prompt() {
                let prompt_text = alloc::format!("{}{}_", prompt_label, filename);
                framebuffer::draw_string(MENU_START_X, MENU_START_Y + 4, &prompt_text, COLOR_TEXT);
            } else {
                let prompt_text = alloc::format!("{}_", prompt_label);
                framebuffer::draw_string(MENU_START_X, MENU_START_Y + 4, &prompt_text, COLOR_TEXT);
            }
        } else if let Some(status_msg) = crate::kernel::usb_hid::get_menu_status() {
            // Show status message instead of menu items
            framebuffer::draw_string(MENU_START_X, MENU_START_Y + 4, &status_msg, COLOR_TEXT);
        } else {
            // Get cursor position for hover detection
            let (cursor_x, cursor_y) = framebuffer::get_cursor_pos();

            // Draw menu items with borders and backgrounds
            let mut current_x = MENU_START_X;
            for item in MENU_ITEMS.iter() {
                let item_width = Self::calculate_menu_item_width(item.label);
                let item_y = MENU_START_Y;

                // Check if cursor is hovering over this item
                let is_hovering = cursor_x >= current_x as i32 &&
                                  cursor_x < (current_x + item_width) as i32 &&
                                  cursor_y >= item_y as i32 &&
                                  cursor_y < (item_y + MENU_ITEM_HEIGHT) as i32;

                // Choose background color based on hover state
                let bg_color = if is_hovering {
                    COLOR_MENU_ITEM_HOVER
                } else {
                    COLOR_MENU_ITEM
                };

                // Draw menu item border
                self.draw_menu_rect(current_x, item_y, item_width, MENU_ITEM_HEIGHT, COLOR_MENU_ITEM_BORDER);

                // Draw menu item background (inset by 1 pixel for border)
                self.draw_menu_rect(current_x + 1, item_y + 1,
                                   item_width - 2, MENU_ITEM_HEIGHT - 2,
                                   bg_color);

                // Draw menu item text (centered with padding)
                let text_x = current_x + MENU_ITEM_PADDING_X;
                let text_y = item_y + 4;
                framebuffer::draw_string(text_x, text_y, item.label, COLOR_TEXT);

                // Move to next position
                current_x += item_width + MENU_ITEM_SPACING;
            }
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
                // Only prevent duplicates for AboutDialog
                if item.window_type == WindowContent::AboutDialog {
                    let already_exists = self.windows.iter()
                        .any(|w| w.content == WindowContent::AboutDialog);
                    if already_exists {
                        return None;
                    }
                }
                return Some(item.window_type);
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

        id
    }

    /// Remove a window by index
    pub fn remove_window(&mut self, index: usize) {
        if index < self.windows.len() {
            let window = &self.windows[index];

            // Delete the associated console/editor instance
            match window.content {
                WindowContent::Terminal => {
                    crate::kernel::shell::remove_shell(window.instance_id);
                    crate::kernel::console::remove_console(window.instance_id);
                },
                WindowContent::Editor => {
                    crate::kernel::editor::remove_editor(window.instance_id);
                },
                WindowContent::FileExplorer => {
                    crate::kernel::file_explorer::remove_file_explorer(window.instance_id);
                },
                WindowContent::Snake => {
                    crate::kernel::snake::remove_snake_game(window.instance_id);
                },
                WindowContent::AboutDialog => {
                    // No instance to remove
                },
            }

            self.windows.remove(index);
            // Recalculate tiling layout
            self.calculate_layout();
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

    /// Handle mouse down (button press)
    pub fn handle_mouse_down(&mut self, x: i32, y: i32) -> bool {
        // First check if menu bar was clicked
        if let Some(window_type) = self.check_menu_click(x, y) {
            // Create the requested window
            let (title, instance_id) = match window_type {
                WindowContent::Terminal => {
                    let id = crate::kernel::console::create_console();
                    // Initialize shell for this terminal
                    crate::kernel::shell::create_shell(id);
                    ("Terminal", id)
                },
                WindowContent::Editor => {
                    let id = crate::kernel::editor::create_editor();
                    ("Text Editor", id)
                },
                WindowContent::FileExplorer => {
                    let id = crate::kernel::file_explorer::create_file_explorer();
                    ("Files", id)
                },
                WindowContent::Snake => {
                    let id = crate::kernel::snake::create_snake_game();
                    ("Snake", id)
                },
                WindowContent::AboutDialog => {
                    ("About rOSt", 0) // AboutDialog doesn't need an instance
                },
            };
            let window = Window::new(0, 0, 640, 480, title, window_type, instance_id);
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

                // If it's an editor window and click is in content area, handle selection start
                if self.windows[i].content == WindowContent::Editor {
                    let (cx, cy, cw, ch) = self.windows[i].get_content_bounds();
                    if x >= cx && x < cx + cw as i32 && y >= cy && y < cy + ch as i32 {
                        // Click is inside editor content area - start selection
                        let relative_x = x - cx;
                        let relative_y = y - cy;
                        let instance_id = self.windows[i].instance_id;

                        if let Some(editor) = crate::kernel::editor::get_editor(instance_id) {
                            editor.handle_mouse_down(relative_x, relative_y);
                        }
                    }
                }

                // If it's a file explorer window and click is in content area, handle click
                if self.windows[i].content == WindowContent::FileExplorer {
                    let (cx, cy, cw, ch) = self.windows[i].get_content_bounds();
                    if x >= cx && x < cx + cw as i32 && y >= cy && y < cy + ch as i32 {
                        // Click is inside file explorer content area
                        let relative_x = x - cx;
                        let relative_y = y - cy;
                        let instance_id = self.windows[i].instance_id;
                        let current_time = crate::kernel::get_time_ms();

                        use crate::kernel::file_explorer::FileExplorerAction;
                        let action = crate::kernel::file_explorer::handle_click(
                            instance_id,
                            relative_x,
                            relative_y,
                            ch,
                            current_time
                        );

                        match action {
                            FileExplorerAction::OpenFile(filename) => {
                                // Open file in a new editor window
                                // Get filesystem from file explorer
                                if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(instance_id) {
                                    if let (Some(ref fs), Some(device_idx)) = (&explorer.filesystem, explorer.device_index) {
                                        // Get file info by listing all files
                                        let file_list = fs.list_files();
                                        let file_entry = file_list.iter().find(|e| e.get_name() == filename);

                                        if let Some(file) = file_entry {
                                            let size = file.get_size_bytes() as usize;
                                            let mut buffer = alloc::vec![0u8; size];

                                            unsafe {
                                                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                                                    if let Some(device) = devices.get_mut(device_idx) {
                                                        if let Ok(bytes_read) = fs.read_file(device, &filename, &mut buffer) {
                                                            // Find the actual content length
                                                            let actual_len = buffer[..bytes_read].iter()
                                                                .position(|&b| b == 0)
                                                                .unwrap_or(bytes_read);

                                                            if let Ok(text) = core::str::from_utf8(&buffer[..actual_len]) {
                                                                let editor_id = crate::kernel::editor::create_editor_with_content(
                                                                    &filename,
                                                                    text
                                                                );
                                                                let title = alloc::format!("Editor - {}", filename);
                                                                let window = Window::new(0, 0, 640, 480, &title, WindowContent::Editor, editor_id);
                                                                self.add_window(window);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            FileExplorerAction::Refresh => {
                                crate::kernel::file_explorer::refresh(instance_id);
                            },
                            FileExplorerAction::DeleteFile => {
                                // Get selected filename and start delete confirmation
                                if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(instance_id) {
                                    if let Some(filename) = explorer.get_selected_filename() {
                                        crate::kernel::usb_hid::start_delete_confirm(&filename);
                                    }
                                }
                            },
                            FileExplorerAction::NewFile => {
                                // Request filename from user via menu bar prompt
                                crate::kernel::usb_hid::start_filename_prompt();
                            },
                            FileExplorerAction::RenameFile => {
                                // Get selected filename and start rename prompt
                                if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(instance_id) {
                                    if let Some(filename) = explorer.get_selected_filename() {
                                        crate::kernel::usb_hid::start_rename_prompt(&filename);
                                    }
                                }
                            },
                            FileExplorerAction::Redraw => {
                                // Just need to redraw
                            },
                            FileExplorerAction::None => {},
                        }
                    }
                }

                // Focus this window
                self.focus_window(i);
                return true;
            }
        }
        false
    }

    /// Handle mouse drag (while button is held) - returns true if selection changed
    pub fn handle_mouse_drag(&mut self, x: i32, y: i32) -> bool {
        // Check if we're dragging in an editor window
        for i in (0..self.windows.len()).rev() {
            if self.windows[i].content == WindowContent::Editor && self.windows[i].is_focused {
                let (cx, cy, cw, ch) = self.windows[i].get_content_bounds();
                // Allow dragging even slightly outside bounds for smoother selection
                if x >= cx - 10 && x < cx + cw as i32 + 10 && y >= cy - 10 && y < cy + ch as i32 + 10 {
                    let relative_x = (x - cx).max(0).min(cw as i32 - 1);
                    let relative_y = (y - cy).max(0).min(ch as i32 - 1);
                    let instance_id = self.windows[i].instance_id;

                    if let Some(editor) = crate::kernel::editor::get_editor(instance_id) {
                        return editor.handle_mouse_drag(relative_x, relative_y);
                    }
                    return false;
                }
            }
        }
        false
    }

    /// Handle mouse up (button release)
    pub fn handle_mouse_up(&mut self, _x: i32, _y: i32) {
        // End selection in all editors
        for window in &self.windows {
            if window.content == WindowContent::Editor {
                if let Some(editor) = crate::kernel::editor::get_editor(window.instance_id) {
                    editor.handle_mouse_up();
                }
            }
        }
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

    /// Get the focused terminal window instance ID
    pub fn get_focused_terminal_id(&self) -> Option<usize> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Terminal && w.is_focused)
            .last()
            .map(|w| w.instance_id)
    }

    /// Get all terminal windows with their instance IDs and content bounds
    pub fn get_all_terminals(&self) -> Vec<(usize, i32, i32, u32, u32)> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Terminal && w.visible)
            .map(|w| {
                let (x, y, width, height) = w.get_content_bounds();
                (w.instance_id, x, y, width, height)
            })
            .collect()
    }

    /// Get the focused editor window instance ID
    pub fn get_focused_editor_id(&self) -> Option<usize> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Editor && w.is_focused)
            .last()
            .map(|w| w.instance_id)
    }

    /// Get all editor windows with their instance IDs and content bounds
    pub fn get_all_editors(&self) -> Vec<(usize, i32, i32, u32, u32)> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Editor && w.visible)
            .map(|w| {
                let (x, y, width, height) = w.get_content_bounds();
                (w.instance_id, x, y, width, height)
            })
            .collect()
    }

    /// Update the editor window title
    pub fn set_editor_title(&mut self, title: &str) {
        if let Some(window) = self.windows.iter_mut()
            .filter(|w| w.content == WindowContent::Editor)
            .last() {
            window.title = String::from(title);
        }
    }

    /// Get the focused file explorer window instance ID
    pub fn get_focused_file_explorer_id(&self) -> Option<usize> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::FileExplorer && w.is_focused)
            .last()
            .map(|w| w.instance_id)
    }

    /// Get all file explorer windows with their instance IDs and content bounds
    pub fn get_all_file_explorers(&self) -> Vec<(usize, i32, i32, u32, u32)> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::FileExplorer && w.visible)
            .map(|w| {
                let (x, y, width, height) = w.get_content_bounds();
                (w.instance_id, x, y, width, height)
            })
            .collect()
    }

    /// Get the focused snake game window instance ID
    pub fn get_focused_snake_id(&self) -> Option<usize> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Snake && w.is_focused)
            .last()
            .map(|w| w.instance_id)
    }

    /// Get all snake game windows with their instance IDs and content bounds
    pub fn get_all_snakes(&self) -> Vec<(usize, i32, i32, u32, u32)> {
        self.windows.iter()
            .filter(|w| w.content == WindowContent::Snake && w.visible)
            .map(|w| {
                let (x, y, width, height) = w.get_content_bounds();
                (w.instance_id, x, y, width, height)
            })
            .collect()
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

pub fn handle_mouse_down(x: i32, y: i32) -> bool {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_mouse_down(x, y)
        } else {
            false
        }
    }
}

pub fn handle_mouse_drag(x: i32, y: i32) -> bool {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_mouse_drag(x, y)
        } else {
            false
        }
    }
}

pub fn handle_mouse_up(x: i32, y: i32) {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.handle_mouse_up(x, y);
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
            wm.get_focused_terminal_id().is_some()
        } else {
            false
        }
    }
}

pub fn get_focused_terminal_id() -> Option<usize> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_terminal_id()
        } else {
            None
        }
    }
}

pub fn get_all_terminals() -> Vec<(usize, i32, i32, u32, u32)> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_all_terminals()
        } else {
            Vec::new()
        }
    }
}

pub fn has_focused_editor() -> bool {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_editor_id().is_some()
        } else {
            false
        }
    }
}

pub fn get_focused_editor_id() -> Option<usize> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_editor_id()
        } else {
            None
        }
    }
}

pub fn get_all_editors() -> Vec<(usize, i32, i32, u32, u32)> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_all_editors()
        } else {
            Vec::new()
        }
    }
}

pub fn set_editor_window_title(title: &str) {
    unsafe {
        if let Some(ref mut wm) = WINDOW_MANAGER {
            wm.set_editor_title(title);
        }
    }
}

pub fn has_focused_file_explorer() -> bool {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_file_explorer_id().is_some()
        } else {
            false
        }
    }
}

pub fn get_focused_file_explorer_id() -> Option<usize> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_file_explorer_id()
        } else {
            None
        }
    }
}

pub fn get_all_file_explorers() -> Vec<(usize, i32, i32, u32, u32)> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_all_file_explorers()
        } else {
            Vec::new()
        }
    }
}

/// Get which menu button is being hovered (returns button index or None)
pub fn get_hovered_menu_button(x: i32, y: i32) -> Option<usize> {
    // Check if in menu bar area
    if y < 0 || y >= MENU_BAR_HEIGHT as i32 {
        return None;
    }

    let mut current_x = MENU_START_X;
    for (index, item) in MENU_ITEMS.iter().enumerate() {
        let item_width = WindowManager::calculate_menu_item_width(item.label);
        let item_y = MENU_START_Y;

        if x >= current_x as i32 &&
           x < (current_x + item_width) as i32 &&
           y >= item_y as i32 &&
           y < (item_y + MENU_ITEM_HEIGHT) as i32 {
            return Some(index);
        }

        current_x += item_width + MENU_ITEM_SPACING;
    }

    None
}

pub fn has_focused_snake() -> bool {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_snake_id().is_some()
        } else {
            false
        }
    }
}

pub fn get_focused_snake_id() -> Option<usize> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_focused_snake_id()
        } else {
            None
        }
    }
}

pub fn get_all_snakes() -> Vec<(usize, i32, i32, u32, u32)> {
    unsafe {
        if let Some(ref wm) = WINDOW_MANAGER {
            wm.get_all_snakes()
        } else {
            Vec::new()
        }
    }
}
