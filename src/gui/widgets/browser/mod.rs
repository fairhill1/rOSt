/// Web browser module for rOSt
/// Features: HTML rendering, address bar, hyperlinks, scrolling, async HTTP/image loading

// Module declarations
mod types;
mod utils;
mod http;
mod layout;
mod render;
mod navigation;

// Re-export types
pub use types::*;

use crate::gui::html_parser::{Parser, Node, NodeType, ElementData};
use crate::gui::css_parser::Stylesheet;
use crate::gui::framebuffer::FONT_8X8;
use crate::gui::widgets::text_input::TextInput;
use crate::gui::bmp_decoder::BmpImage;
use crate::gui::bmp_decoder::decode_bmp;
use crate::gui::png_decoder::decode_png;
use crate::gui::jpeg_decoder::decode_jpeg;
use smoltcp::iface::SocketHandle;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

/// Global list of browser instances
static mut BROWSERS: Vec<Browser> = Vec::new();

/// Browser widget
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

    // Async HTTP state
    http_state: HttpState,

    // Async image loading
    pending_images: Vec<PendingImage>,
    image_load_state: ImageLoadState,

    // Image cache to prevent re-downloading on layout reflow
    image_cache: alloc::collections::BTreeMap<String, BmpImage>,

    // Async CSS loading
    pending_css: Vec<PendingCss>,
    css_load_state: CssLoadState,

    // Loaded stylesheets
    pub stylesheets: Vec<Stylesheet>,

    // Track window width for reflow on resize
    last_window_width: usize,
}

