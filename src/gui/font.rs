// TrueType font rendering using fontdue

use fontdue::Font;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FontMode {
    Auto,      // Use TTF if available, fallback to bitmap
    TrueType,  // Force TTF only (error if not loaded)
    Bitmap,    // Force bitmap font
}

static mut FONT: Option<Font> = None;
static mut FONT_SIZE: f32 = 18.0; // Default font size (18px for better readability)
static mut FONT_LOAD_ATTEMPTED: bool = false; // Track if we've tried loading
static mut FONT_PREFERENCE: FontMode = FontMode::Auto; // User's font preference

/// Try to lazy-load the font from filesystem on first use
fn try_load_font() {
    unsafe {
        if FONT_LOAD_ATTEMPTED {
            return; // Already tried, don't try again
        }
        FONT_LOAD_ATTEMPTED = true;

        crate::kernel::uart_write_string("[FONT] Attempting to load i24.ttf from filesystem...\r\n");

        // Access global block devices and mount filesystem directly
        if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
            crate::kernel::uart_write_string("[FONT] Block devices available\r\n");

            if devices.is_empty() {
                crate::kernel::uart_write_string("[FONT] ✗ No block devices found\r\n");
                return;
            }

            let device_idx = 0;
            let device = &mut devices[device_idx];

            // Mount the filesystem directly
            match crate::system::fs::filesystem::SimpleFilesystem::mount(device) {
                Ok(fs) => {
                    crate::kernel::uart_write_string("[FONT] Filesystem mounted successfully\r\n");

                    // Get file list
                    let files = fs.list_files();
                    crate::kernel::uart_write_string(&alloc::format!("[FONT] Found {} files in filesystem\r\n", files.len()));

                    if let Some(file) = files.iter().find(|f| f.get_name() == "i24.ttf") {
                        let size = file.get_size_bytes() as usize;
                        crate::kernel::uart_write_string(&alloc::format!("[FONT] i24.ttf found! Size: {} bytes\r\n", size));

                        let mut buffer = alloc::vec![0u8; size];

                        match fs.read_file(device, "i24.ttf", &mut buffer) {
                            Ok(_) => {
                                crate::kernel::uart_write_string("[FONT] File read successfully\r\n");

                                match Font::from_bytes(buffer, fontdue::FontSettings::default()) {
                                    Ok(font) => {
                                        FONT = Some(font);
                                        crate::kernel::uart_write_string("[FONT] ✓ TrueType font loaded successfully!\r\n");
                                    }
                                    Err(_) => {
                                        crate::kernel::uart_write_string("[FONT] ✗ Failed to parse TTF font\r\n");
                                    }
                                }
                            }
                            Err(e) => {
                                crate::kernel::uart_write_string(&alloc::format!("[FONT] ✗ Failed to read i24.ttf: {}\r\n", e));
                            }
                        }
                    } else {
                        crate::kernel::uart_write_string("[FONT] ✗ i24.ttf not found in filesystem\r\n");
                    }
                }
                Err(e) => {
                    crate::kernel::uart_write_string(&alloc::format!("[FONT] ✗ Failed to mount filesystem: {}\r\n", e));
                }
            }
        } else {
            crate::kernel::uart_write_string("[FONT] ✗ No block devices available\r\n");
        }
    }
}

/// Check if fontdue is available (lazy loads on first call)
/// Returns true if TrueType font should be used based on preference and availability
pub fn is_available() -> bool {
    unsafe {
        if !FONT_LOAD_ATTEMPTED {
            try_load_font();
        }

        match FONT_PREFERENCE {
            FontMode::Auto => FONT.is_some(),      // Use TTF if loaded
            FontMode::TrueType => FONT.is_some(),  // Force TTF (will fail if not loaded)
            FontMode::Bitmap => false,             // Force bitmap
        }
    }
}

/// Set font preference (Auto, TrueType, or Bitmap)
pub fn set_font_mode(mode: FontMode) {
    unsafe {
        FONT_PREFERENCE = mode;
    }
}

/// Get current font preference
pub fn get_font_mode() -> FontMode {
    unsafe { FONT_PREFERENCE }
}

/// Set the default font size
pub fn set_size(size: f32) {
    unsafe {
        FONT_SIZE = size;
    }
}

