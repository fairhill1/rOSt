/// Web browser for rOSt
/// Features: HTML rendering, address bar, hyperlinks, scrolling

use crate::kernel::html_parser::{Parser, Node, NodeType, ElementData};
use crate::kernel::framebuffer::FONT_8X8;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use alloc::format;

/// Global list of browser instances
static mut BROWSERS: Vec<Browser> = Vec::new();

const CHAR_WIDTH: usize = 8;
const CHAR_HEIGHT: usize = 8;

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
}

pub struct Browser {
    pub url: String,
    pub url_input: String,
    pub url_focused: bool,
    pub dom: Option<Node>,
    pub layout: Vec<LayoutBox>,
    pub scroll_offset: usize,
    pub history: Vec<String>,
    pub history_index: usize,
    pub loading: bool,
}

impl Browser {
    pub fn new() -> Self {
        Browser {
            url: String::from("about:blank"),
            url_input: String::new(),
            url_focused: false,
            dom: None,
            layout: Vec::new(),
            scroll_offset: 0,
            history: Vec::new(),
            history_index: 0,
            loading: false,
        }
    }

    /// Navigate to a URL
    pub fn navigate(&mut self, url: String) {
        // crate::uart_write_string(&alloc::format!("Browser: Navigating to {}\r\n", url));

        // Add to history
        if self.history_index < self.history.len() {
            self.history.truncate(self.history_index);
        }
        self.history.push(url.clone());
        self.history_index = self.history.len();

        self.url = url.clone();
        self.url_input = url.clone();
        self.scroll_offset = 0;
        self.loading = true;

        // Handle special URLs
        if url.starts_with("about:") {
            self.load_about_page(&url);
            return;
        }

        // Parse URL to get host, port, path
        let (host, port, path) = self.parse_url(&url);

        // For now, only support IP addresses (not domain names)
        // TODO: Add DNS resolution support later
        let ip = if host.chars().all(|c| c.is_digit(10) || c == '.') {
            host.clone()
        } else {
            self.load_error_page("Browser only supports IP addresses for now. Try http://10.0.2.2:8888");
            return;
        };

        // Make HTTP request
        match self.http_get(&ip, port, &path) {
            Some(html) => {
                self.load_html(html);
            }
            None => {
                self.load_error_page("HTTP requests not yet implemented. Use Terminal 'httptest' command to test HTTP.");
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

    /// Make HTTP GET request
    /// TODO: Implement full HTTP client using TCP stack
    fn http_get(&self, _ip: &str, _port: u16, _path: &str) -> Option<String> {
        // HTTP requests not yet implemented - need to integrate with TCP stack
        // For now, return an error page
        None
    }

    /// Load HTML content
    pub fn load_html(&mut self, html: String) {
        let mut parser = Parser::new(html);
        let dom = parser.parse();

        self.layout = Vec::new();

        // Layout the DOM tree
        self.layout_node(&dom, 10, 50, 1000, &Color::BLACK, false, false);

        // Store the DOM after layout
        self.dom = Some(dom);
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
                <li>HTTP requests not yet implemented</li>\
                <li>No CSS support</li>\
                <li>Basic tags only h1-h6 p a ul ol li br div b i</li>\
                </ul>\
                <p>Use Terminal httptest command to test HTTP</p>\
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
                let char_width = CHAR_WIDTH;
                let char_height = CHAR_HEIGHT;

                for word in words {
                    let word_width = word.len() * char_width;

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
                        font_size: 1,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                    });

                    current_x += word_width + char_width;
                }

                (current_x, current_y)
            }
            NodeType::Element(elem) => {
                self.layout_element(node, elem, x, y, max_width, color, bold, italic)
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
    ) -> (usize, usize) {
        let tag = elem.tag_name.as_str();

        let mut current_x = x;
        let mut current_y = y;

        // Block-level elements start on new line
        let is_block = matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "ul" | "ol" | "li" | "br");
        if is_block && !self.layout.is_empty() {
            current_x = x;
            current_y = self.layout.last().map(|b| b.y + b.height + 4).unwrap_or(y);
        }

        // Determine color and style
        let color = parent_color;
        let bold = parent_bold || matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong");
        let italic = parent_italic || matches!(tag, "i" | "em");

        // Handle special tags
        match tag {
            "br" => {
                return (x, current_y + CHAR_HEIGHT + 2);
            }
            "a" => {
                // Hyperlink - render children with link color
                let link_url = elem.attributes.get("href").cloned().unwrap_or_default();
                let link_color = Color::new(0, 0, 255); // Blue

                for child in &node.children {
                    let start_idx = self.layout.len();
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, &link_color, bold, italic);

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
                // Headings - larger font size
                current_y += 8; // Extra spacing before heading

                for child in &node.children {
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, color, bold, italic);
                    current_x = new_x;
                    current_y = new_y;
                }

                current_y += 8; // Extra spacing after heading
                return (x, current_y);
            }
            "ul" | "ol" => {
                // Lists
                for (i, child) in node.children.iter().enumerate() {
                    // Add bullet or number (use ASCII * since bullet • is not in ASCII)
                    let bullet = if tag == "ul" { "* " } else { &alloc::format!("{}. ", i + 1) };

                    self.layout.push(LayoutBox {
                        x: current_x,
                        y: current_y,
                        width: bullet.len() * CHAR_WIDTH,
                        height: CHAR_HEIGHT,
                        text: bullet.to_string(),
                        color: *color,
                        font_size: 1,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                    });

                    let (_, new_y) = self.layout_node(child, current_x + bullet.len() * CHAR_WIDTH + 8, current_y, max_width - bullet.len() * CHAR_WIDTH - 8, color, bold, italic);
                    current_y = new_y + CHAR_HEIGHT + 2;
                }
                return (x, current_y);
            }
            _ => {}
        }

