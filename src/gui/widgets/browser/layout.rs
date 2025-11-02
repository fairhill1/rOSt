/// Layout engine for browser - handles HTML DOM tree layout and rendering boxes
///
/// This module contains all layout-related functions extracted from the browser widget.
/// It's responsible for converting the DOM tree into a list of positioned layout boxes
/// that can be rendered to the screen.

use crate::gui::html_parser::{Parser, Node, NodeType, ElementData};
use crate::gui::css_parser::{InlineStyle, Selector, SimpleSelector};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

use super::{Browser, LayoutBox, Color, PendingImage};

const CHAR_WIDTH: usize = 8;
const CHAR_HEIGHT: usize = 8;

/// Ancestor chain for matching descendant selectors
#[derive(Debug, Clone)]
pub struct Ancestor {
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

/// Check if selector matches element with given ancestors
fn selector_matches(selector: &Selector, tag: &str, classes: &[&str], id: Option<&str>, ancestors: &[Ancestor]) -> bool {
    match selector {
        Selector::Simple(simple) => simple.matches(tag, classes, id),
        Selector::Descendant(parts) => {
            if parts.is_empty() {
                return false;
            }

            // Last part must match current element
            let last = &parts[parts.len() - 1];
            if !last.matches(tag, classes, id) {
                return false;
            }

            // If only one part, we're done
            if parts.len() == 1 {
                return true;
            }

            // Earlier parts must match ancestors (in order, but can skip ancestors)
            let mut ancestor_idx = ancestors.len(); // Start from end (most recent ancestor)
            for part in parts[..parts.len() - 1].iter().rev() {
                // Find an ancestor that matches this part
                let mut found = false;
                while ancestor_idx > 0 {
                    ancestor_idx -= 1;
                    let anc = &ancestors[ancestor_idx];
                    let anc_classes: Vec<&str> = anc.classes.iter().map(|s| s.as_str()).collect();
                    if part.matches(&anc.tag, &anc_classes, anc.id.as_deref()) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
            true
        }
    }
}

/// Get the actual font size in pixels based on heading level
fn get_font_size_px(font_size_level: usize) -> f32 {
    // When using TTF, use real pixel sizes
    // When using bitmap, use multipliers of 8px
    if crate::gui::font::is_available() {
        match font_size_level {
            5 => 36.0,  // h1: large
            4 => 28.0,  // h2: medium-large
            3 => 24.0,  // h3: medium
            2 => 20.0,  // h4-h6: slightly larger than body
            1 => 18.0,  // body text
            _ => 18.0,
        }
    } else {
        // Bitmap font - return multiplier * 8
        (font_size_level * 8) as f32
    }
}

/// Extract title from DOM tree
pub fn extract_title(node: &Node) -> Option<String> {
    match &node.node_type {
        NodeType::Element(elem) => {
            if elem.tag_name == "title" {
                // Found title element - extract text from children
                for child in &node.children {
                    if let NodeType::Text(text) = &child.node_type {
                        return Some(text.trim().to_string());
                    }
                }
            }
            // Recursively search children
            for child in &node.children {
                if let Some(title) = extract_title(child) {
                    return Some(title);
                }
            }
        }
        _ => {}
    }
    None
}

/// Extract CSS stylesheet URLs from <link rel="stylesheet" href="..."> tags
pub fn extract_css_urls(node: &Node, base_url: &str) -> Vec<String> {
    let mut urls = Vec::new();

    match &node.node_type {
        NodeType::Element(elem) => {
            // Check if this is a stylesheet link
            if elem.tag_name == "link" {
                if let Some(rel) = elem.attributes.get("rel") {
                    if rel.to_lowercase().contains("stylesheet") {
                        if let Some(href) = elem.attributes.get("href") {
                            // Resolve relative URLs
                            let css_url = if href.starts_with("http://") || href.starts_with("https://") {
                                href.clone()
                            } else if href.starts_with('/') {
                                // Absolute path - use current host
                                let (host, port, _) = super::http::parse_url(base_url);
                                alloc::format!("http://{}:{}{}", host, port, href)
                            } else {
                                // Relative path - append to current URL's directory
                                let base = if let Some(last_slash) = base_url.rfind('/') {
                                    &base_url[..last_slash]
                                } else {
                                    base_url
                                };
                                alloc::format!("{}/{}", base, href)
                            };

                            crate::kernel::uart_write_string(&alloc::format!("Found CSS link: {}\r\n", css_url));
                            urls.push(css_url);
                        }
                    }
                }
            }

            // Recursively search children
            for child in &node.children {
                urls.extend(extract_css_urls(child, base_url));
            }
        }
        _ => {}
    }

    urls
}

/// Extract inline CSS from <style> tags
pub fn extract_inline_css(node: &Node) -> Vec<String> {
    let mut css_blocks = Vec::new();

    match &node.node_type {
        NodeType::Element(elem) => {
            // Check if this is a style tag
            if elem.tag_name == "style" {
                // Extract text content from children
                for child in &node.children {
                    if let NodeType::Text(text) = &child.node_type {
                        if !text.trim().is_empty() {
                            crate::kernel::uart_write_string(&alloc::format!("Found inline <style>: {} bytes\r\n", text.len()));
                            css_blocks.push(text.clone());
                        }
                    }
                }
            }

            // Recursively search children
            for child in &node.children {
                css_blocks.extend(extract_inline_css(child));
            }
        }
        _ => {}
    }

    css_blocks
}

/// Load HTML content
pub fn load_html(browser: &mut Browser, html: String) {
    crate::kernel::uart_write_string("load_html: Starting HTML parsing\r\n");
    let mut parser = Parser::new(html);
    let dom = parser.parse();

    crate::kernel::uart_write_string("load_html: HTML parsed, clearing layout\r\n");

    // Debug: Print DOM structure
    debug_print_dom(&dom, 0);

    browser.layout.clear();
    browser.stylesheets.clear(); // Clear previous stylesheets

    // Extract page title from DOM
    browser.page_title = extract_title(&dom);

    // Update window title
    if let Some(ref title) = browser.page_title {
        let window_title = alloc::format!("Browser - {}", title);
        crate::gui::window_manager::set_browser_window_title(browser.instance_id, &window_title);
    }

    // Extract and parse inline <style> tags
    let inline_css_blocks = extract_inline_css(&dom);
    for css_text in inline_css_blocks {
        let stylesheet = crate::gui::css_parser::Stylesheet::parse(&css_text);
        crate::kernel::uart_write_string(&alloc::format!("Parsed inline <style>: {} rules\r\n", stylesheet.rules.len()));
        browser.stylesheets.push(stylesheet);
    }

    // Extract and queue CSS files for loading
    let css_urls = extract_css_urls(&dom, &browser.url);
    for css_url in css_urls {
        browser.pending_css.push(super::PendingCss { url: css_url });
    }

    // Layout the DOM tree - search for <body> element
    crate::kernel::uart_write_string("load_html: Starting layout\r\n");

    // Find and layout the <body> element (it might be nested in malformed HTML)
    // Use wider layout width to accommodate larger windows (most common is 1280px)
    find_and_layout_body(browser, &dom, 10, 10, 1260);

    crate::kernel::uart_write_string(&alloc::format!("load_html: Layout complete, {} layout boxes created\r\n", browser.layout.len()));

    // Store the DOM after layout
    browser.dom = Some(dom);
}

/// Debug helper to print DOM structure
pub fn debug_print_dom(node: &Node, depth: usize) {
    let indent = "  ".repeat(depth);
    match &node.node_type {
        NodeType::Element(elem) => {
            crate::kernel::uart_write_string(&alloc::format!("{}Element: <{}> ({} children)\r\n",
                indent, elem.tag_name, node.children.len()));
            for child in &node.children {
                debug_print_dom(child, depth + 1);
            }
        }
        NodeType::Text(text) => {
            let preview = if text.len() > 40 { &text[..40] } else { text };
            crate::kernel::uart_write_string(&alloc::format!("{}Text: \"{}\"\r\n", indent, preview));
        }
    }
}

/// Find and layout the <body> element, wherever it is in the DOM
pub fn find_and_layout_body(browser: &mut Browser, node: &Node, x: usize, y: usize, max_width: usize) {
    match &node.node_type {
        NodeType::Element(elem) => {
            if elem.tag_name == "body" {
                // Found the body! Layout it (which will recursively layout its children)
                crate::kernel::uart_write_string("find_and_layout_body: Found <body> element\r\n");
                // Body text uses font_size_level = 1 (18px TTF / 8px bitmap)
                layout_node(browser, node, x, y, max_width, &Color::BLACK, &None, false, false, 1, "", &[]);

                // Add bottom padding (spacer box at end of page)
                if let Some(last_box) = browser.layout.last() {
                    let bottom_padding_y = last_box.y + last_box.height;
                    browser.layout.push(LayoutBox {
                        x: 10,
                        y: bottom_padding_y,
                        width: 1,
                        height: 25, // 25px tall spacer creates bottom padding
                        text: String::new(),
                        color: Color::new(255, 255, 255), // White (invisible on white bg)
                        background_color: None,
                        font_size: 1,
                        is_link: false,
                        link_url: String::new(),
                        bold: false,
                        italic: false,
                        element_id: String::new(),
                        is_image: false,
                        image_data: None,
                        is_hr: false,
                        is_table_cell: false,
                        is_header_cell: false,
                    });
                }
                return;
            }
            // Not body, recurse into children to find it
            for child in &node.children {
                find_and_layout_body(browser, child, x, y, max_width);
            }
        }
        NodeType::Text(_) => {
            // Text nodes can't contain body
        }
    }
}

/// Recursive layout function
pub fn layout_node(
    browser: &mut Browser,
    node: &Node,
    x: usize,
    y: usize,
    max_width: usize,
    color: &Color,
    background_color: &Option<Color>,
    bold: bool,
    italic: bool,
    font_size: usize,
    element_id: &str,
    ancestors: &[Ancestor],
) -> (usize, usize) {
    match &node.node_type {
        NodeType::Text(text) => {
            if text.is_empty() || text.trim().is_empty() {
                return (x, y);
            }

            // Word wrap
            let words: Vec<&str> = text.split_whitespace().collect();
            let mut current_x = x;
            let mut current_y = y;

            // Calculate dimensions based on font type
            let (char_width, char_height) = if crate::gui::font::is_available() {
                let font_size_px = get_font_size_px(font_size);
                let space_width = crate::gui::font::measure_string(" ", font_size_px) as usize;
                let height = crate::gui::font::get_char_height() as usize;
                (space_width, height)
            } else {
                (CHAR_WIDTH * font_size, CHAR_HEIGHT * font_size)
            };

            for word in words {
                // Measure actual word width
                let word_width = if crate::gui::font::is_available() {
                    let font_size_px = get_font_size_px(font_size);
                    let text_with_space = alloc::format!("{} ", word);
                    crate::gui::font::measure_string(&text_with_space, font_size_px) as usize
                } else {
                    (word.len() + 1) * char_width
                };

                // Check if word fits on current line
                if current_x + word_width > max_width && current_x > x {
                    current_x = x;
                    current_y += char_height + 2;
                }

                // Add layout box for word
                browser.layout.push(LayoutBox {
                    x: current_x,
                    y: current_y,
                    width: word_width,
                    height: char_height,
                    text: word.to_string() + " ",
                    color: *color,
                    background_color: *background_color,
                    font_size,
                    is_link: false,
                    link_url: String::new(),
                    bold,
                    italic,
                    element_id: element_id.to_string(),
                    is_image: false,
                    image_data: None,
                    is_hr: false,
                    is_table_cell: false,
                    is_header_cell: false,
                });

                current_x += word_width;
            }

            (current_x, current_y)
        }
        NodeType::Element(elem) => {
            layout_element(browser, node, elem, x, y, max_width, color, bold, italic, font_size, element_id, ancestors)
        }
    }
}

/// Layout an element
pub fn layout_element(
    browser: &mut Browser,
    node: &Node,
    elem: &ElementData,
    x: usize,
    y: usize,
    max_width: usize,
    parent_color: &Color,
    parent_bold: bool,
    parent_italic: bool,
    parent_font_size: usize,
    parent_element_id: &str,
    ancestors: &[Ancestor],
) -> (usize, usize) {
    let tag = elem.tag_name.as_str();

    // Skip rendering <head> and its contents
    if tag == "head" {
        return (x, y);
    }

    // Extract element ID from attributes if present
    let element_id = elem.attributes.get("id")
        .map(|s| s.as_str())
        .unwrap_or(parent_element_id);

    // Extract class attribute and split into classes
    let class_attr = elem.attributes.get("class").map(|s| s.as_str()).unwrap_or("");
    let classes: Vec<&str> = class_attr.split_whitespace().collect();

    // Match stylesheet rules for this element
    let mut matched_styles: Vec<(&Selector, &InlineStyle)> = Vec::new();
    for stylesheet in &browser.stylesheets {
        for rule in &stylesheet.rules {
            if selector_matches(&rule.selector, tag, &classes, Some(element_id).filter(|s| !s.is_empty()), ancestors) {
                matched_styles.push((&rule.selector, &rule.style));
            }
        }
    }

    // Parse inline CSS styles if present
    let inline_style_raw = elem.attributes.get("style")
        .map(|s| InlineStyle::parse(s))
        .unwrap_or_default();

    // Merge styles: base (default) + stylesheet matches + inline styles
    let base_style = InlineStyle::default();
    let merged_from_sheets = crate::gui::css_parser::merge_styles(base_style, &matched_styles);

    // Inline styles have highest priority, so merge them last
    let inline_overrides: Vec<(&Selector, &InlineStyle)> = Vec::new(); // Empty, we'll apply inline directly
    let inline_style = if inline_style_raw.color.is_some() || inline_style_raw.background_color.is_some()
        || inline_style_raw.font_size.is_some() || inline_style_raw.margin.is_some()
        || inline_style_raw.padding.is_some() || inline_style_raw.display.is_some() {
        // Merge inline over stylesheet
        let mut result = merged_from_sheets.clone();
        if let Some(c) = inline_style_raw.color { result.color = Some(c); }
        if let Some(bg) = inline_style_raw.background_color { result.background_color = Some(bg); }
        if let Some(fs) = inline_style_raw.font_size { result.font_size = Some(fs); }
        if let Some(m) = inline_style_raw.margin { result.margin = Some(m); }
        if let Some(p) = inline_style_raw.padding { result.padding = Some(p); }
        if let Some(ta) = inline_style_raw.text_align { result.text_align = Some(ta); }
        if let Some(d) = inline_style_raw.display { result.display = Some(d); }
        result
    } else {
        merged_from_sheets
    };

    // If display:none, skip rendering this element entirely
    if let Some(crate::gui::css_parser::Display::None) = inline_style.display {
        return (x, y);
    }

    let mut current_x = x;
    let mut current_y = y;

    // Block-level elements start on new line
    let is_block = matches!(tag,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
        "p" | "div" |
        "ul" | "ol" | "li" |
        "hr" | "table" |
        // HTML5 semantic elements
        "header" | "footer" | "nav" | "section" | "article" | "aside" | "main" |
        "figure" | "figcaption" | "blockquote" | "pre"
    );
    if is_block && !browser.layout.is_empty() {
        current_x = x;
        // Use whichever is lower on page: explicit spacing from parent (y) or end of last element
        // No hardcoded spacing - CSS margin/padding controls all spacing
        let default_y = browser.layout.last().map(|b| b.y + b.height).unwrap_or(y);
        current_y = default_y.max(y);
    }

    // Determine color, style, and font size level - CSS can override
    let color = inline_style.color.as_ref().unwrap_or(parent_color);
    let bold = parent_bold || matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong");
    let italic = parent_italic || matches!(tag, "i" | "em" | "cite");

    // <code> and <pre> could use monospace font in future, for now just render normally

    // Determine font size - tag-based defaults, potentially overridden by CSS
    let default_font_size_level = match tag {
        "h1" => 5,  // 36px TTF / 40px bitmap
        "h2" => 4,  // 28px TTF / 32px bitmap
        "h3" => 3,  // 24px TTF / 24px bitmap
        "h4" => 2,  // 20px TTF / 16px bitmap
        "h5" => 2,  // 20px TTF / 16px bitmap
        "h6" => 2,  // 20px TTF / 16px bitmap
        _ => parent_font_size,
    };

    // Apply CSS font-size if specified (convert px to level: px / 8 for bitmap, use directly for TTF)
    let font_size_level = if let Some(css_font_size) = inline_style.font_size {
        if crate::gui::font::is_available() {
            // TTF: use pixel size directly (will be applied in render)
            (css_font_size / 8).max(1)  // Rough approximation for now
        } else {
            // Bitmap: convert to multiplier (8px = 1, 16px = 2, etc.)
            (css_font_size / 8).max(1)
        }
    } else {
        default_font_size_level
    };

    // Store background color and spacing from CSS
    let background_color = inline_style.background_color;

    // Default margins for certain elements (like browsers do)
    let default_margin = match tag {
        "p" => 8,  // Paragraphs get default top/bottom margin
        _ => 0,
    };

    let css_margin = inline_style.margin.unwrap_or(default_margin);
    let css_padding = inline_style.padding.unwrap_or(0);

    // Get actual height for spacing calculations
    let element_height = if crate::gui::font::is_available() {
        crate::gui::font::get_char_height() as usize
    } else {
        CHAR_HEIGHT * font_size_level
    };

    // Apply margin (spacing before element) - this creates white space BEFORE the element
    if is_block && css_margin > 0 {
        current_y += css_margin;
    }

    // Track starting position for full-width backgrounds (AFTER margin applied)
    let block_start_y = current_y;
    let block_start_idx = browser.layout.len();

    // Handle special tags
    match tag {
        "br" => {
            // Line break - just move down one line (no extra spacing)
            return (x, current_y + element_height);
        }
        "hr" => {
            // Horizontal rule - draw a solid pixel line across the page
            // Add spacing before
            if !browser.layout.is_empty() {
                current_y += element_height + 4;
            }

            // Create HR layout box - will be rendered as actual pixel line
            let line_width = max_width.saturating_sub(20); // Leave 10px margin on each side

            browser.layout.push(LayoutBox {
                x: x + 10,
                y: current_y,
                width: line_width,
                height: 2, // 2px thick line
                text: String::new(),
                color: Color::new(180, 180, 180), // Light gray
                background_color: None,
                font_size: font_size_level,
                is_link: false,
                link_url: String::new(),
                bold: false,
                italic: false,
                element_id: element_id.to_string(),
                is_image: false,
                image_data: None,
                is_hr: true, // Mark as HR for pixel rendering
                is_table_cell: false,
                is_header_cell: false,
            });

            // Add spacing after
            current_y += element_height + 4;
            return (x, current_y);
        }
        "img" => {
            // Image tag - fetch and display image
            if let Some(src) = elem.attributes.get("src") {
                crate::kernel::uart_write_string(&alloc::format!("layout_element: Found <img src=\"{}\">\r\n", src));

                // Parse the image URL (resolve relative URLs)
                let img_url = if src.starts_with("http://") || src.starts_with("https://") {
                    src.clone()
                } else if src.starts_with('/') {
                    // Absolute path - use current host
                    let (host, port, _) = super::http::parse_url(&browser.url);
                    alloc::format!("http://{}:{}{}", host, port, src)
                } else {
                    // Relative path - append to current URL's directory
                    let base_url = if let Some(last_slash) = browser.url.rfind('/') {
                        &browser.url[..last_slash]
                    } else {
                        &browser.url
                    };
                    alloc::format!("{}/{}", base_url, src)
                };

                crate::kernel::uart_write_string(&alloc::format!("layout_element: Queuing async load for: {}\r\n", img_url));

                // Parse width/height attributes if present (prevents layout reflow)
                let img_width = elem.attributes.get("width")
                    .and_then(|w| w.parse::<usize>().ok())
                    .unwrap_or(0); // Default 0px if not specified (will reflow when image loads)
                let img_height = elem.attributes.get("height")
                    .and_then(|h| h.parse::<usize>().ok())
                    .unwrap_or(0);

                // Add spacing before image if needed
                if !browser.layout.is_empty() {
                    current_y += 4;
                }

                // Check if image is already cached
                let cached_image = browser.image_cache.get(&img_url).cloned();

                // If no width/height specified and we have cached image, use its dimensions
                let (final_width, final_height) = if img_width == 0 && img_height == 0 {
                    if let Some(ref img) = cached_image {
                        (img.width as usize, img.height as usize)
                    } else {
                        (0, 0) // Unknown size, will reflow when loaded
                    }
                } else {
                    (img_width, img_height)
                };

                // Create layout box for image
                browser.layout.push(LayoutBox {
                    x: current_x,
                    y: current_y,
                    width: final_width,
                    height: final_height,
                    text: if cached_image.is_some() { String::new() } else { String::from("[Loading image...]") },
                    color: Color::new(128, 128, 128),
                    background_color: None,
                    font_size: font_size_level,
                    is_link: false,
                    link_url: String::new(),
                    bold: false,
                    italic: false,
                    element_id: element_id.to_string(),
                    is_image: true,
                    image_data: cached_image.clone(),
                    is_hr: false,
                    is_table_cell: false,
                    is_header_cell: false,
                });

                // Only queue async load if not cached
                if cached_image.is_none() {
                    let layout_box_index = browser.layout.len() - 1;
                    browser.pending_images.push(PendingImage {
                        url: img_url,
                        layout_box_index,
                    });
                }

                // Move to next line after image
                current_y += browser.layout.last().unwrap().height + 4;
                return (x, current_y);
            }
            return (current_x, current_y);
        }
        "a" => {
            // Hyperlink - render children with link color
            let link_url = elem.attributes.get("href").cloned().unwrap_or_default();
            let link_color = Color::new(0, 0, 255); // Blue

            // Build ancestor chain for link children
            let mut link_ancestors = ancestors.to_vec();
            link_ancestors.push(Ancestor {
                tag: tag.to_string(),
                classes: classes.iter().map(|s| s.to_string()).collect(),
                id: if element_id.is_empty() { None } else { Some(element_id.to_string()) },
            });

            for child in &node.children {
                let start_idx = browser.layout.len();
                let (new_x, new_y) = layout_node(browser, child, current_x, current_y, max_width, &link_color, &background_color, bold, italic, font_size_level, element_id, &link_ancestors);

                // Mark all boxes created for this link
                for i in start_idx..browser.layout.len() {
                    browser.layout[i].is_link = true;
                    browser.layout[i].link_url = link_url.clone();
                }

                current_x = new_x;
                current_y = new_y;
            }
            return (current_x, current_y);
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            // Headings - larger font size with proportional spacing
            // Only add spacing before if there's already content above
            if !browser.layout.is_empty() {
                current_y += element_height; // Extra spacing before heading
            }
            // Don't early return - let headings use full-width background handling below
        }
        "ul" | "ol" => {
            // Lists - use small fixed indent to prevent excessive nesting
            const LIST_INDENT: usize = 32; // Small fixed indent per nesting level

            // Add extra spacing before nested lists (x > 10 means we're indented)
            if !browser.layout.is_empty() && x > 10 {
                current_y += element_height / 2; // Extra space before nested list
            }

            for (i, child) in node.children.iter().enumerate() {
                // Save the starting Y position for this list item
                let list_item_y = current_y;

                // Determine nesting level based on x position
                let nesting_level = if current_x <= 10 {
                    0
                } else {
                    (current_x - 10) / LIST_INDENT
                };

                // Add bullet or number (different bullets for different nesting levels)
                let bullet = if tag == "ul" {
                    match nesting_level {
                        0 => "• ",      // Filled bullet (U+2022)
                        1 => "◦ ",      // White circle (U+25E6)
                        _ => "▪ ",      // Small square (U+25AA)
                    }
                } else {
                    &alloc::format!("{}. ", i + 1)
                };
                let bullet_width = if crate::gui::font::is_available() {
                    let font_size_px = get_font_size_px(font_size_level);
                    crate::gui::font::measure_string(&bullet, font_size_px) as usize
                } else {
                    bullet.len() * CHAR_WIDTH * font_size_level
                };

                // Build ancestor chain for list items
                let mut list_ancestors = ancestors.to_vec();
                list_ancestors.push(Ancestor {
                    tag: tag.to_string(),
                    classes: classes.iter().map(|s| s.to_string()).collect(),
                    id: if element_id.is_empty() { None } else { Some(element_id.to_string()) },
                });

                // Layout the list item content first to get its starting position
                let content_start_idx = browser.layout.len();
                let (_, new_y) = layout_node(browser, child, current_x + LIST_INDENT, list_item_y, max_width - LIST_INDENT, color, &background_color, bold, italic, font_size_level, element_id, &list_ancestors);

                // Find the Y position where the content actually started
                let content_y = if browser.layout.len() > content_start_idx {
                    browser.layout[content_start_idx].y
                } else {
                    list_item_y
                };

                // Now add the bullet at the same Y position as the content
                browser.layout.insert(content_start_idx, LayoutBox {
                    x: current_x,
                    y: content_y,
                    width: bullet_width,
                    height: element_height,
                    text: bullet.to_string(),
                    color: *color,
                    background_color: None,
                    font_size: font_size_level,
                    is_link: false,
                    link_url: String::new(),
                    bold,
                    italic,
                    element_id: element_id.to_string(),
                    is_image: false,
                    image_data: None,
                    is_hr: false,
                    is_table_cell: false,
                    is_header_cell: false,
                });

                current_y = new_y + element_height + 2;
            }
            // Remove the spacing after the last item (block element will add its own spacing)
            current_y -= element_height + 2;
            return (x, current_y);
        }
        "table" => {
            // Tables - parse rows and cells, layout in grid
            const CELL_PADDING: usize = 8; // Padding inside cells
            const BORDER_WIDTH: usize = 1; // Border thickness

            // Add spacing before table
            if !browser.layout.is_empty() {
                current_y += element_height + 4;
            }

            // First pass: collect rows and determine column count
            let mut rows: Vec<Vec<&Node>> = Vec::new();
            let mut max_cols = 0;

            for child in &node.children {
                if let NodeType::Element(child_elem) = &child.node_type {
                    if child_elem.tag_name == "tr" {
                        let mut cells: Vec<&Node> = Vec::new();
                        for cell in &child.children {
                            if let NodeType::Element(cell_elem) = &cell.node_type {
                                if cell_elem.tag_name == "td" || cell_elem.tag_name == "th" {
                                    cells.push(cell);
                                }
                            }
                        }
                        max_cols = max_cols.max(cells.len());
                        rows.push(cells);
                    }
                }
            }

            if rows.is_empty() {
                return (x, current_y);
            }

            // Calculate column width (equal width for all columns)
            let table_width = max_width.saturating_sub(20); // Leave margins
            let col_width = if max_cols > 0 {
                table_width / max_cols
            } else {
                100
            };

            let table_x = x + 10;
            let mut table_y = current_y;

            // Second pass: layout cells
            for row_cells in &rows {
                let row_start_y = table_y;
                let mut row_height = 0;

                // Layout all cells in this row first to determine row height
                let mut cell_layouts: Vec<(usize, usize, Vec<LayoutBox>)> = Vec::new();

                for (col_idx, cell) in row_cells.iter().enumerate() {
                    let cell_x = table_x + col_idx * col_width;
                    let content_x = cell_x + CELL_PADDING;
                    let content_y = row_start_y + CELL_PADDING;
                    let content_width = col_width.saturating_sub(CELL_PADDING * 2);

                    // Check if this is a header cell
                    let is_header = if let NodeType::Element(cell_elem) = &cell.node_type {
                        cell_elem.tag_name == "th"
                    } else {
                        false
                    };

                    // Save layout state
                    let layout_start = browser.layout.len();

                    // Build ancestor chain for table cells
                    let mut table_ancestors = ancestors.to_vec();
                    table_ancestors.push(Ancestor {
                        tag: tag.to_string(),
                        classes: classes.iter().map(|s| s.to_string()).collect(),
                        id: if element_id.is_empty() { None } else { Some(element_id.to_string()) },
                    });

                    // Layout cell content
                    let cell_bold = bold || is_header;
                    for cell_child in &cell.children {
                        layout_node(browser, cell_child, content_x, content_y, content_width, color, &background_color, cell_bold, italic, font_size_level, element_id, &table_ancestors);
                    }

                    // Calculate cell content height
                    let mut cell_height = CELL_PADDING * 2; // Min height with padding
                    if browser.layout.len() > layout_start {
                        let min_y = browser.layout[layout_start].y;
                        let max_y = browser.layout[layout_start..].iter()
                            .map(|b| b.y + b.height)
                            .max()
                            .unwrap_or(content_y);
                        cell_height = cell_height.max(max_y - min_y + CELL_PADDING * 2);
                    }

                    row_height = row_height.max(cell_height);

                    // Store cell layout info
                    let cell_boxes: Vec<LayoutBox> = browser.layout[layout_start..].iter().cloned().collect();
                    browser.layout.truncate(layout_start); // Remove temporarily
                    cell_layouts.push((cell_x, cell_height, cell_boxes));
                }

                // Now add all cells with correct row height
                for (col_idx, (cell_x, _, cell_boxes)) in cell_layouts.iter().enumerate() {
                    let cell = row_cells[col_idx];
                    let is_header = if let NodeType::Element(cell_elem) = &cell.node_type {
                        cell_elem.tag_name == "th"
                    } else {
                        false
                    };

                    // Create cell background box
                    let bg_color = if is_header {
                        Color::new(230, 230, 230) // Light gray for headers
                    } else {
                        Color::new(255, 255, 255) // White for cells
                    };

                    browser.layout.push(LayoutBox {
                        x: *cell_x,
                        y: row_start_y,
                        width: col_width,
                        height: row_height,
                        text: String::new(),
                        color: bg_color,
                        background_color: None,
                        font_size: font_size_level,
                        is_link: false,
                        link_url: String::new(),
                        bold: false,
                        italic: false,
                        element_id: element_id.to_string(),
                        is_image: false,
                        image_data: None,
                        is_hr: false,
                        is_table_cell: true,
                        is_header_cell: is_header,
                    });

                    // Add cell content boxes back
                    browser.layout.extend(cell_boxes.clone());
                }

                table_y += row_height;
            }

            current_y = table_y + element_height;
            return (x, current_y);
        }
        _ => {}
    }

    // Apply padding to content position (only for block elements - inline padding doesn't shift baseline)
    let content_x = if css_padding > 0 && is_block { current_x + css_padding } else { current_x };
    let content_y = if css_padding > 0 && is_block { current_y + css_padding } else { current_y };
    let content_max_width = if css_padding > 0 && is_block { max_width.saturating_sub(css_padding * 2) } else { max_width };

    if css_padding > 0 && is_block {
        current_x = content_x;
        current_y = content_y;
    }

    // Build new ancestor chain for children
    let mut new_ancestors = ancestors.to_vec();
    new_ancestors.push(Ancestor {
        tag: tag.to_string(),
        classes: classes.iter().map(|s| s.to_string()).collect(),
        id: if element_id.is_empty() { None } else { Some(element_id.to_string()) },
    });

    // Render children
    for child in &node.children {
        // For block-level children (like nested lists), pass the base x position
        // For inline children (like text), pass current_x (continues on same line)
        // Special case: <br> needs base x to reset to left margin
        let child_is_block = if let NodeType::Element(child_elem) = &child.node_type {
            matches!(child_elem.tag_name.as_str(),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
                "p" | "div" |
                "ul" | "ol" | "li" |
                "hr" | "table" |
                // HTML5 semantic elements
                "header" | "footer" | "nav" | "section" | "article" | "aside" | "main" |
                "figure" | "figcaption" | "blockquote" | "pre"
            )
        } else {
            false
        };

        let is_br = if let NodeType::Element(child_elem) = &child.node_type {
            child_elem.tag_name == "br"
        } else {
            false
        };

        let child_base_x = if css_padding > 0 && is_block { content_x } else { x };
        let child_x = if child_is_block || is_br { child_base_x } else { current_x };
        let (new_x, new_y) = layout_node(browser, child, child_x, current_y, content_max_width, color, &background_color, bold, italic, font_size_level, element_id, &new_ancestors);
        current_x = new_x;
        current_y = new_y;
    }

    // Add bottom padding (only for block elements)
    if css_padding > 0 && is_block {
        current_y += css_padding;
    }

    // Add full-width background for block elements with background color
    if is_block && background_color.is_some() {
        let bg_color = background_color.unwrap();

        // Calculate the actual height of the block content
        // If we have child boxes, use their max Y
        // If no child boxes (only block children), use current_y
        let mut block_end_y = block_start_y;
        if browser.layout.len() > block_start_idx {
            for i in block_start_idx..browser.layout.len() {
                let box_end = browser.layout[i].y + browser.layout[i].height;
                if box_end > block_end_y {
                    block_end_y = box_end;
                }
            }
        } else {
            // No child boxes created - use current_y as end
            block_end_y = current_y;
        }

        // Add padding if specified (top padding already in positions, add bottom padding to height)
        let block_height = if css_padding > 0 {
            block_end_y.saturating_sub(block_start_y) + css_padding + 6
        } else {
            block_end_y.saturating_sub(block_start_y) + 6
        };

        // Clear background_color from child text boxes (full-width bg will handle it)
        // But keep backgrounds on nested block elements (empty text = background box)
        for i in block_start_idx..browser.layout.len() {
            if !browser.layout[i].text.is_empty() {
                browser.layout[i].background_color = None;
            }
        }

        // Insert full-width background box at the beginning (so it renders behind content)
        browser.layout.insert(block_start_idx, LayoutBox {
            x,
            y: block_start_y,
            width: max_width,
            height: block_height,
            text: String::new(),
            color: bg_color,
            background_color: Some(bg_color),
            font_size: font_size_level,
            is_link: false,
            link_url: String::new(),
            bold: false,
            italic: false,
            element_id: element_id.to_string(),
            is_image: false,
            image_data: None,
            is_hr: false,
            is_table_cell: false,
            is_header_cell: false,
        });
    }

    // Block elements end with newline
    if is_block {
        // Only add CSS margin if specified - no hardcoded spacing
        // Exception: headings get minimal spacing for readability
        let bottom_spacing = if css_margin > 0 {
            css_margin
        } else if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            8  // Small spacing after headings for readability
        } else {
            0  // No spacing - CSS controls all layout
        };
        (x, current_y + bottom_spacing)
    } else {
        (current_x, current_y)
    }
}
