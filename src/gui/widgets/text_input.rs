// Reusable single-line text input widget for rOSt

use crate::gui::framebuffer;
extern crate alloc;
use alloc::string::String;

const CHAR_WIDTH: u32 = 16;
const CHAR_HEIGHT: u32 = 16;

// Colors
const COLOR_TEXT: u32 = 0xFF000000;          // Black text
const COLOR_CURSOR: u32 = 0xFF00FF00;        // Green cursor
const COLOR_SELECTION: u32 = 0xFF3366CC;     // Bright blue selection highlight
const COLOR_BG: u32 = 0xFFFFFFFF;            // White background
const COLOR_BORDER: u32 = 0xFFCCCCCC;        // Gray border
const COLOR_BORDER_FOCUSED: u32 = 0xFF4A90E2; // Blue border when focused

/// Single-line text input widget
pub struct TextInput {
    /// Text content
    text: String,
    /// Cursor position (character index)
    cursor_pos: usize,
    /// Selection start (character index) - None if no selection
    selection_start: Option<usize>,
    /// Selection end (character index) - None if no selection
    selection_end: Option<usize>,
    /// Whether we're currently selecting (mouse drag)
    is_selecting: bool,
    /// Maximum character length (0 = unlimited)
    max_length: usize,
    /// Last click timestamp (for double/triple-click detection)
    last_click_time: u64,
    /// Number of consecutive clicks
    click_count: u32,
}

impl TextInput {
    /// Create a new empty text input
    pub fn new() -> Self {
        TextInput {
            text: String::new(),
            cursor_pos: 0,
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            max_length: 0,
            last_click_time: 0,
            click_count: 0,
        }
    }

    /// Create text input with initial text
    pub fn with_text(text: &str) -> Self {
        let len = text.chars().count();
        TextInput {
            text: String::from(text),
            cursor_pos: len,
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            max_length: 0,
            last_click_time: 0,
            click_count: 0,
        }
    }

    /// Set maximum character length
    pub fn set_max_length(&mut self, max_length: usize) {
        self.max_length = max_length;
    }

    /// Get the text content
    pub fn get_text(&self) -> &str {
        &self.text
    }

    /// Set the text content and move cursor to end
    pub fn set_text(&mut self, text: &str) {
        self.text = String::from(text);
        self.cursor_pos = self.text.chars().count();
        self.clear_selection();
    }

