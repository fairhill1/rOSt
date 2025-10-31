// Simple text editor for rOSt

use crate::kernel::framebuffer;
extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::vec;

const EDITOR_WIDTH: usize = 1024;  // Characters per line (allow longer lines)
const EDITOR_HEIGHT: usize = 35; // Lines visible
const CHAR_WIDTH: u32 = 16;
const CHAR_HEIGHT: u32 = 16;
const LINE_SPACING: u32 = 4;
const LINE_HEIGHT: u32 = CHAR_HEIGHT + LINE_SPACING;

// Colors
const COLOR_TEXT: u32 = 0xFFFFFFFF;        // White text
const COLOR_CURSOR: u32 = 0xFF00FF00;      // Green cursor
const COLOR_STATUS: u32 = 0xFFCCCCCC;      // Light gray for status bar

pub struct TextEditor {
    /// Lines of text in the editor
    lines: Vec<String>,
    /// Current cursor position (row, column)
    cursor_row: usize,
    cursor_col: usize,
    /// Filename being edited (None for new file)
    filename: Option<String>,
    /// Whether the file has been modified
    modified: bool,
    /// Scroll offset (top visible line)
    scroll_offset: usize,
    /// Status message
    status: String,
}

impl TextEditor {
    pub fn new() -> Self {
        TextEditor {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            filename: None,
            modified: false,
            scroll_offset: 0,
            status: String::from("NEW FILE - Ctrl+S: Save, Ctrl+Q: Quit"),
        }
    }

    /// Create editor with existing file content
    pub fn with_content(filename: &str, content: &str) -> Self {
        let lines: Vec<String> = content
            .lines()
            .map(|s| String::from(s))
            .collect();

        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };

