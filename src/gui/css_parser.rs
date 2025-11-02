/// CSS parser for rOSt browser
/// Supports inline styles and external stylesheets with selectors
/// MVP properties: color, background-color, font-size, width, height, margin, padding, text-align

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::gui::widgets::browser::Color;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineStyle {
    pub color: Option<Color>,
    pub background_color: Option<Color>,
    pub font_size: Option<usize>,  // in pixels
    pub width: Option<usize>,      // in pixels
    pub height: Option<usize>,     // in pixels
    pub margin: Option<usize>,     // simplified: single value for all sides
    pub padding: Option<usize>,    // simplified: single value for all sides
    pub text_align: Option<TextAlign>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// CSS Selector types
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Element(String),    // e.g., "p" or "div"
    Class(String),      // e.g., ".myclass"
    Id(String),         // e.g., "#myid"
}

impl Selector {
    /// Check if this selector matches the given element
    pub fn matches(&self, tag: &str, classes: &[&str], id: Option<&str>) -> bool {
        match self {
            Selector::Element(element) => element == tag,
            Selector::Class(class) => classes.contains(&class.as_str()),
            Selector::Id(selector_id) => {
                if let Some(element_id) = id {
                    selector_id == element_id
                } else {
                    false
                }
            }
        }
    }

    /// Get specificity for cascade (higher = more specific)
    /// ID selectors: 100, Class selectors: 10, Element selectors: 1
    pub fn specificity(&self) -> usize {
        match self {
            Selector::Id(_) => 100,
            Selector::Class(_) => 10,
            Selector::Element(_) => 1,
        }
    }
}

/// A single CSS rule: selector + declarations
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub style: InlineStyle,
}

/// External stylesheet containing multiple rules
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<StyleRule>,
}

impl InlineStyle {
    /// Parse inline style attribute value
    /// Example: "color: red; font-size: 16px; background-color: #ff0000"
    pub fn parse(style_str: &str) -> Self {
        let mut style = InlineStyle::default();

        // Split by semicolons to get individual declarations
        for declaration in style_str.split(';') {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                continue;
            }

            // Split by colon to get property: value
            if let Some(colon_pos) = declaration.find(':') {
                let property = declaration[..colon_pos].trim().to_lowercase();
                let value = declaration[colon_pos + 1..].trim();

                match property.as_str() {
                    "color" => {
                        style.color = parse_color(value);
                    }
                    "background-color" => {
                        style.background_color = parse_color(value);
                    }
                    "font-size" => {
                        style.font_size = parse_size(value);
                    }
                    "width" => {
                        style.width = parse_size(value);
                    }
                    "height" => {
                        style.height = parse_size(value);
                    }
                    "margin" => {
                        style.margin = parse_size(value);
                    }
                    "padding" => {
                        style.padding = parse_size(value);
                    }
                    "text-align" => {
                        style.text_align = parse_text_align(value);
                    }
                    _ => {
                        // Unsupported property - ignore for now
                    }
                }
            }
        }

        style
    }
}

/// Parse CSS color value (named colors, hex, rgb)
fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().to_lowercase();

    // Named colors (common subset)
    match value.as_str() {
        "black" => return Some(Color::new(0, 0, 0)),
        "white" => return Some(Color::new(255, 255, 255)),
        "red" => return Some(Color::new(255, 0, 0)),
        "green" => return Some(Color::new(0, 128, 0)),
        "blue" => return Some(Color::new(0, 0, 255)),
        "yellow" => return Some(Color::new(255, 255, 0)),
        "cyan" => return Some(Color::new(0, 255, 255)),
        "magenta" => return Some(Color::new(255, 0, 255)),
        "gray" | "grey" => return Some(Color::new(128, 128, 128)),
        "orange" => return Some(Color::new(255, 165, 0)),
        "purple" => return Some(Color::new(128, 0, 128)),
        "pink" => return Some(Color::new(255, 192, 203)),
        "brown" => return Some(Color::new(165, 42, 42)),
        _ => {}
    }

    // Hex colors: #RGB or #RRGGBB
    if value.starts_with('#') {
        let hex = &value[1..];

        if hex.len() == 3 {
            // #RGB format - expand to #RRGGBB
            let r = parse_hex_digit(hex.chars().nth(0)?)?;
            let g = parse_hex_digit(hex.chars().nth(1)?)?;
            let b = parse_hex_digit(hex.chars().nth(2)?)?;
            return Some(Color::new(r * 17, g * 17, b * 17)); // 0xF -> 0xFF
        } else if hex.len() == 6 {
            // #RRGGBB format
            let r = parse_hex_byte(&hex[0..2])?;
            let g = parse_hex_byte(&hex[2..4])?;
            let b = parse_hex_byte(&hex[4..6])?;
            return Some(Color::new(r, g, b));
        }
    }

    // rgb(r, g, b) format
    if value.starts_with("rgb(") && value.ends_with(')') {
        let rgb = &value[4..value.len()-1];
        let parts: Vec<&str> = rgb.split(',').map(|s| s.trim()).collect();
        if parts.len() == 3 {
            let r = parts[0].parse::<u8>().ok()?;
            let g = parts[1].parse::<u8>().ok()?;
            let b = parts[2].parse::<u8>().ok()?;
            return Some(Color::new(r, g, b));
        }
    }

    None
}

/// Parse CSS size value (px only for MVP)
fn parse_size(value: &str) -> Option<usize> {
    let value = value.trim().to_lowercase();

    // Remove 'px' suffix if present
    let number_str = if value.ends_with("px") {
        &value[..value.len()-2]
    } else {
        &value
    };

    number_str.trim().parse::<usize>().ok()
}