/// Get the current font size
pub fn get_size() -> f32 {
    unsafe { FONT_SIZE }
}

/// Get the character height for the current font (without line spacing)
/// Returns height in pixels (suitable for cursor rendering)
pub fn get_char_height() -> u32 {
    unsafe {
        // Check if we should use TrueType based on preference
        if is_available() {
            if let Some(ref font) = FONT {
                if let Some(metrics) = font.horizontal_line_metrics(FONT_SIZE) {
                    // Use font's natural height (ascent + descent)
                    let height = metrics.ascent - metrics.descent;
                    // Round up (no_std compatible - add 1 if has fractional part)
                    let truncated = height as u32;
                    if height > truncated as f32 { truncated + 1 } else { truncated }
                } else {
                    16 // Fallback if metrics unavailable
                }
            } else {
                // Bitmap font fallback: 16px char height
                16
            }
        } else {
            // Bitmap font: 16px char height
            16
        }
    }
}

/// Get the recommended line height for the current font
/// Returns height in pixels (suitable for use in LINE_HEIGHT constants)
pub fn get_line_height() -> u32 {
    unsafe {
        // Check if we should use TrueType based on preference
        if is_available() {
            if let Some(ref font) = FONT {
                if let Some(metrics) = font.horizontal_line_metrics(FONT_SIZE) {
                    // Use font's natural line height (ascent + descent + line gap)
                    let height = metrics.ascent - metrics.descent + metrics.line_gap;
                    // Add a small spacing buffer (20% of font size, min 2px)
                    let spacing = (FONT_SIZE * 0.2).max(2.0);
                    let total = height + spacing;
                    // Round up (no_std compatible - add 1 if has fractional part)
                    let truncated = total as u32;
                    if total > truncated as f32 { truncated + 1 } else { truncated }
                } else {
                    20 // Fallback if metrics unavailable
                }
            } else {
                // Bitmap font fallback: 16px char + 4px spacing
                20
            }
        } else {
            // Bitmap font: 16px char + 4px spacing
            20
        }
    }
}

/// Render a single character and return its metrics and bitmap
/// Returns (width, height, advance_width, bitmap) where bitmap is grayscale 0-255
pub fn rasterize_char(ch: char, size: f32) -> Option<(usize, usize, f32, Vec<u8>)> {
    unsafe {
        if let Some(ref font) = FONT {
            let (metrics, bitmap) = font.rasterize(ch, size);
            Some((metrics.width, metrics.height, metrics.advance_width, bitmap))
        } else {
            None
        }
    }
}

/// Draw a single character at the specified position with color
/// Returns the advance width (how much to move X for the next character)
pub fn draw_char(x: i32, y: i32, ch: char, color: u32, size: f32) -> i32 {
    unsafe {
        if let Some(ref font) = FONT {
            let (metrics, bitmap) = font.rasterize(ch, size);

            // Draw each pixel of the glyph with antialiasing
            // For screen coordinates (PositiveYDown), the baseline offset is -(height + ymin)
            // This ensures 'c' and 'I' align properly on the baseline
            let baseline_offset = -(metrics.height as i32 + metrics.ymin);

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];

                    // Skip fully transparent pixels
                    if alpha == 0 {
                        continue;
                    }

                    let px = x + metrics.xmin + col as i32;
                    let py = y + baseline_offset + row as i32;

                    // For sharper rendering: threshold alpha instead of full blending
                    // This trades some smoothness for crispness
                    let blended_color = if alpha > 127 {
                        // Above threshold - draw full color (sharper)
                        color
                    } else if alpha > 64 {
                        // Light antialiasing only at edges
                        blend_with_background(px, py, color, alpha)
                    } else {
                        // Too transparent, skip
                        continue;
                    };

                    crate::gui::framebuffer::draw_pixel(px as u32, py as u32, blended_color);
                }
            }

            metrics.advance_width as i32
        } else {
            0
        }
    }
}

