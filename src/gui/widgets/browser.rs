/// Web browser for rOSt
/// Features: HTML rendering, address bar, hyperlinks, scrolling

use crate::gui::html_parser::{Parser, Node, NodeType, ElementData};
use crate::gui::framebuffer::FONT_8X8;
use crate::gui::widgets::text_input::TextInput;
use crate::gui::bmp_decoder::BmpImage;
use crate::gui::bmp_decoder::decode_bmp;
use crate::gui::png_decoder::decode_png;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

/// Global list of browser instances
static mut BROWSERS: Vec<Browser> = Vec::new();

const CHAR_WIDTH: usize = 8;
const CHAR_HEIGHT: usize = 8;

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

/// Find the end of HTTP headers in binary data
/// Returns (start_of_separator, length_of_separator)
fn find_header_end(data: &[u8]) -> Option<(usize, usize)> {
    // Look for \r\n\r\n
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == b'\r' && data[i+1] == b'\n' && data[i+2] == b'\r' && data[i+3] == b'\n' {
            return Some((i, 4));
        }
    }
    // Look for \n\n
    for i in 0..data.len().saturating_sub(1) {
        if data[i] == b'\n' && data[i+1] == b'\n' {
            return Some((i, 2));
        }
    }
    None
}

/// Simple color structure
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    pub const BLACK: Color = Color::new(0, 0, 0);

    pub fn to_u32(&self) -> u32 {
        0xFF000000 | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub text: String,
    pub color: Color,
    pub font_size: usize, // Multiplier for 8x8 font (1=8px, 2=16px, etc.)
    pub is_link: bool,
    pub link_url: String,
    pub bold: bool,
    pub italic: bool,
    pub element_id: String, // HTML element ID attribute
    pub is_image: bool,
    pub image_data: Option<BmpImage>,
}

pub struct Browser {
    pub url: String,
    pub url_input: TextInput,
    pub url_focused: bool,
    pub dom: Option<Node>,
    pub layout: Vec<LayoutBox>,
    pub scroll_offset: usize,
    pub history: Vec<String>,
    pub history_index: usize,
    pub loading: bool,
    pub page_title: Option<String>,
    pub instance_id: usize, // ID for updating window title
}

impl Browser {
    pub fn new(instance_id: usize) -> Self {
        Browser {
            url: String::from("about:blank"),
            url_input: TextInput::new(),
            url_focused: false,
            dom: None,
            layout: Vec::new(),
            scroll_offset: 0,
            history: Vec::new(),
            history_index: 0,
            loading: false,
            page_title: None,
            instance_id,
        }
    }

    /// Navigate to a URL
    pub fn navigate(&mut self, url: String) {
        // Add to history
        if self.history_index < self.history.len() {
            self.history.truncate(self.history_index);
        }
        self.history.push(url.clone());
        self.history_index = self.history.len();

        self.url = url.clone();
        self.url_input.set_text(&url);
        self.scroll_offset = 0;
        self.loading = true;

        // Handle special URLs
        if url.starts_with("about:") {
            self.load_about_page(&url);
            return;
        }

        // Show loading page first
        self.load_html("<html><body><h1>Loading...</h1><p>Please wait while the page loads. This may take a few seconds.</p></body></html>".to_string());

        crate::kernel::uart_write_string(&alloc::format!("Browser: Navigating to {}\r\n", url));

        // Parse URL to get host, port, path
        let (host, port, path) = self.parse_url(&url);

        crate::kernel::uart_write_string(&alloc::format!("Browser: Host={}, Port={}, Path={}\r\n", host, port, path));

        // Make HTTP request
        match self.http_get(&host, port, &path) {
            Some(html) => {
                crate::kernel::uart_write_string(&alloc::format!("Browser: HTTP request succeeded, HTML length={}\r\n", html.len()));
                crate::kernel::uart_write_string(&alloc::format!("Browser: HTML content:\r\n{}\r\n", html));
                self.load_html(html);
            }
            None => {
                crate::kernel::uart_write_string("Browser: HTTP request failed\r\n");
                self.load_error_page("HTTP request failed. Check network connection and URL.");
            }
        }

        self.loading = false;
    }