        TextEditor {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            filename: Some(String::from(filename)),
            modified: false,
            scroll_offset: 0,
            status: String::from("Editing - Ctrl+S: Save, Ctrl+Q: Quit"),
        }
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, ch: char) {
        if self.cursor_row >= self.lines.len() {
            self.lines.push(String::new());
        }

        // Check line length limit
        if self.lines[self.cursor_row].len() >= EDITOR_WIDTH {
            self.set_status("Line too long!");
            return;
        }

        self.lines[self.cursor_row].insert(self.cursor_col, ch);
        self.cursor_col += 1;
        self.modified = true;
    }

    /// Insert a newline at the cursor position
    pub fn insert_newline(&mut self) {
        if self.cursor_row >= self.lines.len() {
            self.lines.push(String::new());
            self.cursor_row = self.lines.len() - 1;
            self.cursor_col = 0;
            self.modified = true;
            return;
        }

        // Split the current line at cursor
        let current_line = &self.lines[self.cursor_row];
        let before = current_line[..self.cursor_col].to_string();
        let after = current_line[self.cursor_col..].to_string();

        self.lines[self.cursor_row] = before;
        self.lines.insert(self.cursor_row + 1, after);

        self.cursor_row += 1;
        self.cursor_col = 0;
        self.modified = true;

        // Auto-scroll if cursor moved below visible area
        if self.cursor_row >= self.scroll_offset + EDITOR_HEIGHT {
            self.scroll_offset = self.cursor_row - EDITOR_HEIGHT + 1;
        }
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        if self.cursor_col > 0 {
            // Delete character in current line
            self.lines[self.cursor_row].remove(self.cursor_col - 1);
            self.cursor_col -= 1;
            self.modified = true;
        } else if self.cursor_row > 0 {
            // Join with previous line
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&current_line);
            self.modified = true;

            // Auto-scroll if needed
            if self.cursor_row < self.scroll_offset {
                self.scroll_offset = self.cursor_row;
            }
        }
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor_row].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }

            // Auto-scroll if needed
            if self.cursor_row < self.scroll_offset {
                self.scroll_offset = self.cursor_row;
            }
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_row < self.lines.len() - 1 {
            self.cursor_row += 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor_row].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + EDITOR_HEIGHT {
                self.scroll_offset = self.cursor_row - EDITOR_HEIGHT + 1;
            }
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            // Move to end of previous line
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();

            // Auto-scroll if needed
            if self.cursor_row < self.scroll_offset {
                self.scroll_offset = self.cursor_row;
            }
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor_col < self.lines[self.cursor_row].len() {
            self.cursor_col += 1;
        } else if self.cursor_row < self.lines.len() - 1 {
            // Move to start of next line
            self.cursor_row += 1;
            self.cursor_col = 0;

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + EDITOR_HEIGHT {
                self.scroll_offset = self.cursor_row - EDITOR_HEIGHT + 1;
            }
        }
    }

    /// Get all text as a single string
    pub fn get_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Set status message
    pub fn set_status(&mut self, msg: &str) {
        self.status = String::from(msg);
    }

    /// Get filename
    pub fn get_filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }

    /// Set filename
    pub fn set_filename(&mut self, name: &str) {
        self.filename = Some(String::from(name));
    }

    /// Check if modified
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Mark as saved
    pub fn mark_saved(&mut self) {
        self.modified = false;
    }

    /// Get byte size of content
    pub fn get_content_size(&self) -> usize {
        self.get_text().len()
    }

    /// Render the editor at a specific offset (for window rendering)
    pub fn render_at(&self, offset_x: i32, offset_y: i32) {
        // Draw visible lines
        let visible_end = (self.scroll_offset + EDITOR_HEIGHT).min(self.lines.len());

        for (idx, line_num) in (self.scroll_offset..visible_end).enumerate() {
            let line = &self.lines[line_num];
            let y = offset_y + (idx as i32 * LINE_HEIGHT as i32);

            // Draw each character
            for (col, ch) in line.chars().enumerate() {
                if col >= EDITOR_WIDTH {
                    break; // Don't draw beyond editor width
                }
                let x = offset_x + (col as i32 * CHAR_WIDTH as i32);

                // Draw character
                let mut buf = [0u8; 4];
                let s = ch.encode_utf8(&mut buf);
                framebuffer::draw_string(x as u32, y as u32, s, COLOR_TEXT);
            }
        }

        // Draw cursor (only if visible in current scroll view)
        if self.cursor_row >= self.scroll_offset &&
           self.cursor_row < self.scroll_offset + EDITOR_HEIGHT {
            let visible_row = self.cursor_row - self.scroll_offset;
            let cursor_x = offset_x + (self.cursor_col as i32 * CHAR_WIDTH as i32);
            let cursor_y = offset_y + (visible_row as i32 * LINE_HEIGHT as i32);

            // Draw cursor as a vertical bar
            for dy in 0..CHAR_HEIGHT {
                for dx in 0..2 {
                    let px = cursor_x + dx as i32;
                    let py = cursor_y + dy as i32;
                    if px >= 0 && py >= 0 {
                        framebuffer::draw_pixel(px as u32, py as u32, COLOR_CURSOR);
                    }
                }
            }
        }
    }
}

/// Global editor instances
static mut EDITORS: Vec<TextEditor> = Vec::new();

pub fn init() {
    // Nothing to do - editors are created on demand
}

/// Create a new editor instance and return its ID
pub fn create_editor() -> usize {
    unsafe {
        EDITORS.push(TextEditor::new());
        EDITORS.len() - 1
    }
}

/// Create a new editor with content and return its ID
pub fn create_editor_with_content(filename: &str, content: &str) -> usize {
    unsafe {
        EDITORS.push(TextEditor::with_content(filename, content));
        EDITORS.len() - 1
    }
}

/// Remove an editor instance by ID
pub fn remove_editor(id: usize) {
    unsafe {
        if id < EDITORS.len() {
            EDITORS.remove(id);
        }
    }
}

/// Get an editor instance by ID
pub fn get_editor(id: usize) -> Option<&'static mut TextEditor> {
    unsafe {
        EDITORS.get_mut(id)
    }
}

pub fn render_at(id: usize, offset_x: i32, offset_y: i32) {
    if let Some(editor) = get_editor(id) {
        editor.render_at(offset_x, offset_y);
    }
}