impl Browser {
    /// Create a new browser instance
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
            http_state: HttpState::Idle,
            pending_images: Vec::new(),
            image_load_state: ImageLoadState::Idle,
            image_cache: alloc::collections::BTreeMap::new(),
            pending_css: Vec::new(),
            css_load_state: CssLoadState::Idle,
            stylesheets: Vec::new(),
            last_window_width: 0,
        }
    }

    /// Poll async HTTP request state machine (called each frame)
    /// Returns true if display needs redraw
    pub fn poll_http(&mut self) -> bool {
        // Move state out temporarily to avoid borrow checker issues
        let current_state = core::mem::replace(&mut self.http_state, HttpState::Idle);

        let mut needs_redraw = false;

        self.http_state = match current_state {
            HttpState::Idle => HttpState::Idle,

            HttpState::ResolvingDns { host, path, port, start_time } => {
                // Try to resolve DNS
                unsafe {
                    if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                        // Check if host is already an IP (instant)
                        let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(&host) {
                            Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                        } else {
                            // Need DNS - use blocking dns_lookup for now (we'll make it async later)
                            match crate::system::net::helpers::dns_lookup(stack, &host, 5000) {
                                Ok(addresses) => {
                                    if addresses.is_empty() {
                                        None
                                    } else {
                                        let ip = addresses[0];
                                        Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                                    }
                                }
                                Err(_) => None,
                            }
                        };

                        if let Some(server_ip) = server_ip {
                                crate::kernel::uart_write_string(&alloc::format!("DNS resolved, connecting...\r\n"));

                                // Create TCP socket
                                let tcp_handle = stack.create_tcp_socket();

                                // Generate dynamic local port
                                static mut LOCAL_PORT_COUNTER: u16 = 49152;
                                let local_port = unsafe {
                                    let port = LOCAL_PORT_COUNTER;
                                    LOCAL_PORT_COUNTER = if LOCAL_PORT_COUNTER >= 65000 { 49152 } else { LOCAL_PORT_COUNTER + 1 };
                                    port
                                };

                                // Initiate connection
                                let remote_endpoint = smoltcp::wire::IpEndpoint::new(server_ip, port);
                                if let Err(_) = stack.tcp_connect(tcp_handle, remote_endpoint, local_port) {
                                    stack.remove_socket(tcp_handle);
                                    HttpState::Error {
                                        message: "Failed to initiate TCP connection".to_string(),
                                    }
                                } else {
                                    // Build HTTP request
                                    let http_request = alloc::format!(
                                        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
                                        path, host
                                    );

                                    HttpState::Connecting {
                                        socket_handle: tcp_handle,
                                        http_request,
                                        start_time: crate::kernel::drivers::timer::get_time_ms(),
                                    }
                                }
                            } else {
                                // DNS still resolving, stay in this state
                                HttpState::ResolvingDns { host, path, port, start_time }
                            }
                        } else {
                            HttpState::Error {
                                message: "No network stack available".to_string(),
                            }
                        }
                    }
            }

            HttpState::Connecting { socket_handle, http_request, start_time } => {
                // Check timeout (10 seconds for connection)
                if crate::kernel::drivers::timer::get_time_ms() - start_time > 10000 {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            stack.remove_socket(socket_handle);
                        }
                    }
                    HttpState::Error {
                        message: "Connection timeout".to_string(),
                    }
                } else {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            let is_connected = stack.with_tcp_socket(socket_handle, |socket| {
                                socket.may_send() && socket.may_recv()
                            });

                            if is_connected {
                                crate::kernel::uart_write_string("Connected, sending request...\r\n");

                                // Send HTTP request
                                stack.with_tcp_socket(socket_handle, |socket| {
                                    socket.send_slice(http_request.as_bytes()).ok();
                                });

                                HttpState::ReceivingResponse {
                                    socket_handle,
                                    response_data: Vec::new(),
                                    last_recv_time: crate::kernel::drivers::timer::get_time_ms(),
                                }
                            } else {
                                // Still connecting
                                HttpState::Connecting { socket_handle, http_request, start_time }
                            }
                        } else {
                            HttpState::Error {
                                message: "Network stack disappeared".to_string(),
                            }
                        }
                    }
                }
            }

            HttpState::ReceivingResponse { socket_handle, mut response_data, last_recv_time } => {
                unsafe {
                    if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                        let mut received_data = false;
                        let mut connection_closed = false;

                        stack.with_tcp_socket(socket_handle, |socket| {
                            // Receive data
                            while socket.can_recv() {
                                if let Ok(_) = socket.recv(|buffer| {
                                    let len = buffer.len();
                                    if len > 0 {
                                        response_data.extend_from_slice(buffer);
                                        received_data = true;
                                    }
                                    (len, ())
                                }) {}
                            }

                            // Check if connection closed
                            if !socket.may_recv() {
                                connection_closed = true;
                            }
                        });

                        let new_last_recv_time = if received_data {
                            crate::kernel::drivers::timer::get_time_ms()
                        } else {
                            last_recv_time
                        };

                        if connection_closed {
                            crate::kernel::uart_write_string(&alloc::format!("Received {} bytes total\r\n", response_data.len()));
                            stack.remove_socket(socket_handle);

                            // Parse HTTP response
                            if let Ok(response) = core::str::from_utf8(&response_data) {
                                // Find the blank line that separates headers from body
                                let html = if let Some(body_start) = response.find("\r\n\r\n") {
                                    response[body_start + 4..].to_string()
                                } else if let Some(body_start) = response.find("\n\n") {
                                    response[body_start + 2..].to_string()
                                } else {
                                    response.to_string()
                                };

                                HttpState::Complete { html }
                            } else {
                                HttpState::Error {
                                    message: "Invalid UTF-8 in response".to_string(),
                                }
                            }
                        } else {
                            // Check timeout (30 seconds of no data)
                            if crate::kernel::drivers::timer::get_time_ms() - new_last_recv_time > 30000 {
                                stack.remove_socket(socket_handle);
                                HttpState::Error {
                                    message: "Receive timeout".to_string(),
                                }
                            } else {
                                HttpState::ReceivingResponse {
                                    socket_handle,
                                    response_data,
                                    last_recv_time: new_last_recv_time,
                                }
                            }
                        }
                    } else {
                        HttpState::Error {
                            message: "Network stack disappeared".to_string(),
                        }
                    }
                }
            }

            HttpState::Complete { html } => {
                crate::kernel::uart_write_string(&alloc::format!("Page loaded, {} bytes\r\n", html.len()));
                layout::load_html(self, html);
                self.loading = false;
                needs_redraw = true; // Trigger redraw!
                HttpState::Idle
            }

            HttpState::Error { message } => {
                crate::kernel::uart_write_string(&alloc::format!("HTTP error: {}\r\n", message));
                navigation::load_error_page(self, &message);
                self.loading = false;
                needs_redraw = true; // Trigger redraw!
                HttpState::Idle
            }
        };

        // Also poll async image loading
        if let ImageLoadState::Idle = self.image_load_state {
            // Start next pending image load
            if let Some(pending) = self.pending_images.pop() {
                crate::kernel::uart_write_string(&alloc::format!("Starting async image load: {}\r\n", pending.url));

                // Parse URL and initiate TCP connection
                let (host, port, path) = http::parse_url(&pending.url);
                let format = if pending.url.ends_with(".png") {
                    ImageFormat::Png
                } else if pending.url.ends_with(".jpg") || pending.url.ends_with(".jpeg") {
                    ImageFormat::Jpeg
                } else {
                    ImageFormat::Bmp
                };

                unsafe {
                    if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                        // Resolve DNS
                        let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(&host) {
                            Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                        } else {
                            match crate::system::net::helpers::dns_lookup(stack, &host, 5000) {
                                Ok(addresses) => {
                                    if !addresses.is_empty() {
                                        let ip = addresses[0];
                                        Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => None,
                            }
                        };

                        if let Some(server_ip) = server_ip {
                            let tcp_handle = stack.create_tcp_socket();

                            static mut IMG_LOCAL_PORT: u16 = 50000;
                            let local_port = unsafe {
                                let port = IMG_LOCAL_PORT;
                                IMG_LOCAL_PORT = if IMG_LOCAL_PORT >= 60000 { 50000 } else { IMG_LOCAL_PORT + 1 };
                                port
                            };

                            let remote_endpoint = smoltcp::wire::IpEndpoint::new(server_ip, port);
                            if stack.tcp_connect(tcp_handle, remote_endpoint, local_port).is_ok() {
                                // Prepare HTTP request
                                let http_request = alloc::format!(
                                    "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
                                    path, host
                                );

                                self.image_load_state = ImageLoadState::Connecting {
                                    socket_handle: tcp_handle,
                                    http_request,
                                    start_time: crate::kernel::drivers::timer::get_time_ms(),
                                    layout_box_index: pending.layout_box_index,
                                    format,
                                    url: pending.url.clone(),
                                };
                            }
                        }
                    }
                }
            }
        } else {
            // Poll current image load
            let current_state = core::mem::replace(&mut self.image_load_state, ImageLoadState::Idle);

            self.image_load_state = match current_state {
                ImageLoadState::Idle => ImageLoadState::Idle,

                ImageLoadState::Connecting { socket_handle, http_request, start_time, layout_box_index, format, url } => {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            let connected = stack.with_tcp_socket(socket_handle, |socket| {
                                socket.may_send() && socket.may_recv()
                            });

                            if connected {
                                // Send HTTP request
                                stack.with_tcp_socket(socket_handle, |socket| {
                                    socket.send_slice(http_request.as_bytes()).ok();
                                });

                                ImageLoadState::Loading {
                                    socket_handle,
                                    response_data: Vec::new(),
                                    last_recv_time: crate::kernel::drivers::timer::get_time_ms(),
                                    layout_box_index,
                                    format,
                                    url,
                                }
                            } else if crate::kernel::drivers::timer::get_time_ms() - start_time > 10000 {
                                stack.remove_socket(socket_handle);
                                ImageLoadState::Idle
                            } else {
                                ImageLoadState::Connecting { socket_handle, http_request, start_time, layout_box_index, format, url }
                            }
                        } else {
                            ImageLoadState::Idle
                        }
                    }
                }

                ImageLoadState::Loading { socket_handle, mut response_data, last_recv_time, layout_box_index, format, url } => {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            let mut received_data = false;
                            let mut connection_closed = false;

                            stack.with_tcp_socket(socket_handle, |socket| {
                                while socket.can_recv() {
                                    if let Ok(_) = socket.recv(|buffer| {
                                        let len = buffer.len();
                                        if len > 0 {
                                            response_data.extend_from_slice(buffer);
                                            received_data = true;
                                        }
                                        (len, ())
                                    }) {}
                                }

                                if !socket.may_recv() {
                                    connection_closed = true;
                                }
                            });

                            let new_last_recv_time = if received_data {
                                crate::kernel::drivers::timer::get_time_ms()
                            } else {
                                last_recv_time
                            };

                            if connection_closed {
                                stack.remove_socket(socket_handle);

                                // Parse HTTP response and decode image
                                if let Some(body_start) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
                                    let image_data = &response_data[body_start + 4..];

                                    let decoded_image = match format {
                                        ImageFormat::Png => decode_png(image_data),
                                        ImageFormat::Jpeg => decode_jpeg(image_data),
                                        ImageFormat::Bmp => decode_bmp(image_data),
                                    };

                                    if let Some(img) = decoded_image {
                                        crate::kernel::uart_write_string(&alloc::format!("Image loaded: {}x{}\r\n", img.width, img.height));

                                        // Cache the loaded image
                                        self.image_cache.insert(url.clone(), img.clone());

                                        // Check if dimensions will change (need reflow)
                                        // Reflow if placeholder was 0x0 (no size specified in HTML)
                                        let needs_reflow = if layout_box_index < self.layout.len() {
                                            self.layout[layout_box_index].width == 0 &&
                                            self.layout[layout_box_index].height == 0
                                        } else {
                                            false
                                        };

                                        if needs_reflow {
                                            // Dimensions changed - need full reflow
                                            crate::kernel::uart_write_string("Image dimensions changed, reflowing layout\r\n");
                                            if let Some(ref dom) = self.dom.clone() {
                                                self.layout.clear();
                                                layout::layout_node(self, &dom, 10, 10, 1260, &Color::BLACK, &None, false, false, 1, "", &[]);

                                                // Add bottom padding after reflow
                                                if let Some(last_box) = self.layout.last() {
                                                    let bottom_padding_y = last_box.y + last_box.height;
                                                    self.layout.push(LayoutBox {
                                                        x: 10,
                                                        y: bottom_padding_y,
                                                        width: 1,
                                                        height: 25,
                                                        text: String::new(),
                                                        color: Color::new(255, 255, 255),
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
                                            }
                                            needs_redraw = true;
                                        } else {
                                            // Just update the image data in place
                                            if layout_box_index < self.layout.len() {
                                                self.layout[layout_box_index].image_data = Some(img.clone());
                                                self.layout[layout_box_index].text = String::new();
                                                needs_redraw = true;
                                            }
                                        }
                                    }
                                }

                                ImageLoadState::Idle
                            } else if crate::kernel::drivers::timer::get_time_ms() - new_last_recv_time > 30000 {
                                stack.remove_socket(socket_handle);
                                ImageLoadState::Idle
                            } else {
                                ImageLoadState::Loading {
                                    socket_handle,
                                    response_data,
                                    last_recv_time: new_last_recv_time,
                                    layout_box_index,
                                    format,
                                    url,
                                }
                            }
                        } else {
                            ImageLoadState::Idle
                        }
                    }
                }
            };
        }

        // Poll async CSS loading
        if let CssLoadState::Idle = self.css_load_state {
            // Start next pending CSS load
            if let Some(pending) = self.pending_css.pop() {
                crate::kernel::uart_write_string(&alloc::format!("Starting async CSS load: {}\r\n", pending.url));

                // Parse URL and initiate TCP connection
                let (host, port, path) = http::parse_url(&pending.url);

                unsafe {
                    if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                        // Resolve DNS
                        let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(&host) {
                            Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                        } else {
                            match crate::system::net::helpers::dns_lookup(stack, &host, 5000) {
                                Ok(addresses) => {
                                    if !addresses.is_empty() {
                                        let ip = addresses[0];
                                        Some(smoltcp::wire::IpAddress::Ipv4(smoltcp::wire::Ipv4Address::new(ip[0], ip[1], ip[2], ip[3])))
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => None,
                            }
                        };

                        if let Some(server_ip) = server_ip {
                            let tcp_handle = stack.create_tcp_socket();

                            static mut CSS_LOCAL_PORT: u16 = 51000;
                            let local_port = unsafe {
                                let port = CSS_LOCAL_PORT;
                                CSS_LOCAL_PORT = if CSS_LOCAL_PORT >= 61000 { 51000 } else { CSS_LOCAL_PORT + 1 };
                                port
                            };

                            let remote_endpoint = smoltcp::wire::IpEndpoint::new(server_ip, port);
                            if stack.tcp_connect(tcp_handle, remote_endpoint, local_port).is_ok() {
                                // Prepare HTTP request
                                let http_request = alloc::format!(
                                    "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
                                    path, host
                                );

                                self.css_load_state = CssLoadState::Connecting {
                                    socket_handle: tcp_handle,
                                    http_request,
                                    start_time: crate::kernel::drivers::timer::get_time_ms(),
                                    url: pending.url,
                                };
                            }
                        }
                    }
                }
            }
        } else {
            // Poll current CSS load
            let current_state = core::mem::replace(&mut self.css_load_state, CssLoadState::Idle);

            self.css_load_state = match current_state {
                CssLoadState::Idle => CssLoadState::Idle,

                CssLoadState::Connecting { socket_handle, http_request, start_time, url } => {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            let connected = stack.with_tcp_socket(socket_handle, |socket| {
                                socket.may_send() && socket.may_recv()
                            });

                            if connected {
                                // Send HTTP request
                                stack.with_tcp_socket(socket_handle, |socket| {
                                    socket.send_slice(http_request.as_bytes()).ok();
                                });

                                CssLoadState::Loading {
                                    socket_handle,
                                    response_data: Vec::new(),
                                    last_recv_time: crate::kernel::drivers::timer::get_time_ms(),
                                    url,
                                }
                            } else if crate::kernel::drivers::timer::get_time_ms() - start_time > 10000 {
                                stack.remove_socket(socket_handle);
                                CssLoadState::Idle
                            } else {
                                CssLoadState::Connecting { socket_handle, http_request, start_time, url }
                            }
                        } else {
                            CssLoadState::Idle
                        }
                    }
                }

                CssLoadState::Loading { socket_handle, mut response_data, last_recv_time, url } => {
                    unsafe {
                        if let Some(ref mut stack) = crate::kernel::NETWORK_STACK {
                            let mut received_data = false;
                            let mut connection_closed = false;

                            stack.with_tcp_socket(socket_handle, |socket| {
                                while socket.can_recv() {
                                    if let Ok(_) = socket.recv(|buffer| {
                                        let len = buffer.len();
                                        if len > 0 {
                                            response_data.extend_from_slice(buffer);
                                            received_data = true;
                                        }
                                        (len, ())
                                    }) {}
                                }

                                if !socket.may_recv() {
                                    connection_closed = true;
                                }
                            });

                            let new_last_recv_time = if received_data {
                                crate::kernel::drivers::timer::get_time_ms()
                            } else {
                                last_recv_time
                            };

                            if connection_closed {
                                stack.remove_socket(socket_handle);

                                // Parse HTTP response and extract CSS
                                if let Some(body_start) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
                                    let css_data = &response_data[body_start + 4..];

                                    if let Ok(css_text) = core::str::from_utf8(css_data) {
                                        crate::kernel::uart_write_string(&alloc::format!("CSS loaded: {} bytes\r\n", css_text.len()));

                                        // Parse the CSS
                                        let stylesheet = crate::gui::css_parser::Stylesheet::parse(css_text);
                                        crate::kernel::uart_write_string(&alloc::format!("Parsed {} CSS rules\r\n", stylesheet.rules.len()));

                                        // Add to stylesheets
                                        self.stylesheets.push(stylesheet);

                                        // Trigger reflow to apply styles
                                        if let Some(ref dom) = self.dom.clone() {
                                            self.layout.clear();
                                            // Use find_and_layout_body with actual window width
                                            // Start at (0, 0) with full width - CSS controls margins/padding
                                            let layout_width = if self.last_window_width > 0 {
                                                self.last_window_width
                                            } else {
                                                1280
                                            };
                                            layout::find_and_layout_body(self, &dom, 0, 0, layout_width);
                                        }

                                        needs_redraw = true;
                                    }
                                }

                                CssLoadState::Idle
                            } else if crate::kernel::drivers::timer::get_time_ms() - new_last_recv_time > 30000 {
                                stack.remove_socket(socket_handle);
                                CssLoadState::Idle
                            } else {
                                CssLoadState::Loading {
                                    socket_handle,
                                    response_data,
                                    last_recv_time: new_last_recv_time,
                                    url,
                                }
                            }
                        } else {
                            CssLoadState::Idle
                        }
                    }
                }
            };
        }

        needs_redraw
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

            // Back button (32px wide, 22px tall, starting at win_width - 80, y=4)
            let back_btn_x = win_width.saturating_sub(80);
            if rel_x >= back_btn_x && rel_x < back_btn_x + 32 && rel_y >= 4 && rel_y < 26 {
                navigation::go_back(self);
            // Forward button (32px wide, 22px tall, starting at win_width - 44, y=4)
            } else if rel_x >= win_width.saturating_sub(44) && rel_x < win_width.saturating_sub(12) && rel_y >= 4 && rel_y < 26 {
                navigation::go_forward(self);
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
                        let (host, port, _) = http::parse_url(&self.url);
                        alloc::format!("http://{}:{}{}", host, port, layout_box.link_url)
                    } else {
                        // Relative path - append to current URL's directory
                        alloc::format!("{}/{}", self.url.trim_end_matches('/'), layout_box.link_url)
                    };

                    navigation::navigate(self, url);
                    return;
                }
            }
        }

        // Click elsewhere - unfocus address bar
        self.url_focused = false;
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

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char, ctrl: bool, shift: bool) {
        if self.url_focused {
            if key == '\n' {
                // Enter key - navigate
                self.url_focused = false;
                let url = self.url_input.get_text().to_string();
                navigation::navigate(self, url);
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

    /// Get current URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Set URL focus state
    pub fn set_url_focus(&mut self, focused: bool) {
        self.url_focused = focused;
        if focused {
            self.url_input.set_text(&self.url);
            self.url_input.select_all();
        }
    }

    /// Render browser to framebuffer
    pub fn render(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, win_x: usize, win_y: usize, win_width: usize, win_height: usize) {
        render::render(self, fb, fb_width, fb_height, win_x, win_y, win_width, win_height);
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
        navigation::navigate(&mut browser, "about:home".to_string());

        BROWSERS.push(browser);
        id
    }
}

/// Poll all browsers' async HTTP state machines (call from main loop)
/// Returns true if any browser needs redraw
pub fn poll_all_browsers() -> bool {
    unsafe {
        let mut needs_redraw = false;
        for browser in BROWSERS.iter_mut() {
            if browser.poll_http() {
                needs_redraw = true;
            }
        }
        needs_redraw
    }
}

/// Render a browser at a specific position
pub fn render_at(instance_id: usize, x: usize, y: usize, width: usize, height: usize) {
    unsafe {
        if instance_id < BROWSERS.len() {
            let browser = &mut BROWSERS[instance_id];

            // Check if window width changed - trigger reflow if needed
            // Also reflow on first render (last_window_width == 0) if we have a DOM
            if browser.last_window_width != width {
                crate::kernel::uart_write_string(&alloc::format!(
                    "Browser: Window resized from {} to {} - reflowing layout\r\n",
                    browser.last_window_width, width
                ));

                // Reflow the layout with new width
                if let Some(ref dom) = browser.dom.clone() {
                    browser.layout.clear();
                    // Use find_and_layout_body just like initial load
                    // Start at (0, 0) with full width - CSS controls margins/padding
                    layout::find_and_layout_body(browser, &dom, 0, 0, width);
                }
            }

            browser.last_window_width = width;

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