    /// Parse URL into (host, port, path)
    fn parse_url(&self, url: &str) -> (String, u16, String) {
        let url = url.trim();

        // Remove http:// or https:// prefix
        let url = if url.starts_with("http://") {
            &url[7..]
        } else if url.starts_with("https://") {
            &url[8..]
        } else {
            url
        };

        // Split into host and path
        let parts: Vec<&str> = url.splitn(2, '/').collect();
        let host_part = parts[0];
        let path = if parts.len() > 1 {
            format!("/{}", parts[1])
        } else {
            "/".to_string()
        };

        // Split host and port
        let (host, port) = if host_part.contains(':') {
            let parts: Vec<&str> = host_part.splitn(2, ':').collect();
            (parts[0].to_string(), parts[1].parse().unwrap_or(80))
        } else {
            (host_part.to_string(), 80)
        };

        (host, port, path)
    }

    /// Make HTTP GET request (returns full HTTP response including headers)
    /// Make HTTP GET request (returns full HTTP response including headers)
    fn http_get(&self, host: &str, port: u16, path: &str) -> Option<String> {
        unsafe {
            crate::kernel::uart_write_string("http_get: Starting (using smoltcp)\r\n");

            // Use smoltcp network stack  
            let stack = match crate::kernel::NETWORK_STACK.as_mut() {
                Some(s) => s,
                None => {
                    crate::kernel::uart_write_string("http_get: No network stack\r\n");
                    return None;
                }
            };

            // Use smoltcp http_get helper
            match crate::system::net::helpers::http_get(stack, host, path, port, 10000) {
                Ok(response_data) => {
                    crate::kernel::uart_write_string(&alloc::format!("http_get: Received {} bytes\r\n", response_data.len()));
                    
                    // Convert to string and extract body
                    if let Ok(response) = core::str::from_utf8(&response_data) {
                        // Find the blank line that separates headers from body
                        if let Some(body_start) = response.find("\r\n\r\n") {
                            Some(response[body_start + 4..].to_string())
                        } else if let Some(body_start) = response.find("\n\n") {
                            Some(response[body_start + 2..].to_string())
                        } else {
                            Some(response.to_string())
                        }
                    } else {
                        crate::kernel::uart_write_string("http_get: Invalid UTF-8 in response\r\n");
                        None
                    }
                }
                Err(e) => {
                    crate::kernel::uart_write_string(&alloc::format!("http_get: Error: {}\r\n", e));
                    None
                }
            }
        }
    }
    /// Make HTTP GET request for binary data (images)
    fn http_get_binary(&self, host: &str, port: u16, path: &str) -> Option<Vec<u8>> {
        unsafe {
            crate::kernel::uart_write_string("http_get_binary: Starting (using smoltcp)\r\n");

            // Use smoltcp network stack
            let stack = match crate::kernel::NETWORK_STACK.as_mut() {
                Some(s) => s,
                None => {
                    crate::kernel::uart_write_string("http_get_binary: No network stack\r\n");
                    return None;
                }
            };

            // Use smoltcp http_get helper
            match crate::system::net::helpers::http_get(stack, host, path, port, 10000) {
                Ok(response_data) => {
                    crate::kernel::uart_write_string(&alloc::format!("http_get_binary: Received {} bytes\r\n", response_data.len()));

                    // Find the blank line that separates headers from body
                    if let Some(body_start) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
                        // Print the headers
                        if let Ok(headers) = core::str::from_utf8(&response_data[0..body_start]) {
                            crate::kernel::uart_write_string(&alloc::format!(
                                "http_get_binary: HTTP Headers:\r\n{}\r\n",
                                headers
                            ));
                        }

                        let body_len = response_data.len() - (body_start + 4);
                        crate::kernel::uart_write_string(&alloc::format!(
                            "http_get_binary: Body is {} bytes\r\n",
                            body_len
                        ));
                        Some(response_data[body_start + 4..].to_vec())
                    } else if let Some(body_start) = response_data.windows(2).position(|w| w == b"\n\n") {
                        let body_len = response_data.len() - (body_start + 2);
                        crate::kernel::uart_write_string(&alloc::format!(
                            "http_get_binary: Found \\n\\n at position {}, body is {} bytes\r\n",
                            body_start, body_len
                        ));
                        Some(response_data[body_start + 2..].to_vec())
                    } else {
                        crate::kernel::uart_write_string("http_get_binary: No header separator found, returning all data\r\n");
                        Some(response_data)
                    }
                }
                Err(e) => {
                    crate::kernel::uart_write_string(&alloc::format!("http_get_binary: Error: {}\r\n", e));
                    None
                }
            }
        }
    }

    /// Extract title from DOM tree
    fn extract_title(&self, node: &Node) -> Option<String> {
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
                    if let Some(title) = self.extract_title(child) {
                        return Some(title);
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Load HTML content
    pub fn load_html(&mut self, html: String) {
        crate::kernel::uart_write_string("load_html: Starting HTML parsing\r\n");
        let mut parser = Parser::new(html);
        let dom = parser.parse();

        crate::kernel::uart_write_string("load_html: HTML parsed, clearing layout\r\n");

        // Debug: Print DOM structure
        self.debug_print_dom(&dom, 0);

        self.layout = Vec::new();

        // Extract page title from DOM
        self.page_title = self.extract_title(&dom);

        // Update window title
        if let Some(ref title) = self.page_title {
            let window_title = alloc::format!("Browser - {}", title);
            crate::gui::window_manager::set_browser_window_title(self.instance_id, &window_title);
        }

        // Layout the DOM tree - search for <body> element
        crate::kernel::uart_write_string("load_html: Starting layout\r\n");

        // Find and layout the <body> element (it might be nested in malformed HTML)
        self.find_and_layout_body(&dom, 10, 10, 1000);

        crate::kernel::uart_write_string(&alloc::format!("load_html: Layout complete, {} layout boxes created\r\n", self.layout.len()));

        // Store the DOM after layout
        self.dom = Some(dom);
    }

    /// Debug helper to print DOM structure
    fn debug_print_dom(&self, node: &Node, depth: usize) {
        let indent = "  ".repeat(depth);
        match &node.node_type {
            NodeType::Element(elem) => {
                crate::kernel::uart_write_string(&alloc::format!("{}Element: <{}> ({} children)\r\n",
                    indent, elem.tag_name, node.children.len()));
                for child in &node.children {
                    self.debug_print_dom(child, depth + 1);
                }
            }
            NodeType::Text(text) => {
                let preview = if text.len() > 40 { &text[..40] } else { text };
                crate::kernel::uart_write_string(&alloc::format!("{}Text: \"{}\"\r\n", indent, preview));
            }
        }
    }

    /// Find and layout the <body> element, wherever it is in the DOM
    fn find_and_layout_body(&mut self, node: &Node, x: usize, y: usize, max_width: usize) {
        match &node.node_type {
            NodeType::Element(elem) => {
                if elem.tag_name == "body" {
                    // Found the body! Layout it (which will recursively layout its children)
                    crate::kernel::uart_write_string("find_and_layout_body: Found <body> element\r\n");
                    // Body text uses font_size_level = 1 (18px TTF / 8px bitmap)
                    self.layout_node(node, x, y, max_width, &Color::BLACK, false, false, 1, "");
                    return;
                }
                // Not body, recurse into children to find it
                for child in &node.children {
                    self.find_and_layout_body(child, x, y, max_width);
                }
            }
            NodeType::Text(_) => {
                // Text nodes can't contain body
            }
        }
    }

    /// Load error page
    fn load_error_page(&mut self, message: &str) {
        let html = alloc::format!(
            "<html><body><h1>Error</h1><p>{}</p></body></html>",
            message
        );
        self.load_html(html);
    }

    /// Load about: page
    fn load_about_page(&mut self, url: &str) {
        let html = match url {
            "about:blank" => "<html><body></body></html>".to_string(),
            _ => alloc::format!(
                "<html><body>\
                <h1>rOSt Browser</h1>\
                <p>Version 1.0 - A simple web browser for rOSt</p>\
                <h2>Features</h2>\
                <ul>\
                <li>HTML parser with DOM tree</li>\
                <li>Text layout engine</li>\
                <li>Clickable hyperlinks</li>\
                <li>Address bar navigation</li>\
                <li>Keyboard shortcuts Ctrl+L</li>\
                </ul>\
                <h2>Current Limitations</h2>\
                <ul>\
                <li>Powered by smoltcp TCP/IP stack</li>\
                <li>No CSS support</li>\
                <li>Basic tags only h1-h6 p a ul ol li br div b i img</li>\
                <li>BMP image support (24-bit uncompressed)</li>\
                </ul>\
                <p>Use Terminal http command to test HTTP: <code>http example.com</code></p>\
                <p>Try clicking this test link <a href=\"about:blank\">about:blank</a></p>\
                </body></html>"
            ),
        };
        self.load_html(html);
        self.loading = false;
    }

    /// Recursive layout function
    fn layout_node(
        &mut self,
        node: &Node,
        x: usize,
        y: usize,
        max_width: usize,
        color: &Color,
        bold: bool,
        italic: bool,
        font_size: usize,
        element_id: &str,
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
                    self.layout.push(LayoutBox {
                        x: current_x,
                        y: current_y,
                        width: word_width,
                        height: char_height,
                        text: word.to_string() + " ",
                        color: *color,
                        font_size,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                        element_id: element_id.to_string(),
                        is_image: false,
                        image_data: None,
                    });

                    current_x += word_width;
                }

                (current_x, current_y)
            }
            NodeType::Element(elem) => {
                self.layout_element(node, elem, x, y, max_width, color, bold, italic, font_size, element_id)
            }
        }
    }

    /// Layout an element
    fn layout_element(
        &mut self,
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

        let mut current_x = x;
        let mut current_y = y;

        // Block-level elements start on new line
        let is_block = matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "ul" | "ol" | "li" | "br" | "hr");
        if is_block && !self.layout.is_empty() {
            current_x = x;
            // Use whichever is lower on page: explicit spacing from parent (y) or default spacing
            let default_y = self.layout.last().map(|b| b.y + b.height + 4).unwrap_or(y);
            current_y = default_y.max(y);
        }

        // Determine color, style, and font size level
        let color = parent_color;
        let bold = parent_bold || matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong");
        let italic = parent_italic || matches!(tag, "i" | "em");
        let font_size_level = match tag {
            "h1" => 5,  // 36px TTF / 40px bitmap
            "h2" => 4,  // 28px TTF / 32px bitmap
            "h3" => 3,  // 24px TTF / 24px bitmap
            "h4" => 2,  // 20px TTF / 16px bitmap
            "h5" => 2,  // 20px TTF / 16px bitmap
            "h6" => 2,  // 20px TTF / 16px bitmap
            _ => parent_font_size,
        };

        // Get actual height for spacing calculations
        let element_height = if crate::gui::font::is_available() {
            crate::gui::font::get_char_height() as usize
        } else {
            CHAR_HEIGHT * font_size_level
        };

        // Handle special tags
        match tag {
            "br" => {
                return (x, current_y + element_height + 2);
            }
            "hr" => {
                // Horizontal rule - draw a line across the page
                // Add spacing before
                if !self.layout.is_empty() {
                    current_y += element_height + 4;
                }

                // Draw horizontal line using dashes
                let line_width = max_width.saturating_sub(20); // Leave 10px margin on each side
                let char_width_hr = if crate::gui::font::is_available() {
                    let font_size_px = get_font_size_px(font_size_level);
                    crate::gui::font::measure_string("-", font_size_px) as usize
                } else {
                    CHAR_WIDTH * font_size_level
                };
                let num_dashes = line_width / char_width_hr;
                let hr_line = alloc::format!("{}", "-".repeat(num_dashes));

                self.layout.push(LayoutBox {
                    x: x + 10,
                    y: current_y,
                    width: num_dashes * char_width_hr,
                    height: element_height,
                    text: hr_line,
                    color: Color::new(128, 128, 128), // Gray
                    font_size: font_size_level,
                    is_link: false,
                    link_url: String::new(),
                    bold: false,
                    italic: false,
                    element_id: element_id.to_string(),
                    is_image: false,
                    image_data: None,
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
                        let (host, port, _) = self.parse_url(&self.url);
                        alloc::format!("http://{}:{}{}", host, port, src)
                    } else {
                        // Relative path - append to current URL's directory
                        let base_url = if let Some(last_slash) = self.url.rfind('/') {
                            &self.url[..last_slash]
                        } else {
                            &self.url
                        };
                        alloc::format!("{}/{}", base_url, src)
                    };

                    crate::kernel::uart_write_string(&alloc::format!("layout_element: Fetching image from: {}\r\n", img_url));

                    // Parse URL and fetch image
                    let (host, port, path) = self.parse_url(&img_url);
                    if let Some(image_data) = self.http_get_binary(&host, port, &path) {
                        crate::kernel::uart_write_string(&alloc::format!("layout_element: Fetched {} bytes\r\n", image_data.len()));

                        // Detect image format by magic bytes
                        let is_png = image_data.len() >= 8 &&
                                     image_data[0] == 0x89 && image_data[1] == 0x50 &&
                                     image_data[2] == 0x4E && image_data[3] == 0x47;
                        let is_bmp = image_data.len() >= 2 &&
                                     image_data[0] == 0x42 && image_data[1] == 0x4D;

                        // Decode image (try PNG first if detected, otherwise BMP)
                        let decoded_image = if is_png {
                            decode_png(&image_data)
                        } else if is_bmp {
                            decode_bmp(&image_data)
                        } else {
                            None
                        };

                        if let Some(img) = decoded_image {
                            let format_name = if is_png { "PNG" } else { "BMP" };
                            crate::kernel::uart_write_string(&alloc::format!("layout_element: Decoded {} {}x{}\r\n", format_name, img.width, img.height));

                            // Add spacing before image if needed
                            if !self.layout.is_empty() {
                                current_y += 4;
                            }

                            // Create layout box for image
                            self.layout.push(LayoutBox {
                                x: current_x,
                                y: current_y,
                                width: img.width as usize,
                                height: img.height as usize,
                                text: String::new(),
                                color: Color::BLACK,
                                font_size: font_size_level,
                                is_link: false,
                                link_url: String::new(),
                                bold: false,
                                italic: false,
                                element_id: element_id.to_string(),
                                is_image: true,
                                image_data: Some(img),
                            });

                            // Move to next line after image
                            current_y += self.layout.last().unwrap().height + 4;
                            return (x, current_y);
                        } else {
                            crate::kernel::uart_write_string("layout_element: Failed to decode BMP\r\n");
                        }
                    } else {
                        crate::kernel::uart_write_string("layout_element: Failed to fetch image\r\n");
                    }
                }
                // If image fetch/decode failed, just continue
                return (current_x, current_y);
            }
            "a" => {
                // Hyperlink - render children with link color
                let link_url = elem.attributes.get("href").cloned().unwrap_or_default();
                let link_color = Color::new(0, 0, 255); // Blue

                for child in &node.children {
                    let start_idx = self.layout.len();
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, &link_color, bold, italic, font_size_level, element_id);

                    // Mark all boxes created for this link
                    for i in start_idx..self.layout.len() {
                        self.layout[i].is_link = true;
                        self.layout[i].link_url = link_url.clone();
                    }

                    current_x = new_x;
                    current_y = new_y;
                }
                return (current_x, current_y);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                // Headings - larger font size with proportional spacing
                // Only add spacing before if there's already content above
                if !self.layout.is_empty() {
                    current_y += element_height; // Extra spacing before heading
                }

                for child in &node.children {
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, color, bold, italic, font_size_level, element_id);
                    current_x = new_x;
                    current_y = new_y;
                }

                // Add height of the text + spacing after
                current_y += element_height * 2;
                return (x, current_y);
            }
            "ul" | "ol" => {
                // Lists - use small fixed indent to prevent excessive nesting
                const LIST_INDENT: usize = 32; // Small fixed indent per nesting level

                // Add extra spacing before nested lists (x > 10 means we're indented)
                if !self.layout.is_empty() && x > 10 {
                    current_y += element_height / 2; // Extra space before nested list
                }

                for (i, child) in node.children.iter().enumerate() {
                    // Save the starting Y position for this list item
                    let list_item_y = current_y;

                    // Add bullet or number (use ASCII * since bullet • is not in ASCII)
                    let bullet = if tag == "ul" { "* " } else { &alloc::format!("{}. ", i + 1) };
                    let bullet_width = if crate::gui::font::is_available() {
                        let font_size_px = get_font_size_px(font_size_level);
                        crate::gui::font::measure_string(&bullet, font_size_px) as usize
                    } else {
                        bullet.len() * CHAR_WIDTH * font_size_level
                    };

                    // Layout the list item content first to get its starting position
                    let content_start_idx = self.layout.len();
                    let (_, new_y) = self.layout_node(child, current_x + LIST_INDENT, list_item_y, max_width - LIST_INDENT, color, bold, italic, font_size_level, element_id);

                    // Find the Y position where the content actually started
                    let content_y = if self.layout.len() > content_start_idx {
                        self.layout[content_start_idx].y
                    } else {
                        list_item_y
                    };

                    // Now add the bullet at the same Y position as the content
                    self.layout.insert(content_start_idx, LayoutBox {
                        x: current_x,
                        y: content_y,
                        width: bullet_width,
                        height: element_height,
                        text: bullet.to_string(),
                        color: *color,
                        font_size: font_size_level,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                        element_id: element_id.to_string(),
                        is_image: false,
                        image_data: None,
                    });

                    current_y = new_y + element_height + 2;
                }
                return (x, current_y);
            }
            _ => {}
        }

        // Render children
        for child in &node.children {
            // For block-level children (like nested lists), pass the base x position
            // For inline children (like text), pass current_x (continues on same line)
            let child_is_block = if let NodeType::Element(child_elem) = &child.node_type {
                matches!(child_elem.tag_name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "ul" | "ol" | "li" | "br" | "hr")
            } else {
                false
            };

            let child_x = if child_is_block { x } else { current_x };
            let (new_x, new_y) = self.layout_node(child, child_x, current_y, max_width, color, bold, italic, font_size_level, element_id);
            current_x = new_x;
            current_y = new_y;
        }

        // Block elements end with newline
        if is_block {
            (x, current_y + element_height + 2)
        } else {
            (current_x, current_y)
        }
    }

    /// Render browser to framebuffer
    pub fn render(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) {
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
        self.url_input.render_at(
            (win_x + input_x) as i32,
            (win_y + input_y) as i32,
            input_width as u32,
            input_height as u32,
            self.url_focused
        );

        // Back button
        let back_btn_x = win_x + win_width - 80;
        let back_btn_y = win_y + 3;
        let back_btn_width = 32;
        let back_btn_height = 24;
        self.draw_button(fb, fb_width, fb_height, back_btn_x, back_btn_y, back_btn_width, back_btn_height, "<");

        // Forward button
        let fwd_btn_x = win_x + win_width - 44;
        let fwd_btn_y = win_y + 3;
        let fwd_btn_width = 32;
        let fwd_btn_height = 24;
        self.draw_button(fb, fb_width, fb_height, fwd_btn_x, fwd_btn_y, fwd_btn_width, fwd_btn_height, ">");

        // Content area
        let content_y = win_y + 35;
        let content_height = win_height.saturating_sub(35);

        for layout_box in &self.layout {
            // Apply scroll offset
            if layout_box.y < self.scroll_offset {
                continue;
            }
            let y = layout_box.y - self.scroll_offset;

            if y + layout_box.height > content_height {
                break;
            }

            // Check if this is an image or text
            if layout_box.is_image {
                // Draw image
                if let Some(ref img) = layout_box.image_data {
                    // Render image (decoders output pixels[0] as top-left)
                    for img_y in 0..img.height as usize {
                        for img_x in 0..img.width as usize {
                            let fb_x = win_x + layout_box.x + img_x;
                            let fb_y = content_y + y + img_y;

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
            } else {
                // Draw text with underline for links
                self.draw_text(
                    fb,
                    fb_width,
                    fb_height,
                    win_x + layout_box.x,
                    content_y + y,
                    &layout_box.text,
                    &layout_box.color,
                    layout_box.font_size,
                );

                // Underline links
                if layout_box.is_link {
                    for x in 0..layout_box.width {
                        let fb_x = win_x + layout_box.x + x;
                        let fb_y = content_y + y + layout_box.height;
                        if fb_x < fb_width && fb_y < fb_height {
                            fb[fb_y * fb_width + fb_x] = layout_box.color.to_u32();
                        }
                    }
                }
            }
        }
    }

    /// Draw text using TrueType font if available, otherwise bitmap font
    fn draw_text(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, text: &str, color: &Color, font_size_level: usize) {
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
    fn draw_button(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, width: usize, height: usize, text: &str) {
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
            self.draw_text(fb, fb_width, fb_height, text_x, text_y, text, &Color::new(255, 255, 255), 1);
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char, ctrl: bool, shift: bool) {
        if self.url_focused {
            if key == '\n' {
                // Enter key - navigate
                self.url_focused = false;
                let url = self.url_input.get_text().to_string();
                self.navigate(url);
            } else {
                // Pass to TextInput
                self.url_input.handle_key(key, ctrl, shift);
            }
        } else {
            // Not focused on URL bar
            if key == 'l' && ctrl {
                // Ctrl+L - focus address bar and select all (like modern browsers)
                self.url_focused = true;
                self.url_input.set_text(&self.url);
                self.url_input.select_all();
            }
        }
    }

    /// Handle arrow key input
    pub fn handle_arrow_key(&mut self, arrow: crate::gui::widgets::text_input::ArrowKey, shift: bool) {
        if self.url_focused {
            self.url_input.handle_arrow_key(arrow, shift);
        }
    }

    /// Handle mouse click
    pub fn handle_click(&mut self, x: usize, y: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) {
        let rel_x = x.saturating_sub(win_x);
        let rel_y = y.saturating_sub(win_y);

        // Check if click is in address bar
        if rel_y < 30 {
            let input_x = 40;
            let input_y = 3;
            let input_width = win_width.saturating_sub(130);
            let input_height = 24;

            // Back button (32px wide, starting at win_width - 80)
            let back_btn_x = win_width.saturating_sub(80);
            if rel_x >= back_btn_x && rel_x < back_btn_x + 32 && rel_y >= 3 && rel_y < 27 {
                self.go_back();
            // Forward button (32px wide, starting at win_width - 44)
            } else if rel_x >= win_width.saturating_sub(44) && rel_x < win_width.saturating_sub(12) && rel_y >= 3 && rel_y < 27 {
                self.go_forward();
            } else if rel_x >= input_x && rel_x < input_x + input_width
                   && rel_y >= input_y && rel_y < input_y + input_height {
                // Click inside URL input field
                let was_focused = self.url_focused;

                if !was_focused {
                    // First click on unfocused URL bar - focus and select all (like Chrome/Safari)
                    self.url_focused = true;
                    self.url_input.set_text(&self.url);
                    self.url_input.select_all();
                } else {
                    // Already focused - position cursor at click location (subsequent click)
                    self.url_input.handle_mouse_down(rel_x as i32, (input_x + 4) as i32);
                }
            } else {
                // Click elsewhere in address bar - unfocus
                self.url_focused = false;
            }
            return;
        }

        // Check if click is on a link
        let content_y = 35;
        let click_y = rel_y.saturating_sub(content_y) + self.scroll_offset;

        for layout_box in &self.layout {
            if layout_box.is_link {
                if rel_x >= layout_box.x
                    && rel_x < layout_box.x + layout_box.width
                    && click_y >= layout_box.y
                    && click_y < layout_box.y + layout_box.height
                {
                    // Clicked on link!

                    // Handle internal anchor links
                    if layout_box.link_url.starts_with('#') {
                        // Internal anchor - find element and scroll to it
                        let anchor_id = &layout_box.link_url[1..]; // Strip the '#'

                        // Find the first layout box with this element_id
                        if let Some(target_box) = self.layout.iter().find(|b| b.element_id == anchor_id) {
                            // Calculate scroll bounds to avoid scrolling past content
                            let content_height = win_height.saturating_sub(35); // Address bar is 35px
                            let max_content_y = self.layout.iter()
                                .map(|box_| box_.y + box_.height)
                                .max()
                                .unwrap_or(0);
                            let max_scroll = max_content_y.saturating_sub(content_height);

                            // Scroll to the target element, but respect scroll bounds
                            self.scroll_offset = target_box.y.min(max_scroll);
                        }
                        return;
                    }

                    // Handle relative URLs
                    let url = if layout_box.link_url.starts_with("http://") || layout_box.link_url.starts_with("https://") {
                        layout_box.link_url.clone()
                    } else if layout_box.link_url.starts_with('/') {
                        // Absolute path - use current host
                        let (host, port, _) = self.parse_url(&self.url);
                        alloc::format!("http://{}:{}{}", host, port, layout_box.link_url)
                    } else {
                        // Relative path - append to current URL's directory
                        alloc::format!("{}/{}", self.url.trim_end_matches('/'), layout_box.link_url)
                    };

                    self.navigate(url);
                    return;
                }
            }
        }

        // Click elsewhere - unfocus address bar
        self.url_focused = false;
    }

    /// Handle mouse drag - returns true if selection changed
    pub fn handle_mouse_drag(&mut self, x: usize, y: usize, win_x: usize, win_y: usize, win_width: usize, _win_height: usize) -> bool {
        let rel_x = x.saturating_sub(win_x);
        let rel_y = y.saturating_sub(win_y);

        // Only handle drag in URL input area when focused
        if self.url_focused && rel_y < 30 {
            let input_x = 40;
            let input_y = 3;
            let input_width = win_width.saturating_sub(130);
            let input_height = 24;

            if rel_x >= input_x && rel_x < input_x + input_width
               && rel_y >= input_y && rel_y < input_y + input_height {
                return self.url_input.handle_mouse_drag(rel_x as i32, (input_x + 4) as i32);
            }
        }
        false
    }

    /// Handle mouse up
    pub fn handle_mouse_up(&mut self) {
        if self.url_focused {
            self.url_input.handle_mouse_up();
        }
    }

    /// Handle scroll
    pub fn handle_scroll(&mut self, delta: i32, win_height: usize) {
        // Calculate total content height
        let content_height = win_height.saturating_sub(35); // Address bar is 35px
        let max_content_y = self.layout.iter()
            .map(|box_| box_.y + box_.height)
            .max()
            .unwrap_or(0);

        // Calculate maximum scroll offset (don't scroll past the end)
        let max_scroll = max_content_y.saturating_sub(content_height);

        if delta > 0 {
            // Scroll down - clamp to max
            self.scroll_offset = (self.scroll_offset.saturating_add(20)).min(max_scroll);
        } else {
            // Scroll up - clamp to 0
            self.scroll_offset = self.scroll_offset.saturating_sub(20);
        }
    }

    /// Go back in history
    pub fn go_back(&mut self) {
        if self.history_index > 1 {
            self.history_index -= 1;
            let url = self.history[self.history_index - 1].clone();
            self.url = url.clone();
            self.url_input.set_text(&url);

            // Reload page (simplified - in real browser we'd use cache)
            self.navigate(url);
        }
    }

    /// Go forward in history
    pub fn go_forward(&mut self) {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            let url = self.history[self.history_index - 1].clone();
            self.url = url.clone();
            self.url_input.set_text(&url);

            // Reload page
            self.navigate(url);
        }
    }
}

/// Initialize browser system
pub fn init() {
    unsafe {
        BROWSERS = Vec::new();
    }
}

/// Create a new browser instance
pub fn create_browser() -> usize {
    unsafe {
        let id = BROWSERS.len();
        let mut browser = Browser::new(id);

        // Navigate to default page
        browser.navigate("about:home".to_string());

        BROWSERS.push(browser);
        id
    }
}

/// Get a browser instance by ID
pub fn get_browser(id: usize) -> Option<&'static mut Browser> {
    unsafe {
        if id < BROWSERS.len() {
            Some(&mut BROWSERS[id])
        } else {
            None
        }
    }
}

/// Render a browser at a specific position
pub fn render_at(instance_id: usize, x: usize, y: usize, width: usize, height: usize) {
    unsafe {
        if instance_id < BROWSERS.len() {
            let browser = &BROWSERS[instance_id];

            // Get framebuffer
            let fb = crate::gui::framebuffer::get_back_buffer();
            let (fb_width, fb_height) = crate::gui::framebuffer::get_screen_dimensions();
            browser.render(fb, fb_width as usize, fb_height as usize, x, y, width, height);
        }
    }
}

/// Handle keyboard input for a browser
pub fn handle_key(instance_id: usize, key: char, ctrl: bool, shift: bool) {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_key(key, ctrl, shift);
        }
    }
}

/// Handle arrow key input for a browser
pub fn handle_arrow_key(instance_id: usize, arrow: crate::gui::widgets::text_input::ArrowKey, shift: bool) {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_arrow_key(arrow, shift);
        }
    }
}

/// Handle mouse click for a browser
pub fn handle_click(instance_id: usize, x: usize, y: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_click(x, y, win_x, win_y, win_width, win_height);
        }
    }
}

/// Handle mouse drag for a browser
pub fn handle_mouse_drag(instance_id: usize, x: usize, y: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) -> bool {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_mouse_drag(x, y, win_x, win_y, win_width, win_height)
        } else {
            false
        }
    }
}

/// Handle mouse up for a browser
pub fn handle_mouse_up(instance_id: usize) {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_mouse_up();
        }
    }
}

/// Remove a browser instance (when window is closed)
pub fn remove_browser(_instance_id: usize) {
    // For now, we don't actually remove browsers from the vector
    // They'll just remain unused. In a real implementation, we'd need to
    // handle this properly with Option<Browser> or Vec::remove
}