/// Blend a color with the background pixel using alpha
fn blend_with_background(x: i32, y: i32, fg_color: u32, alpha: u8) -> u32 {
    // Get existing background color
    let bg_color = get_pixel_color(x, y);

    // Extract RGB components
    let fg_r = ((fg_color >> 16) & 0xFF) as u16;
    let fg_g = ((fg_color >> 8) & 0xFF) as u16;
    let fg_b = (fg_color & 0xFF) as u16;

    let bg_r = ((bg_color >> 16) & 0xFF) as u16;
    let bg_g = ((bg_color >> 8) & 0xFF) as u16;
    let bg_b = (bg_color & 0xFF) as u16;

    let alpha = alpha as u16;

    // Alpha blend: result = (fg * alpha + bg * (255 - alpha)) / 255
    let r = ((fg_r * alpha + bg_r * (255 - alpha)) / 255) as u32;
    let g = ((fg_g * alpha + bg_g * (255 - alpha)) / 255) as u32;
    let b = ((fg_b * alpha + bg_b * (255 - alpha)) / 255) as u32;

    (r << 16) | (g << 8) | b
}

/// Get pixel color at position (helper for blending)
fn get_pixel_color(x: i32, y: i32) -> u32 {
    let buffer = crate::gui::framebuffer::get_back_buffer();
    let (width, _height) = crate::gui::framebuffer::get_screen_dimensions();

    if x >= 0 && y >= 0 && x < width as i32 {
        let offset = (y as u32 * width + x as u32) as usize;
        if offset < buffer.len() {
            return buffer[offset];
        }
    }

    0x000000 // Default to black
}

/// Draw a string at the specified position with color
/// Returns the total width of the rendered string
pub fn draw_string(x: i32, y: i32, text: &str, color: u32, size: f32) -> i32 {
    unsafe {
        if let Some(ref font) = FONT {
            // Get the font metrics to establish proper vertical positioning
            // The ascent is the distance from baseline to top of tallest glyph
            if let Some(line_metrics) = font.horizontal_line_metrics(size) {
                // Y coordinate passed in is the TOP of the text
                // Baseline is ascent pixels down from the top
                let baseline_y = y + line_metrics.ascent as i32;

                let mut cursor_x = x;

                for ch in text.chars() {
                    if ch == '\n' {
                        // Newline - caller should handle this
                        break;
                    }

                    let advance = draw_char(cursor_x, baseline_y, ch, color, size);
                    cursor_x += advance;
                }

                return cursor_x - x; // Return total width
            }
        }
    }

    // Fallback if font not available
    0
}

/// Measure the width of a string without rendering it
/// IMPORTANT: Must match draw_string's rounding behavior (accumulate as i32)
pub fn measure_string(text: &str, size: f32) -> i32 {
    unsafe {
        if let Some(ref font) = FONT {
            // Accumulate as i32 to match draw_string's rounding (prevents cursor drift)
            let mut width = 0i32;
            for ch in text.chars() {
                let metrics = font.metrics(ch, size);
                width += metrics.advance_width as i32;
            }
            width
        } else {
            // Fallback: estimate based on bitmap font (8px * 2 = 16px per char)
            (text.len() * 16) as i32
        }
    }
}

/// Get metrics for a single character
pub fn char_metrics(ch: char, size: f32) -> Option<(f32, f32, f32)> {
    unsafe {
        if let Some(ref font) = FONT {
            let metrics = font.metrics(ch, size);
            Some((metrics.width as f32, metrics.height as f32, metrics.advance_width))
        } else {
            None
        }
    }
}

/// Get the line height for a specific font size
/// Returns height in pixels (ascent + descent + line gap)
pub fn get_line_height_for_size(size: f32) -> u32 {
    unsafe {
        if is_available() {
            if let Some(ref font) = FONT {
                if let Some(metrics) = font.horizontal_line_metrics(size) {
                    let height = metrics.ascent - metrics.descent + metrics.line_gap;
                    // Round up (no_std compatible - add 1 if has fractional part)
                    let int_height = height as u32;
                    if height > int_height as f32 {
                        int_height + 1
                    } else {
                        int_height
                    }
                } else {
                    // Fallback: use global font height
                    get_char_height()
                }
            } else {
                get_char_height()
            }
        } else {
            // Bitmap font fallback
            let multiplier = ((size / 8.0) + 0.5) as u32;
            16 * multiplier
        }
    }
}