    /// Clear all text
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
        self.clear_selection();
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.is_selecting = false;
    }

    /// Delete selected text and return true if something was deleted
    fn delete_selection(&mut self) -> bool {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            // Normalize selection
            let (start, end) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };

            if start == end {
                // Empty selection
                self.clear_selection();
                return false;
            }

            // Convert to byte indices for removal
            let chars: alloc::vec::Vec<char> = self.text.chars().collect();
            let before: String = chars[..start].iter().collect();
            let after: String = chars[end..].iter().collect();
            self.text = before + &after;

            // Move cursor to start of selection
            self.cursor_pos = start;
            self.clear_selection();
            true
        } else {
            false
        }
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, ch: char) {
        // Delete selection first if there is one
        self.delete_selection();

        // Check max length
        if self.max_length > 0 && self.text.chars().count() >= self.max_length {
            return;
        }

        // Convert to character vector for easy insertion
        let mut chars: alloc::vec::Vec<char> = self.text.chars().collect();
        chars.insert(self.cursor_pos, ch);
        self.text = chars.iter().collect();
        self.cursor_pos += 1;
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        // If there's a selection, delete it instead
        if self.delete_selection() {
            return;
        }

        if self.cursor_pos > 0 {
            let mut chars: alloc::vec::Vec<char> = self.text.chars().collect();
            chars.remove(self.cursor_pos - 1);
            self.text = chars.iter().collect();
            self.cursor_pos -= 1;
        }
    }

    /// Delete character after cursor (delete key)
    pub fn delete(&mut self) {
        // If there's a selection, delete it instead
        if self.delete_selection() {
            return;
        }

        let len = self.text.chars().count();
        if self.cursor_pos < len {
            let mut chars: alloc::vec::Vec<char> = self.text.chars().collect();
            chars.remove(self.cursor_pos);
            self.text = chars.iter().collect();
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        self.clear_selection();
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        self.clear_selection();
        let len = self.text.chars().count();
        if self.cursor_pos < len {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to start
    pub fn move_home(&mut self) {
        self.clear_selection();
        self.cursor_pos = 0;
    }

    /// Move cursor to end
    pub fn move_end(&mut self) {
        self.clear_selection();
        self.cursor_pos = self.text.chars().count();
    }

    /// Move cursor left with selection (Shift+Left)
    pub fn move_left_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }

        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }

        // Update selection end
        self.selection_end = Some(self.cursor_pos);
    }

    /// Move cursor right with selection (Shift+Right)
    pub fn move_right_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some(self.cursor_pos);
        }

        let len = self.text.chars().count();
        if self.cursor_pos < len {
            self.cursor_pos += 1;
        }

        // Update selection end
        self.selection_end = Some(self.cursor_pos);
    }

    /// Select all text
    pub fn select_all(&mut self) {
        let len = self.text.chars().count();
        if len > 0 {
            self.selection_start = Some(0);
            self.selection_end = Some(len);
            self.cursor_pos = len;
        }
    }

    /// Check if a character is a word boundary (for double-click word selection)
    fn is_word_boundary(ch: char) -> bool {
        ch.is_whitespace() || matches!(ch, '/' | ':' | '?' | '&' | '=' | '.' | '#' | '@' | '-' | '_')
    }

    /// Select word at cursor position (for double-click)
    pub fn select_word_at_cursor(&mut self) {
        if self.text.is_empty() {
            return;
        }

        let chars: alloc::vec::Vec<char> = self.text.chars().collect();
        let len = chars.len();

        // If cursor is at the end, move it back to last character
        let cursor_pos = if self.cursor_pos >= len && len > 0 {
            len - 1
        } else {
            self.cursor_pos
        };

        if cursor_pos >= len {
            return;
        }

        // Find word start (go backwards from cursor)
        let mut start = cursor_pos;
        while start > 0 && !Self::is_word_boundary(chars[start - 1]) {
            start -= 1;
        }

        // Find word end (go forwards from cursor)
        let mut end = cursor_pos;
        while end < len && !Self::is_word_boundary(chars[end]) {
            end += 1;
        }

        // Select the word
        if start < end {
            self.selection_start = Some(start);
            self.selection_end = Some(end);
            self.cursor_pos = end;
        }
    }

    /// Get selected text (if any)
    fn get_selected_text(&self) -> Option<String> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            // Normalize selection
            let (start, end) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };

            if start == end {
                return None; // Empty selection
            }

            let chars: alloc::vec::Vec<char> = self.text.chars().collect();
            Some(chars[start..end].iter().collect())
        } else {
            None
        }
    }

    /// Copy selected text to clipboard
    pub fn copy(&self) {
        if let Some(text) = self.get_selected_text() {
            crate::gui::clipboard::copy(text);
        }
    }

    /// Cut selected text to clipboard
    pub fn cut(&mut self) {
        if let Some(text) = self.get_selected_text() {
            crate::gui::clipboard::copy(text);
            self.delete_selection();
        }
    }

    /// Paste text from clipboard
    pub fn paste(&mut self) {
        if let Some(text) = crate::gui::clipboard::paste() {
            // Delete selection first if there is one
            self.delete_selection();

            // Insert the text character by character
            for ch in text.chars() {
                // Skip newlines (single-line input)
                if ch == '\n' || ch == '\r' {
                    continue;
                }

                // Check max length
                if self.max_length > 0 && self.text.chars().count() >= self.max_length {
                    break;
                }

                self.insert_char(ch);
            }
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char, ctrl: bool, shift: bool) {
        if ctrl {
            match key {
                'a' => self.select_all(),
                'c' => self.copy(),
                'x' => self.cut(),
                'v' => self.paste(),
                _ => {}
            }
        } else if key == '\x08' {
            // Backspace
            self.backspace();
        } else if key == '\x7F' {
            // Delete (DEL key)
            self.delete();
        } else if key == '\x1B' {
            // Arrow keys are handled separately via handle_arrow_key
        } else if key == '\n' || key == '\r' {
            // Enter key - caller should handle this
        } else if key.is_ascii() && !key.is_control() {
            self.insert_char(key);
        }
    }

    /// Handle arrow key (separate from handle_key for cleaner interface)
    pub fn handle_arrow_key(&mut self, arrow: ArrowKey, shift: bool) {
        match arrow {
            ArrowKey::Left => {
                if shift {
                    self.move_left_select();
                } else {
                    self.move_left();
                }
            }
            ArrowKey::Right => {
                if shift {
                    self.move_right_select();
                } else {
                    self.move_right();
                }
            }
            ArrowKey::Home => {
                self.move_home();
            }
            ArrowKey::End => {
                self.move_end();
            }
            _ => {}
        }
    }

    /// Handle mouse down - start selection, detect double/triple clicks
    pub fn handle_mouse_down(&mut self, click_x: i32, base_x: i32) {
        // Get current time for double/triple-click detection
        let current_time = crate::kernel::drivers::timer::get_time_ms();
        const MULTI_CLICK_THRESHOLD_MS: u64 = 500;

        // Calculate relative position
        let rel_x = (click_x - base_x).max(0);

        // Convert to character position
        let char_pos = ((rel_x + CHAR_WIDTH as i32 / 2) / CHAR_WIDTH as i32).max(0) as usize;

        // Clamp to text length
        let len = self.text.chars().count();
        let new_cursor_pos = char_pos.min(len);

        // Check if this is a multi-click (double or triple)
        let time_diff = current_time.saturating_sub(self.last_click_time);

        if time_diff <= MULTI_CLICK_THRESHOLD_MS {
            // Multi-click detected
            self.click_count += 1;
        } else {
            // Too long between clicks - reset to single click
            self.click_count = 1;
        }

        self.last_click_time = current_time;
        self.cursor_pos = new_cursor_pos;

        match self.click_count {
            1 => {
                // Single click - position cursor and start selection
                self.selection_start = Some(self.cursor_pos);
                self.selection_end = Some(self.cursor_pos);
                self.is_selecting = true;
            }
            2 => {
                // Double click - select word at cursor
                self.select_word_at_cursor();
                self.is_selecting = false; // Don't allow dragging after double-click
            }
            _ => {
                // Triple click (or more) - select all
                self.select_all();
                self.is_selecting = false; // Don't allow dragging after triple-click
                // Reset click count to prevent overflow
                self.click_count = 3;
            }
        }
    }

    /// Handle mouse drag - update selection
    pub fn handle_mouse_drag(&mut self, click_x: i32, base_x: i32) -> bool {
        if !self.is_selecting {
            return false;
        }

        // Calculate relative position
        let rel_x = (click_x - base_x).max(0);

        // Convert to character position
        let char_pos = ((rel_x + CHAR_WIDTH as i32 / 2) / CHAR_WIDTH as i32).max(0) as usize;

        // Clamp to text length
        let len = self.text.chars().count();
        let clamped_pos = char_pos.min(len);

        // Check if position changed
        if self.selection_end == Some(clamped_pos) {
            return false; // No change
        }

        // Update selection end and cursor
        self.selection_end = Some(clamped_pos);
        self.cursor_pos = clamped_pos;
        true
    }

    /// Handle mouse up - end selection
    pub fn handle_mouse_up(&mut self) {
        self.is_selecting = false;

        // If selection is empty (start == end), clear it
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            if start == end {
                self.selection_start = None;
                self.selection_end = None;
            }
        }
    }

    /// Render the text input at a specific position
    pub fn render_at(&self, x: i32, y: i32, width: u32, height: u32, focused: bool) {
        // Draw background
        for dy in 0..height {
            for dx in 0..width {
                let px = x + dx as i32;
                let py = y + dy as i32;
                if px >= 0 && py >= 0 {
                    framebuffer::draw_pixel(px as u32, py as u32, COLOR_BG);
                }
            }
        }

        // Draw border
        let border_color = if focused { COLOR_BORDER_FOCUSED } else { COLOR_BORDER };
        // Top and bottom borders
        for dx in 0..width {
            let px = x + dx as i32;
            // Top
            if px >= 0 && y >= 0 {
                framebuffer::draw_pixel(px as u32, y as u32, border_color);
            }
            // Bottom
            let py_bottom = y + height as i32 - 1;
            if px >= 0 && py_bottom >= 0 {
                framebuffer::draw_pixel(px as u32, py_bottom as u32, border_color);
            }
        }
        // Left and right borders
        for dy in 0..height {
            let py = y + dy as i32;
            // Left
            if x >= 0 && py >= 0 {
                framebuffer::draw_pixel(x as u32, py as u32, border_color);
            }
            // Right
            let px_right = x + width as i32 - 1;
            if px_right >= 0 && py >= 0 {
                framebuffer::draw_pixel(px_right as u32, py as u32, border_color);
            }
        }

        // Calculate text rendering area (with padding)
        let text_x = x + 4;
        let text_y = y + ((height as i32 - CHAR_HEIGHT as i32) / 2).max(0);

        // Normalize selection for rendering
        let selection = if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            if start <= end {
                Some((start, end))
            } else {
                Some((end, start))
            }
        } else {
            None
        };

        // Draw selection highlight
        if let Some((start, end)) = selection {
            if start < end {
                let sel_x = text_x + (start as i32 * CHAR_WIDTH as i32);
                let sel_width = (end - start) as u32 * CHAR_WIDTH;

                for dy in 0..CHAR_HEIGHT {
                    for dx in 0..sel_width {
                        let px = sel_x + dx as i32;
                        let py = text_y + dy as i32;
                        if px >= 0 && py >= 0 {
                            framebuffer::draw_pixel(px as u32, py as u32, COLOR_SELECTION);
                        }
                    }
                }
            }
        }

        // Draw text
        for (idx, ch) in self.text.chars().enumerate() {
            let char_x = text_x + (idx as i32 * CHAR_WIDTH as i32);

            // Only draw if visible
            if char_x + CHAR_WIDTH as i32 > x + width as i32 {
                break;
            }

            // Draw character
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            framebuffer::draw_string(char_x as u32, text_y as u32, s, COLOR_TEXT);
        }

        // Draw cursor (if focused)
        if focused {
            let cursor_x = text_x + (self.cursor_pos as i32 * CHAR_WIDTH as i32);

            // Draw cursor as a vertical bar
            for dy in 0..CHAR_HEIGHT {
                for dx in 0..2 {
                    let px = cursor_x + dx as i32;
                    let py = text_y + dy as i32;
                    if px >= 0 && py >= 0 && px < x + width as i32 {
                        framebuffer::draw_pixel(px as u32, py as u32, COLOR_CURSOR);
                    }
                }
            }
        }
    }
}

/// Arrow key enum for cleaner API
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowKey {
    Left,
    Right,
    Up,    // Not used for single-line, but included for completeness
    Down,  // Not used for single-line, but included for completeness
    Home,
    End,
}