/// Parse text-align value
fn parse_text_align(value: &str) -> Option<TextAlign> {
    match value.trim().to_lowercase().as_str() {
        "left" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" => Some(TextAlign::Right),
        _ => None,
    }
}

/// Parse single hex digit (0-9, A-F)
fn parse_hex_digit(c: char) -> Option<u8> {
    c.to_digit(16).map(|d| d as u8)
}

/// Parse two hex digits as a byte
fn parse_hex_byte(hex: &str) -> Option<u8> {
    u8::from_str_radix(hex, 16).ok()
}

impl Stylesheet {
    /// Parse external CSS stylesheet
    /// Example: "p { color: red; } .myclass { font-size: 16px; }"
    pub fn parse(css: &str) -> Self {
        let mut rules = Vec::new();
        let mut chars = css.chars().peekable();

        // Skip whitespace and comments
        let skip_whitespace_and_comments = |chars: &mut core::iter::Peekable<core::str::Chars>| {
            loop {
                // Skip whitespace
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }

                // Check for comment start
                if chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    if chars.peek() == Some(&'*') {
                        chars.next(); // consume '*'
                        // Skip until we find */
                        let mut prev_was_star = false;
                        while let Some(c) = chars.next() {
                            if prev_was_star && c == '/' {
                                break;
                            }
                            prev_was_star = c == '*';
                        }
                        continue; // Check for more whitespace/comments
                    } else {
                        // Not a comment, we need to put '/' back somehow
                        // Since we can't put it back, we'll just break
                        // This is a limitation - we'll skip lone '/' characters
                        break;
                    }
                } else {
                    break;
                }
            }
        };

        while chars.peek().is_some() {
            skip_whitespace_and_comments(&mut chars);

            if chars.peek().is_none() {
                break;
            }

            // Parse selector
            let mut selector_str = String::new();
            while let Some(&c) = chars.peek() {
                if c == '{' || c.is_whitespace() {
                    break;
                }
                selector_str.push(chars.next().unwrap());
            }

            if selector_str.is_empty() {
                break;
            }

            skip_whitespace_and_comments(&mut chars);

            // Expect '{'
            if chars.peek() != Some(&'{') {
                break;
            }
            chars.next(); // consume '{'

            // Parse declarations until '}'
            let mut declarations = String::new();
            let mut brace_depth = 1;
            while let Some(c) = chars.next() {
                if c == '{' {
                    brace_depth += 1;
                    declarations.push(c);
                } else if c == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        break;
                    }
                    declarations.push(c);
                } else {
                    declarations.push(c);
                }
            }

            // Parse selector type
            let selector = if selector_str.starts_with('#') {
                Selector::Id(selector_str[1..].to_string())
            } else if selector_str.starts_with('.') {
                Selector::Class(selector_str[1..].to_string())
            } else {
                Selector::Element(selector_str.trim().to_string())
            };

            // Parse declarations using InlineStyle parser
            let style = InlineStyle::parse(&declarations);

            rules.push(StyleRule { selector, style });
        }

        Stylesheet { rules }
    }
}

/// Merge multiple styles with cascade rules (later/more specific = higher priority)
/// Order of application: element selector < class selector < id selector < inline style
pub fn merge_styles(base: InlineStyle, overrides: &[(&Selector, &InlineStyle)]) -> InlineStyle {
    let mut result = base;

    // Sort by specificity (lower first, so higher specificity overwrites)
    let mut sorted_overrides: Vec<_> = overrides.iter().collect();
    sorted_overrides.sort_by_key(|(sel, _)| sel.specificity());

    // Apply each override in specificity order
    for (_, style) in sorted_overrides {
        if let Some(color) = style.color {
            result.color = Some(color);
        }
        if let Some(bg) = style.background_color {
            result.background_color = Some(bg);
        }
        if let Some(fs) = style.font_size {
            result.font_size = Some(fs);
        }
        if let Some(w) = style.width {
            result.width = Some(w);
        }
        if let Some(h) = style.height {
            result.height = Some(h);
        }
        if let Some(m) = style.margin {
            result.margin = Some(m);
        }
        if let Some(p) = style.padding {
            result.padding = Some(p);
        }
        if let Some(ta) = style.text_align {
            result.text_align = Some(ta);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_named() {
        let style = InlineStyle::parse("color: red");
        assert_eq!(style.color, Some(Color::new(255, 0, 0)));
    }

    #[test]
    fn test_parse_color_hex() {
        let style = InlineStyle::parse("color: #ff0000");
        assert_eq!(style.color, Some(Color::new(255, 0, 0)));

        let style2 = InlineStyle::parse("color: #f00");
        assert_eq!(style2.color, Some(Color::new(255, 0, 0)));
    }

    #[test]
    fn test_parse_font_size() {
        let style = InlineStyle::parse("font-size: 16px");
        assert_eq!(style.font_size, Some(16));

        let style2 = InlineStyle::parse("font-size: 24");
        assert_eq!(style2.font_size, Some(24));
    }

    #[test]
    fn test_parse_multiple() {
        let style = InlineStyle::parse("color: blue; font-size: 20px; background-color: #ffff00");
        assert_eq!(style.color, Some(Color::new(0, 0, 255)));
        assert_eq!(style.font_size, Some(20));
        assert_eq!(style.background_color, Some(Color::new(255, 255, 0)));
    }

    #[test]
    fn test_parse_external_css() {
        let css = "p { color: red; } .myclass { font-size: 20px; }";
        let sheet = Stylesheet::parse(css);
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].selector, Selector::Element("p".to_string()));
        assert_eq!(sheet.rules[1].selector, Selector::Class("myclass".to_string()));
    }
}
