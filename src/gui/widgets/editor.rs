// Simple text editor for rOSt

use crate::gui::framebuffer;
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
const GUTTER_WIDTH: u32 = 48;  // Width of line number gutter background
const GUTTER_SPACING: u32 = 8;  // Space after gutter before text starts

// Colors
const COLOR_TEXT: u32 = 0xFFFFFFFF;        // White text
const COLOR_CURSOR: u32 = 0xFF00FF00;      // Green cursor
const COLOR_STATUS: u32 = 0xFFCCCCCC;      // Light gray for status bar
const COLOR_SELECTION: u32 = 0xFF3366CC;   // Bright blue selection highlight
const COLOR_GUTTER_BG: u32 = 0xFF2A2A2A;   // Dark gray gutter background
const COLOR_LINE_NUMBER: u32 = 0xFF888888; // Gray line numbers

/// Snapshot of editor state for undo
#[derive(Clone)]
struct EditorSnapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_offset: usize,
}

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
    /// Selection start (row, col) - None if no selection
    selection_start: Option<(usize, usize)>,
    /// Selection end (row, col) - None if no selection
    selection_end: Option<(usize, usize)>,
    /// Whether we're currently selecting (mouse drag)
    is_selecting: bool,
    /// Undo stack (max 100 snapshots)
    undo_stack: Vec<EditorSnapshot>,
    /// Redo stack (max 100 snapshots)
    redo_stack: Vec<EditorSnapshot>,
    /// Actual visible height in lines (updated during rendering)
    visible_height: usize,
}

impl TextEditor {
    /// Calculate pixel offset for a given column in a line (for variable-width fonts)
    fn col_to_pixel_offset(&self, line_idx: usize, col: usize) -> i32 {
        if line_idx >= self.lines.len() {
            return 0;
        }

        let line = &self.lines[line_idx];
        let substring: String = line.chars().take(col).collect();
        crate::gui::framebuffer::measure_string(&substring) as i32
    }

    /// Convert pixel position to column (for variable-width fonts)
    /// Must match col_to_pixel_offset's measurement approach
    fn pixel_to_col(&self, line_idx: usize, pixel_x: i32) -> usize {
        if line_idx >= self.lines.len() {
            return 0;
        }

        let line = &self.lines[line_idx];
        let char_count = line.chars().count();

        // Binary search would be faster, but linear search is simple and works
        for col in 0..=char_count {
            let width_at_col = self.col_to_pixel_offset(line_idx, col);

            // If we haven't reached this position yet, check if click is closer to prev or current
            if pixel_x < width_at_col {
                if col == 0 {
                    return 0;
                }

                // Check if closer to previous column
                let prev_width = self.col_to_pixel_offset(line_idx, col - 1);
                let dist_to_prev = pixel_x - prev_width;
                let dist_to_current = width_at_col - pixel_x;

                if dist_to_prev < dist_to_current {
                    return col - 1;
                } else {
                    return col;
                }
            }
        }

        char_count // Clicked past end
    }

    pub fn new() -> Self {
        TextEditor {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            filename: None,
            modified: false,
            scroll_offset: 0,
            status: String::from("NEW FILE - Ctrl+S: Save, Ctrl+Q: Quit"),
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            visible_height: EDITOR_HEIGHT, // Default to full height
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
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            visible_height: EDITOR_HEIGHT, // Default to full height
        }
    }

    /// Save current state to undo stack
    fn save_snapshot(&mut self) {
        const MAX_UNDO_STACK: usize = 100;

        let snapshot = EditorSnapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            scroll_offset: self.scroll_offset,
        };

        self.undo_stack.push(snapshot);

