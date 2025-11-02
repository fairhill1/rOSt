/// Character dimensions for bitmap font
pub const CHAR_WIDTH: usize = 8;
pub const CHAR_HEIGHT: usize = 8;

/// Get the actual font size in pixels based on heading level
pub fn get_font_size_px(font_size_level: usize) -> f32 {
    // When using TTF, use real pixel sizes
    // When using bitmap, use multipliers of 8px
    if crate::gui::font::is_available() {
        match font_size_level {
            6 => 48.0,  // CSS 48px / 8 = 6
            5 => 36.0,  // h1: large
            4 => 28.0,  // h2: medium-large
            3 => 24.0,  // h3: medium (also CSS 24px / 8 = 3)
            2 => 20.0,  // h4-h6: slightly larger than body (also CSS 18px / 8 = 2.25 → 2)
            1 => 18.0,  // body text
            _ => (font_size_level * 8) as f32,  // For other CSS sizes, multiply by 8
        }
    } else {
        // Bitmap font - return multiplier * 8
        (font_size_level * 8) as f32
    }
}
