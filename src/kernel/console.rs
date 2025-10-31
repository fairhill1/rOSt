// GUI Console/Terminal - Display text on the framebuffer

use crate::kernel::framebuffer;

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

static mut CONSOLE: Option<Console> = None;

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
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.buffer[self.cursor_y][self.cursor_x] = b' ';
                } else if self.cursor_y > 0 {
                    // Move to end of previous line
                    self.cursor_y -= 1;
                    self.cursor_x = CONSOLE_WIDTH - 1;
                    self.buffer[self.cursor_y][self.cursor_x] = b' ';
                }
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

    /// Render the console to the framebuffer
    pub fn render(&mut self) {
        if !self.dirty {
            return;
        }

        // Don't clear screen - the window manager handles that now

        // Draw all characters
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                let ch = self.buffer[y][x];
                if ch != b' ' {
                    // Draw character directly instead of using draw_string
                    let char_x = (x as u32) * CHAR_WIDTH;
                    let char_y = (y as u32) * LINE_HEIGHT; // Use LINE_HEIGHT for spacing

                    // Use a temporary buffer to create a string from the char
                    let mut buf = [0u8; 1];
                    buf[0] = ch;
                    if let Ok(s) = core::str::from_utf8(&buf) {
                        framebuffer::draw_string(char_x, char_y, s, self.fg_color);
                    }
                }
            }
        }

        // Draw cursor (blinking underscore)
        if self.cursor_x < CONSOLE_WIDTH && self.cursor_y < CONSOLE_HEIGHT {
            framebuffer::draw_string(
                (self.cursor_x as u32) * CHAR_WIDTH,
                (self.cursor_y as u32) * LINE_HEIGHT, // Use LINE_HEIGHT for spacing
                "_",
                self.fg_color,
            );
        }

        self.dirty = false;
    }

    /// Mark as dirty to force a redraw
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Initialize the global console
pub fn init() {
    unsafe {
        CONSOLE = Some(Console::new());
    }
}

/// Write a character to the console
pub fn write_char(ch: u8) {
    unsafe {
        if let Some(ref mut console) = CONSOLE {
            console.write_char(ch);
        }
    }
}

/// Write a string to the console
pub fn write_string(s: &str) {
    unsafe {
        if let Some(ref mut console) = CONSOLE {
            console.write_string(s);
        }
    }
}

/// Clear the console
pub fn clear() {
    unsafe {
        if let Some(ref mut console) = CONSOLE {
            console.clear();
        }
    }
}

/// Render the console to the framebuffer
pub fn render() {
    unsafe {
        if let Some(ref mut console) = CONSOLE {
            console.render();
        }
    }
}

/// Mark the console as dirty (needs redraw)
pub fn mark_dirty() {
    unsafe {
        if let Some(ref mut console) = CONSOLE {
            console.mark_dirty();
        }
    }
}
