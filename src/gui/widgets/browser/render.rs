/// Rendering functions for the browser widget
use super::types::*;
use super::utils::*;
use crate::gui::framebuffer::FONT_8X8;

/// Render browser to framebuffer
pub fn render(
    browser: &crate::gui::widgets::browser::Browser,
    fb: &mut [u32],
    fb_width: usize,
    fb_height: usize,
    win_x: usize,
    win_y: usize,
    win_width: usize,
    win_height: usize,
) {
    // Background
    for y in 0..win_height {
        for x in 0..win_width {
            let fb_x = win_x + x;
            let fb_y = win_y + y;
            if fb_x < fb_width && fb_y < fb_height {
                fb[fb_y * fb_width + fb_x] = 0xFFFFFFFF; // White
            }
        }
    }

    // Address bar background
    for y in 0..30 {
        for x in 0..win_width {
            let fb_x = win_x + x;
            let fb_y = win_y + y;
            if fb_x < fb_width && fb_y < fb_height {
                fb[fb_y * fb_width + fb_x] = 0xFFEEEEEE; // Light gray
            }
        }
    }

    // URL input field
    let input_x = 40;
    let input_y = 3;
    let input_width = win_width.saturating_sub(130); // Leave space for buttons
    let input_height = 24;

    // Render the TextInput widget
    browser.url_input.render_at(
        (win_x + input_x) as i32,
        (win_y + input_y) as i32,
        input_width as u32,
        input_height as u32,
        browser.url_focused
    );

    // Back button (22px tall, centered on 24px input field)
    let back_btn_x = win_x + win_width - 80;
    let back_btn_y = win_y + 4; // 3 + (24-22)/2 = 4
    let back_btn_width = 32;
    let back_btn_height = 22;
    draw_button(fb, fb_width, fb_height, back_btn_x, back_btn_y, back_btn_width, back_btn_height, "<");

    // Forward button (22px tall, centered on 24px input field)
    let fwd_btn_x = win_x + win_width - 44;
    let fwd_btn_y = win_y + 4; // 3 + (24-22)/2 = 4
    let fwd_btn_width = 32;
    let fwd_btn_height = 22;
    draw_button(fb, fb_width, fb_height, fwd_btn_x, fwd_btn_y, fwd_btn_width, fwd_btn_height, ">");

    // Content area
    let content_y = win_y + 35;
    let content_height = win_height.saturating_sub(35);

    for layout_box in &browser.layout {
        // Skip boxes completely above the viewport (scrolled off top)
        if layout_box.y + layout_box.height <= browser.scroll_offset {
            continue;
        }

        // Skip boxes completely below the viewport
        if layout_box.y >= browser.scroll_offset + content_height {
            continue;
        }

        // Calculate position relative to scroll (can be negative if partially scrolled off top)
        let y_signed = layout_box.y as isize - browser.scroll_offset as isize;

        // Check if this is an HR, image, or text
        if layout_box.is_hr {
            // Draw horizontal rule as solid pixel line
            if y_signed >= 0 && y_signed < content_height as isize {
                for line_y in 0..layout_box.height {
                    let fb_y = content_y + y_signed as usize + line_y;
                    if fb_y < fb_height {
                        for line_x in 0..layout_box.width {
                            let fb_x = win_x + layout_box.x + line_x;
                            if fb_x < fb_width {
                                fb[fb_y * fb_width + fb_x] = layout_box.color.to_u32();
                            }
                        }
                    }
                }
            }
        } else if layout_box.is_image {
            // Draw image (with clipping for partially visible images)
            if let Some(ref img) = layout_box.image_data {
                // Render image (decoders output pixels[0] as top-left)
                for img_y in 0..img.height as usize {
                    for img_x in 0..img.width as usize {
                        // Calculate screen position
                        let screen_y = y_signed + img_y as isize;

                        // Skip pixels above viewport
                        if screen_y < 0 {
                            continue;
                        }

                        // Skip pixels below viewport
                        if screen_y >= content_height as isize {
                            continue;
                        }

                        let fb_x = win_x + layout_box.x + img_x;
                        let fb_y = content_y + screen_y as usize;

                        // Clip to framebuffer bounds
                        if fb_x < fb_width && fb_y < fb_height {
                            let pixel_idx = img_y * img.width as usize + img_x;
                            if pixel_idx < img.pixels.len() {
                                // Swap R and B channels: 0xAABBGGRR -> 0xAARRGGBB
                                let pixel = img.pixels[pixel_idx];
                                let r = pixel & 0xFF;
                                let g = (pixel >> 8) & 0xFF;
                                let b = (pixel >> 16) & 0xFF;
                                let a = pixel & 0xFF000000;
                                let swapped = a | (r << 16) | (g << 8) | b;
                                fb[fb_y * fb_width + fb_x] = swapped;
                            }
                        }
                    }
                }
            }
        } else if layout_box.is_table_cell {
            // Draw table cell with background and border
            if y_signed >= 0 && y_signed < content_height as isize {
                // Draw cell background
                for cell_y in 0..layout_box.height {
                    let fb_y = content_y + (y_signed as usize).saturating_add(cell_y);
                    if fb_y < fb_height {
                        for cell_x in 0..layout_box.width {
                            let fb_x = win_x + layout_box.x + cell_x;
                            if fb_x < fb_width {
                                fb[fb_y * fb_width + fb_x] = layout_box.color.to_u32();
                            }
                        }
                    }
                }

                // Draw borders (1px solid)
                let border_color = Color::new(180, 180, 180).to_u32(); // Gray

                // Top border
                let fb_y_top = content_y + y_signed as usize;
                if fb_y_top < fb_height {
                    for cell_x in 0..layout_box.width {
                        let fb_x = win_x + layout_box.x + cell_x;
                        if fb_x < fb_width {
                            fb[fb_y_top * fb_width + fb_x] = border_color;
                        }
                    }
                }

                // Bottom border
                let fb_y_bottom = content_y + (y_signed as usize).saturating_add(layout_box.height.saturating_sub(1));
                if fb_y_bottom < fb_height {
                    for cell_x in 0..layout_box.width {
                        let fb_x = win_x + layout_box.x + cell_x;
                        if fb_x < fb_width {
                            fb[fb_y_bottom * fb_width + fb_x] = border_color;
                        }
                    }
                }

                // Left border
                for cell_y in 0..layout_box.height {
                    let fb_y = content_y + (y_signed as usize).saturating_add(cell_y);
                    if fb_y < fb_height {
                        let fb_x = win_x + layout_box.x;
                        if fb_x < fb_width {
                            fb[fb_y * fb_width + fb_x] = border_color;
                        }
                    }
                }

                // Right border
                for cell_y in 0..layout_box.height {
                    let fb_y = content_y + (y_signed as usize).saturating_add(cell_y);
                    if fb_y < fb_height {
                        let fb_x = win_x + layout_box.x + layout_box.width.saturating_sub(1);
                        if fb_x < fb_width {
                            fb[fb_y * fb_width + fb_x] = border_color;
                        }
                    }
                }
            }
        } else {
            // Draw background color if specified (for full-width backgrounds or text backgrounds)
            if let Some(bg_color) = &layout_box.background_color {
                if y_signed >= 0 && y_signed + layout_box.height as isize <= content_height as isize {
                    let bg_color_u32 = bg_color.to_u32();
                    for bg_y in 0..layout_box.height {
                        let fb_y = content_y + y_signed as usize + bg_y;
                        if fb_y < fb_height {
                            for bg_x in 0..layout_box.width {
                                let fb_x = win_x + layout_box.x + bg_x;
                                if fb_x < fb_width {
                                    fb[fb_y * fb_width + fb_x] = bg_color_u32;
                                }
                            }
                        }
                    }
                }
            }

            // Only draw text if there is text and it's fully visible
            if !layout_box.text.is_empty() {
                if y_signed >= 0 && y_signed + layout_box.height as isize <= content_height as isize {
                    draw_text(
                        fb,
                        fb_width,
                        fb_height,
                        win_x + layout_box.x,
                        content_y + y_signed as usize,
                        &layout_box.text,
                        &layout_box.color,
                        layout_box.font_size,
                    );

                    // Underline links
                    if layout_box.is_link {
                        for x in 0..layout_box.width {
                            let fb_x = win_x + layout_box.x + x;
                            let fb_y = content_y + y_signed as usize + layout_box.height;
                            if fb_x < fb_width && fb_y < fb_height {
                                fb[fb_y * fb_width + fb_x] = layout_box.color.to_u32();
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Draw text using TrueType font if available, otherwise bitmap font
pub fn draw_text(fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, text: &str, color: &Color, font_size_level: usize) {
    if crate::gui::font::is_available() {
        // Use TrueType font
        let font_size_px = get_font_size_px(font_size_level);
        let color_u32 = color.to_u32();
        crate::gui::font::draw_string(x as i32, y as i32, text, color_u32, font_size_px);
    } else {
        // Fallback to bitmap font
        let mut current_x = x;
        for ch in text.chars() {
            if ch.is_ascii() {
                let glyph = FONT_8X8[ch as usize];
                // Scale the 8x8 bitmap by font_size_level
                for row in 0..8 {
                    for col in 0..8 {
                        if (glyph[row] & (1 << (7 - col))) != 0 {
                            // Draw a font_size_level x font_size_level block for each pixel in the glyph
                            for dy in 0..font_size_level {
                                for dx in 0..font_size_level {
                                    let fb_x = current_x + col * font_size_level + dx;
                                    let fb_y = y + row * font_size_level + dy;
                                    if fb_x < fb_width && fb_y < fb_height {
                                        fb[fb_y * fb_width + fb_x] = color.to_u32();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            current_x += CHAR_WIDTH * font_size_level;
        }
    }
}

/// Draw a button with border, background, and centered text (like menu bar buttons)
pub fn draw_button(fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, width: usize, height: usize, text: &str) {
    const COLOR_BUTTON_BORDER: u32 = 0xFF555555;
    const COLOR_BUTTON_BG: u32 = 0xFF3D3D3D;
    const COLOR_BUTTON_TEXT: u32 = 0xFFFFFFFF;

    // Draw border
    for py in y..y+height {
        for px in x..x+width {
            if px < fb_width && py < fb_height {
                fb[py * fb_width + px] = COLOR_BUTTON_BORDER;
            }
        }
    }

    // Draw background (inset by 1 pixel for border)
    for py in y+1..y+height-1 {
        for px in x+1..x+width-1 {
            if px < fb_width && py < fb_height {
                fb[py * fb_width + px] = COLOR_BUTTON_BG;
            }
        }
    }

    // Draw centered text
    let text_width = if crate::gui::font::is_available() {
        crate::gui::font::measure_string(text, 18.0) as usize
    } else {
        text.len() * 8
    };
    let text_height = if crate::gui::font::is_available() {
        crate::gui::font::get_char_height() as usize
    } else {
        8
    };

    let text_x = x + (width - text_width) / 2;
    let text_y = y + (height - text_height) / 2;

    if crate::gui::font::is_available() {
        crate::gui::font::draw_string(text_x as i32, text_y as i32, text, COLOR_BUTTON_TEXT, 18.0);
    } else {
        draw_text(fb, fb_width, fb_height, text_x, text_y, text, &Color::new(255, 255, 255), 1);
    }
}