        // Render children
        for child in &node.children {
            let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, color, bold, italic);
            current_x = new_x;
            current_y = new_y;
        }

        // Block elements end with newline
        if is_block {
            (x, current_y + CHAR_HEIGHT + 2)
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

        // Address bar text
        let addr_text = if self.url_focused {
            alloc::format!("URL: {}|", self.url_input)
        } else {
            alloc::format!("URL: {}", self.url)
        };
        self.draw_text(fb, fb_width, fb_height, win_x + 10, win_y + 10, &addr_text, &Color::BLACK);

        // Back button
        self.draw_text(fb, fb_width, fb_height, win_x + win_width - 80, win_y + 10, "[<]", &Color::new(100, 100, 100));

        // Forward button
        self.draw_text(fb, fb_width, fb_height, win_x + win_width - 50, win_y + 10, "[>]", &Color::new(100, 100, 100));

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

            // Draw text with underline for links
            self.draw_text(
                fb,
                fb_width,
                fb_height,
                win_x + layout_box.x,
                content_y + y,
                &layout_box.text,
                &layout_box.color,
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

    /// Draw text
    fn draw_text(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, text: &str, color: &Color) {
        let mut current_x = x;
        for ch in text.chars() {
            if ch.is_ascii() {
                let glyph = FONT_8X8[ch as usize];
                for row in 0..8 {
                    for col in 0..8 {
                        if (glyph[row] & (1 << (7 - col))) != 0 {
                            let fb_x = current_x + col;
                            let fb_y = y + row;
                            if fb_x < fb_width && fb_y < fb_height {
                                fb[fb_y * fb_width + fb_x] = color.to_u32();
                            }
                        }
                    }
                }
            }
            current_x += CHAR_WIDTH;
        }
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char, ctrl: bool) {
        if self.url_focused {
            if key == '\n' {
                // Enter key - navigate
                self.url_focused = false;
                let url = self.url_input.clone();
                self.navigate(url);
            } else if key == '\x08' {
                // Backspace
                self.url_input.pop();
            } else if key.is_ascii() && !ctrl {
                self.url_input.push(key);
            }
        } else {
            // Not focused on URL bar
            if key == 'l' && ctrl {
                // Ctrl+L - focus address bar
                self.url_focused = true;
                self.url_input = self.url.clone();
            }
        }
    }

    /// Handle mouse click
    pub fn handle_click(&mut self, x: usize, y: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) {
        let rel_x = x.saturating_sub(win_x);
        let rel_y = y.saturating_sub(win_y);

        // Check if click is in address bar
        if rel_y < 30 {
            if rel_x > win_width - 80 && rel_x < win_width - 60 {
                // Back button
                self.go_back();
            } else if rel_x > win_width - 50 && rel_x < win_width - 30 {
                // Forward button
                self.go_forward();
            } else if rel_x < win_width - 90 {
                // Click on address bar - focus it
                self.url_focused = true;
                self.url_input = self.url.clone();
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

    /// Handle scroll
    pub fn handle_scroll(&mut self, delta: i32) {
        if delta > 0 {
            self.scroll_offset = self.scroll_offset.saturating_add(20);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub(20);
        }
    }

    /// Go back in history
    pub fn go_back(&mut self) {
        if self.history_index > 1 {
            self.history_index -= 1;
            let url = self.history[self.history_index - 1].clone();
            self.url = url.clone();
            self.url_input = url.clone();

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
            self.url_input = url.clone();

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
        let mut browser = Browser::new();

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
            let fb = crate::kernel::framebuffer::get_back_buffer();
            let (fb_width, fb_height) = crate::kernel::framebuffer::get_screen_dimensions();
            browser.render(fb, fb_width as usize, fb_height as usize, x, y, width, height);
        }
    }
}

/// Handle keyboard input for a browser
pub fn handle_key(instance_id: usize, key: char, ctrl: bool) {
    unsafe {
        if instance_id < BROWSERS.len() {
            BROWSERS[instance_id].handle_key(key, ctrl);
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

/// Remove a browser instance (when window is closed)
pub fn remove_browser(_instance_id: usize) {
    // For now, we don't actually remove browsers from the vector
    // They'll just remain unused. In a real implementation, we'd need to
    // handle this properly with Option<Browser> or Vec::remove
}
