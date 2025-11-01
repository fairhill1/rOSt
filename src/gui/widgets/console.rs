// GUI Console/Terminal - Display text on the framebuffer

use crate::gui::framebuffer;
extern crate alloc;
use alloc::vec::Vec;

const CONSOLE_WIDTH: usize = 64;  // 1024 pixels / 16 = 64 chars
const CONSOLE_HEIGHT: usize = 38; // 768 pixels / 20 = 38 lines (with spacing)
const CHAR_WIDTH: u32 = 16;  // Scaled 2x from 8
const CHAR_HEIGHT: u32 = 16; // Scaled 2x from 8
const LINE_SPACING: u32 = 4; // Extra pixels between lines
const LINE_HEIGHT: u32 = CHAR_HEIGHT + LINE_SPACING; // Total height per line

pub struct Console {
    buffer: [[u8; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u32,
    bg_color: u32,
    dirty: bool, // Track if we need to redraw
}

static mut CONSOLES: Vec<Console> = Vec::new();

impl Console {
    pub fn new() -> Self {
        Console {
            buffer: [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 0xFFFFFFFF, // White text
            bg_color: 0xFF000000, // Black background
            dirty: true,
        }
    }

    /// Write a single character to the console
    pub fn write_char(&mut self, ch: u8) {
        match ch {
            b'\n' => {
                // Newline - move to next line
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= CONSOLE_HEIGHT {
                    self.scroll_up();
                    self.cursor_y = CONSOLE_HEIGHT - 1;
                }
            }
            b'\r' => {
                // Carriage return - move to start of line
                self.cursor_x = 0;
            }
            8 | 127 => {
                // Backspace - move back and clear character
                // Only delete if we're not at the beginning of the line
                // (Don't delete previous lines' content!)
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.buffer[self.cursor_y][self.cursor_x] = b' ';
                }
                // If at beginning of line, do nothing - don't corrupt previous output
            }
            _ => {
                // Regular character
                if self.cursor_x >= CONSOLE_WIDTH {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= CONSOLE_HEIGHT {
                        self.scroll_up();
                        self.cursor_y = CONSOLE_HEIGHT - 1;
                    }
                }
                self.buffer[self.cursor_y][self.cursor_x] = ch;
                self.cursor_x += 1;
            }
        }
        self.dirty = true;
    }

    /// Write a string to the console
    pub fn write_string(&mut self, s: &str) {
        for ch in s.bytes() {
            self.write_char(ch);
        }
    }

    /// Scroll all lines up by one
    fn scroll_up(&mut self) {
        // Move all lines up
        for y in 1..CONSOLE_HEIGHT {
            self.buffer[y - 1] = self.buffer[y];
        }
        // Clear the last line
        self.buffer[CONSOLE_HEIGHT - 1] = [b' '; CONSOLE_WIDTH];
    }

    /// Clear the console
    pub fn clear(&mut self) {
        self.buffer = [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT];
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.dirty = true;
    }

    /// Render the console to the framebuffer at a specific offset (for window rendering)
    /// Note: Always renders, ignoring dirty flag, because caller controls when to redraw
    pub fn render_at(&mut self, offset_x: i32, offset_y: i32, max_width: u32, max_height: u32) {
        // Always render when called - the caller (main loop) decides when to redraw
        // The dirty flag is only used for the legacy render() function

        // Calculate max characters that fit in the window
        let max_chars_x = (max_width / CHAR_WIDTH) as usize;
        let max_chars_y = (max_height / LINE_HEIGHT) as usize;

        // Don't clear screen - the window manager handles that now

        // Draw all characters with offset, but only those that fit in the window
        for y in 0..CONSOLE_HEIGHT.min(max_chars_y) {
            for x in 0..CONSOLE_WIDTH.min(max_chars_x) {
                let ch = self.buffer[y][x];
                if ch != b' ' {
                    // Draw character directly instead of using draw_string
                    let char_x = offset_x + ((x as i32) * CHAR_WIDTH as i32);
                    let char_y = offset_y + ((y as i32) * LINE_HEIGHT as i32);

                    if char_x >= 0 && char_y >= 0 {
                        // Use a temporary buffer to create a string from the char
                        let mut buf = [0u8; 1];
                        buf[0] = ch;
                        if let Ok(s) = core::str::from_utf8(&buf) {
                            framebuffer::draw_string(char_x as u32, char_y as u32, s, self.fg_color);
                        }
                    }
                }
            }
        }

        // Draw cursor (solid block)
        if self.cursor_x < CONSOLE_WIDTH && self.cursor_y < CONSOLE_HEIGHT {
            let cursor_x = offset_x + ((self.cursor_x as i32) * CHAR_WIDTH as i32);
            let cursor_y = offset_y + ((self.cursor_y as i32) * LINE_HEIGHT as i32);

            if cursor_x >= 0 && cursor_y >= 0 {
                // Draw a solid block cursor with bright green color
                for dy in 0..CHAR_HEIGHT {
                    for dx in 0..CHAR_WIDTH {
                        let px = cursor_x as u32 + dx as u32;
                        let py = cursor_y as u32 + dy as u32;
                        framebuffer::draw_pixel(px, py, 0xFF00FF00); // Bright green
                    }
                }

                // Draw the character at cursor position in black so it's visible on green
                let ch = self.buffer[self.cursor_y][self.cursor_x];
                if ch != b' ' {
                    let mut buf = [0u8; 1];
                    buf[0] = ch;
                    if let Ok(s) = core::str::from_utf8(&buf) {
                        framebuffer::draw_string(cursor_x as u32, cursor_y as u32, s, 0xFF000000);
                    }
                }
            }
        }

        self.dirty = false;
    }

    /// Render the console to the framebuffer (legacy interface for compatibility)
    pub fn render(&mut self) {
        // Only render if dirty (optimization for legacy code path)
        if self.dirty {
            // Use full screen dimensions for legacy render
            self.render_at(0, 0, 1024, 768);
        }
    }

    /// Mark as dirty to force a redraw
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Initialize the console system
pub fn init() {
    // Nothing to do - consoles are created on demand
}

/// Create a new console instance and return its ID
pub fn create_console() -> usize {
    unsafe {
        CONSOLES.push(Console::new());
        CONSOLES.len() - 1
    }
}

/// Remove a console instance by ID
pub fn remove_console(id: usize) {
    unsafe {
        if id < CONSOLES.len() {
            CONSOLES.remove(id);
        }
    }
}

/// Get a console instance by ID
pub fn get_console(id: usize) -> Option<&'static mut Console> {
    unsafe {
        CONSOLES.get_mut(id)
    }
}

/// Write a character to a specific console
pub fn write_char(id: usize, ch: u8) {
    if let Some(console) = get_console(id) {
        console.write_char(ch);
    }
}

/// Write a string to a specific console
pub fn write_string(id: usize, s: &str) {
    if let Some(console) = get_console(id) {
        console.write_string(s);
    }
}

/// Clear a specific console
pub fn clear(id: usize) {
    if let Some(console) = get_console(id) {
        console.clear();
    }
}

/// Render a specific console to the framebuffer
pub fn render(id: usize) {
    if let Some(console) = get_console(id) {
        console.render();
    }
}

/// Render a specific console at a specific offset (for window rendering)
pub fn render_at(id: usize, offset_x: i32, offset_y: i32, max_width: u32, max_height: u32) {
    if let Some(console) = get_console(id) {
        console.render_at(offset_x, offset_y, max_width, max_height);
    }
}

/// Mark a specific console as dirty (needs redraw)
pub fn mark_dirty(id: usize) {
    if let Some(console) = get_console(id) {
        console.mark_dirty();
    }
}