        // Limit stack size
        if self.undo_stack.len() > MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }

        // Clear redo stack on new edit (standard undo/redo behavior)
        self.redo_stack.clear();
    }

    /// Undo last edit
    pub fn undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            // Save current state to redo stack before undoing
            let current = EditorSnapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
                scroll_offset: self.scroll_offset,
            };
            self.redo_stack.push(current);

            // Restore previous state
            self.lines = snapshot.lines;
            self.cursor_row = snapshot.cursor_row;
            self.cursor_col = snapshot.cursor_col;
            self.scroll_offset = snapshot.scroll_offset;
            self.clear_selection();
            self.set_status("Undo");
        } else {
            self.set_status("Nothing to undo");
        }
    }

    /// Redo last undone edit
    pub fn redo(&mut self) {
        const MAX_REDO_STACK: usize = 100;

        if let Some(snapshot) = self.redo_stack.pop() {
            // Save current state to undo stack before redoing
            let current = EditorSnapshot {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
                scroll_offset: self.scroll_offset,
            };
            self.undo_stack.push(current);

            // Limit undo stack size
            if self.undo_stack.len() > MAX_REDO_STACK {
                self.undo_stack.remove(0);
            }

            // Restore redo state
            self.lines = snapshot.lines;
            self.cursor_row = snapshot.cursor_row;
            self.cursor_col = snapshot.cursor_col;
            self.scroll_offset = snapshot.scroll_offset;
            self.clear_selection();
            self.set_status("Redo");
        } else {
            self.set_status("Nothing to redo");
        }
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

            let (start_row, start_col) = start;
            let (end_row, end_col) = end;

            if start_row == end_row {
                // Single line selection
                self.lines[start_row].replace_range(start_col..end_col, "");
            } else {
                // Multi-line selection
                // Keep the text before selection start and after selection end
                let before = self.lines[start_row][..start_col].to_string();
                let after = self.lines[end_row][end_col..].to_string();

                // Remove all lines in between
                for _ in start_row..=end_row {
                    self.lines.remove(start_row);
                }

                // Insert the combined line
                self.lines.insert(start_row, before + &after);
            }

            // Move cursor to start of selection
            self.cursor_row = start_row;
            self.cursor_col = start_col;
            self.clear_selection();
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, ch: char) {
        // Save state before modification
        self.save_snapshot();

        // Delete selection first if there is one
        self.delete_selection();

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
        // Save state before modification
        self.save_snapshot();

        // Delete selection first if there is one
        self.delete_selection();

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
        if self.cursor_row >= self.scroll_offset + self.visible_height {
            self.scroll_offset = self.cursor_row - self.visible_height + 1;
        }
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        // Save state before modification
        self.save_snapshot();

        // If there's a selection, delete it instead
        if self.delete_selection() {
            return;
        }

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
        self.clear_selection(); // Clear selection on cursor movement
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
        self.clear_selection(); // Clear selection on cursor movement
        if self.cursor_row < self.lines.len() - 1 {
            self.cursor_row += 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor_row].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + self.visible_height {
                self.scroll_offset = self.cursor_row - self.visible_height + 1;
            }
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        self.clear_selection(); // Clear selection on cursor movement
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
        self.clear_selection(); // Clear selection on cursor movement
        if self.cursor_col < self.lines[self.cursor_row].len() {
            self.cursor_col += 1;
        } else if self.cursor_row < self.lines.len() - 1 {
            // Move to start of next line
            self.cursor_row += 1;
            self.cursor_col = 0;

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + self.visible_height {
                self.scroll_offset = self.cursor_row - self.visible_height + 1;
            }
        }
    }

    /// Move cursor up with selection (Shift+Up)
    pub fn move_up_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some((self.cursor_row, self.cursor_col));
        }

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

        // Update selection end
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    /// Move cursor down with selection (Shift+Down)
    pub fn move_down_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some((self.cursor_row, self.cursor_col));
        }

        if self.cursor_row < self.lines.len() - 1 {
            self.cursor_row += 1;
            // Clamp column to line length
            let line_len = self.lines[self.cursor_row].len();
            if self.cursor_col > line_len {
                self.cursor_col = line_len;
            }

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + self.visible_height {
                self.scroll_offset = self.cursor_row - self.visible_height + 1;
            }
        }

        // Update selection end
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    /// Move cursor left with selection (Shift+Left)
    pub fn move_left_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some((self.cursor_row, self.cursor_col));
        }

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

        // Update selection end
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    /// Move cursor right with selection (Shift+Right)
    pub fn move_right_select(&mut self) {
        // Start selection if not already selecting
        if self.selection_start.is_none() {
            self.selection_start = Some((self.cursor_row, self.cursor_col));
        }

        if self.cursor_col < self.lines[self.cursor_row].len() {
            self.cursor_col += 1;
        } else if self.cursor_row < self.lines.len() - 1 {
            // Move to start of next line
            self.cursor_row += 1;
            self.cursor_col = 0;

            // Auto-scroll if needed
            if self.cursor_row >= self.scroll_offset + self.visible_height {
                self.scroll_offset = self.cursor_row - self.visible_height + 1;
            }
        }

        // Update selection end
        self.selection_end = Some((self.cursor_row, self.cursor_col));
    }

    /// Scroll the editor by a number of lines (positive = down, negative = up)
    pub fn scroll(&mut self, lines: i32) {
        if lines > 0 {
            // Scroll down
            let max_scroll = self.lines.len().saturating_sub(self.visible_height);
            self.scroll_offset = (self.scroll_offset + lines as usize).min(max_scroll);
        } else if lines < 0 {
            // Scroll up
            self.scroll_offset = self.scroll_offset.saturating_sub((-lines) as usize);
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

    /// Handle mouse down - start selection
    pub fn handle_mouse_down(&mut self, click_x: i32, click_y: i32) {
        // Adjust for gutter offset plus spacing
        let text_x = (click_x - GUTTER_WIDTH as i32 - GUTTER_SPACING as i32).max(0);

        // Convert click position to row (no rounding needed - text starts at top of line)
        let visible_row = (click_y / LINE_HEIGHT as i32).max(0) as usize;

        // Calculate actual row accounting for scroll offset
        let row = self.scroll_offset + visible_row;

        // Clamp row to valid range
        if row < self.lines.len() {
            self.cursor_row = row;

            // Convert pixel position to column (handles variable-width fonts)
            let col = self.pixel_to_col(row, text_x);

            // Clamp column to line length
            let line_len = self.lines[row].len();
            self.cursor_col = col.min(line_len);

            // Start selection
            self.selection_start = Some((row, self.cursor_col));
            self.selection_end = Some((row, self.cursor_col));
            self.is_selecting = true;
        }
    }

    /// Handle mouse drag - update selection (returns true if selection changed)
    pub fn handle_mouse_drag(&mut self, click_x: i32, click_y: i32) -> bool {
        if !self.is_selecting {
            return false;
        }

        // Adjust for gutter offset plus spacing
        let text_x = (click_x - GUTTER_WIDTH as i32 - GUTTER_SPACING as i32).max(0);

        // Convert click position to row (no rounding needed - text starts at top of line)
        let visible_row = (click_y / LINE_HEIGHT as i32).max(0) as usize;

        // Calculate actual row accounting for scroll offset
        let row = self.scroll_offset + visible_row;

        // Clamp row to valid range
        if row < self.lines.len() {
            // Convert pixel position to column (handles variable-width fonts)
            let col = self.pixel_to_col(row, text_x);

            // Clamp column to line length
            let line_len = self.lines[row].len();
            let clamped_col = col.min(line_len);

            // Check if selection actually changed
            let new_pos = (row, clamped_col);
            if self.selection_end == Some(new_pos) {
                return false; // No change
            }

            // Update selection end and cursor
            self.selection_end = Some(new_pos);
            self.cursor_row = row;
            self.cursor_col = clamped_col;
            return true;
        }
        false
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

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.is_selecting = false;
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    /// Select all text
    pub fn select_all(&mut self) {
        if self.lines.is_empty() {
            return;
        }

        // Set selection from start of first line to end of last line
        self.selection_start = Some((0, 0));
        let last_line = self.lines.len() - 1;
        let last_col = self.lines[last_line].len();
        self.selection_end = Some((last_line, last_col));

        // Move cursor to end
        self.cursor_row = last_line;
        self.cursor_col = last_col;
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

            let (start_row, start_col) = start;
            let (end_row, end_col) = end;

            if start_row == end_row {
                // Single line selection
                Some(self.lines[start_row][start_col..end_col].to_string())
            } else {
                // Multi-line selection
                let mut result = String::new();

                // First line
                result.push_str(&self.lines[start_row][start_col..]);
                result.push('\n');

                // Middle lines
                for row in (start_row + 1)..end_row {
                    result.push_str(&self.lines[row]);
                    result.push('\n');
                }

                // Last line
                result.push_str(&self.lines[end_row][..end_col]);

                Some(result)
            }
        } else {
            None
        }
    }

    /// Copy selected text to clipboard
    pub fn copy(&mut self) {
        if let Some(text) = self.get_selected_text() {
            crate::gui::clipboard::copy(text);
            self.set_status("Copied to clipboard");
        } else {
            self.set_status("No selection to copy");
        }
    }

    /// Cut selected text to clipboard
    pub fn cut(&mut self) {
        if let Some(text) = self.get_selected_text() {
            // Save state before modification
            self.save_snapshot();

            crate::gui::clipboard::copy(text);
            self.delete_selection();
            self.set_status("Cut to clipboard");
        } else {
            self.set_status("No selection to cut");
        }
    }

    /// Paste text from clipboard
    pub fn paste(&mut self) {
        if let Some(text) = crate::gui::clipboard::paste() {
            // Save state before modification
            self.save_snapshot();

            // Delete selection first if there is one
            self.delete_selection();

            // Insert the text (without saving snapshots for each char)
            for ch in text.chars() {
                if ch == '\n' {
                    // Insert newline without snapshot
                    if self.cursor_row >= self.lines.len() {
                        self.lines.push(String::new());
                        self.cursor_row = self.lines.len() - 1;
                        self.cursor_col = 0;
                        self.modified = true;
                        continue;
                    }

                    let current_line = &self.lines[self.cursor_row];
                    let before = current_line[..self.cursor_col].to_string();
                    let after = current_line[self.cursor_col..].to_string();

                    self.lines[self.cursor_row] = before;
                    self.lines.insert(self.cursor_row + 1, after);

                    self.cursor_row += 1;
                    self.cursor_col = 0;
                    self.modified = true;

                    if self.cursor_row >= self.scroll_offset + self.visible_height {
                        self.scroll_offset = self.cursor_row - self.visible_height + 1;
                    }
                } else {
                    // Insert char without snapshot
                    if self.cursor_row >= self.lines.len() {
                        self.lines.push(String::new());
                    }

                    if self.lines[self.cursor_row].len() < EDITOR_WIDTH {
                        self.lines[self.cursor_row].insert(self.cursor_col, ch);
                        self.cursor_col += 1;
                        self.modified = true;
                    }
                }
            }
            self.set_status("Pasted from clipboard");
        } else {
            self.set_status("Clipboard is empty");
        }
    }

    /// Render the editor at a specific offset (for window rendering)
    pub fn render_at(&mut self, offset_x: i32, offset_y: i32, height: u32) {
        // Calculate how many lines can fit in the given height
        self.visible_height = (height / LINE_HEIGHT) as usize;
        if self.visible_height == 0 {
            self.visible_height = 1; // Always show at least one line
        }
        // Normalize selection (start should be before end)
        let selection = if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            if start <= end {
                Some((start, end))
            } else {
                Some((end, start))
            }
        } else {
            None
        };

        // Draw visible lines
        let visible_end = (self.scroll_offset + self.visible_height).min(self.lines.len());

        // Text starts after gutter plus spacing
        let text_offset_x = offset_x + GUTTER_WIDTH as i32 + GUTTER_SPACING as i32;

        for (idx, line_num) in (self.scroll_offset..visible_end).enumerate() {
            let line = &self.lines[line_num];
            let y = offset_y + (idx as i32 * LINE_HEIGHT as i32);

            // Draw gutter background
            for dy in 0..LINE_HEIGHT {
                for dx in 0..GUTTER_WIDTH {
                    let px = offset_x + dx as i32;
                    let py = y + dy as i32;
                    if px >= 0 && py >= 0 {
                        framebuffer::draw_pixel(px as u32, py as u32, COLOR_GUTTER_BG);
                    }
                }
            }

            // Draw line number in gutter (centered)
            let line_number = line_num + 1; // 1-indexed for display
            let line_num_str = alloc::format!("{:2}", line_number); // 2 digits
            let line_num_width = 2 * CHAR_WIDTH; // 2 chars = 32 pixels
            let line_num_x = offset_x + ((GUTTER_WIDTH - line_num_width) / 2) as i32; // Center in gutter
            framebuffer::draw_string(line_num_x as u32, y as u32, &line_num_str, COLOR_LINE_NUMBER);

            // Draw selection background for this line if applicable
            if let Some(((start_row, start_col), (end_row, end_col))) = selection {
                if line_num >= start_row && line_num <= end_row {
                    // Determine selection range for this line
                    let sel_start_col = if line_num == start_row { start_col } else { 0 };
                    let sel_end_col = if line_num == end_row { end_col } else { line.len() };

                    // Draw selection highlight (offset by gutter width)
                    if sel_start_col < sel_end_col {
                        let sel_start_x = self.col_to_pixel_offset(line_num, sel_start_col);
                        let sel_end_x = self.col_to_pixel_offset(line_num, sel_end_col);
                        let sel_x = text_offset_x + sel_start_x;
                        let sel_width = (sel_end_x - sel_start_x) as u32;

                        for dy in 0..CHAR_HEIGHT {
                            for dx in 0..sel_width {
                                let px = sel_x + dx as i32;
                                let py = y + dy as i32;
                                if px >= 0 && py >= 0 {
                                    framebuffer::draw_pixel(px as u32, py as u32, COLOR_SELECTION);
                                }
                            }
                        }
                    }
                }
            }

            // Draw entire line at once for proper character spacing with variable-width fonts
            let display_line: String = line.chars().take(EDITOR_WIDTH).collect();
            framebuffer::draw_string(text_offset_x as u32, y as u32, &display_line, COLOR_TEXT);
        }

        // Draw cursor (only if visible in current scroll view, offset by gutter width)
        if self.cursor_row >= self.scroll_offset &&
           self.cursor_row < self.scroll_offset + self.visible_height {
            let visible_row = self.cursor_row - self.scroll_offset;
            let cursor_pixel_offset = self.col_to_pixel_offset(self.cursor_row, self.cursor_col);
            let cursor_x = text_offset_x + cursor_pixel_offset;
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

pub fn render_at(id: usize, offset_x: i32, offset_y: i32, height: u32) {
    if let Some(editor) = get_editor(id) {
        editor.render_at(offset_x, offset_y, height);
    }
}
