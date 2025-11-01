/// Web browser for rOSt
/// Features: HTML rendering, address bar, hyperlinks, scrolling

use crate::gui::html_parser::{Parser, Node, NodeType, ElementData};
use crate::gui::framebuffer::FONT_8X8;
use crate::gui::widgets::text_input::TextInput;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
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
    pub element_id: String, // HTML element ID attribute
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

    /// Make HTTP GET request
    fn http_get(&self, host: &str, port: u16, path: &str) -> Option<String> {
        unsafe {
            crate::kernel::uart_write_string("http_get: Starting\r\n");

            // Get network device
            let devices = match crate::kernel::NET_DEVICES.as_mut() {
                Some(d) if !d.is_empty() => d,
                _ => {
                    crate::kernel::uart_write_string("http_get: No network device\r\n");
                    return None;
                }
            };

            crate::kernel::uart_write_string("http_get: Got network device\r\n");

            let our_mac = devices[0].mac_address();
            let our_ip = crate::kernel::OUR_IP;
            let gateway_ip = crate::kernel::GATEWAY_IP;
            let gateway_mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02]; // QEMU user-mode gateway

            // Step 1: Resolve domain name to IP (or parse if already an IP)
            let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(host) {
                crate::kernel::uart_write_string(&alloc::format!("http_get: Parsed IP directly: {:?}\r\n", ip));
                ip
            } else {
                crate::kernel::uart_write_string("http_get: Need DNS resolution\r\n");
                // Need DNS resolution
                let dns_server = [8, 8, 8, 8];
                static mut BROWSER_DNS_QUERY_ID: u16 = 200;
                let query_id = BROWSER_DNS_QUERY_ID;
                BROWSER_DNS_QUERY_ID = BROWSER_DNS_QUERY_ID.wrapping_add(1);

                let dns_query = crate::system::net::dns::build_dns_query(
                    host, crate::system::net::dns::DNS_TYPE_A, query_id);
                let udp_packet = crate::system::net::network::build_udp(
                    our_ip, dns_server, 12345, 53, &dns_query);
                let ip_packet = crate::system::net::network::build_ipv4(
                    our_ip, dns_server,
                    crate::system::net::network::IP_PROTO_UDP,
                    &udp_packet, query_id);
                let eth_frame = crate::system::net::network::build_ethernet(
                    gateway_mac, our_mac, crate::system::net::network::ETHERTYPE_IPV4, &ip_packet);

                devices[0].transmit(&eth_frame).ok()?;
                let _ = devices[0].add_receive_buffers(16);

                // Wait for DNS response
                let mut resolved_ip = None;
                for _ in 0..2000 {
                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                            // Handle ARP
                            if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                    if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                        let arp_reply = crate::system::net::network::build_arp_reply(
                                            our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                        let _ = devices[0].transmit(&arp_reply);
                                    }
                                }
                            }
                            // Handle DNS response
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_UDP {
                                        if let Some((udp_hdr, udp_payload)) = crate::system::net::network::parse_udp(ip_payload) {
                                            if crate::system::net::network::be16_to_cpu(udp_hdr.src_port) == 53 {
                                                if let Some(addresses) = crate::system::net::dns::parse_dns_response(udp_payload) {
                                                    if !addresses.is_empty() {
                                                        resolved_ip = Some(addresses[0]);
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    crate::kernel::drivers::timer::delay_ms(1);  // 1ms delay between checks
                }

                resolved_ip?
            };

            // Step 2: Establish TCP connection
            crate::kernel::uart_write_string(&alloc::format!("http_get: Connecting to {:?}:{}\r\n", server_ip, port));

            static mut BROWSER_LOCAL_PORT: u16 = 60000;
            let local_port = BROWSER_LOCAL_PORT;
            BROWSER_LOCAL_PORT = BROWSER_LOCAL_PORT.wrapping_add(1);

            let mut conn = crate::system::net::tcp::TcpConnection::new(
                our_ip, server_ip, local_port, port);

            // Send SYN
            conn.connect(&mut devices[0], gateway_mac, our_mac).ok()?;
            crate::kernel::uart_write_string("http_get: SYN sent, waiting for SYN-ACK...\r\n");

            // Wait for SYN-ACK
            let mut connection_established = false;
            let mut packets_received = 0;
            for i in 0..2000 {
                let mut rx_buffer = [0u8; 1526];
                if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                    packets_received += 1;
                    if i % 100 == 0 {
                        crate::kernel::uart_write_string(&alloc::format!("http_get: Received packet {} (len={})\r\n", packets_received, len));
                    }
                    if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                        let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                        // Handle ARP
                        if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                            if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                    let arp_reply = crate::system::net::network::build_arp_reply(
                                        our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                    let _ = devices[0].transmit(&arp_reply);
                                }
                            }
                        }
                        // Handle TCP
                        else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                            if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                    if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                        if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                            if conn.handle_segment(&tcp_hdr, tcp_data).is_ok() {
                                                if conn.state == crate::system::net::tcp::TcpState::Established {
                                                    connection_established = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                crate::kernel::drivers::timer::delay_ms(1);  // 1ms delay between checks
            }

            if !connection_established {
                crate::kernel::uart_write_string(&alloc::format!("http_get: Connection failed - no SYN-ACK (received {} packets total)\r\n", packets_received));
                return None;
            }

            crate::kernel::uart_write_string("http_get: Connection established!\r\n");

            // Send ACK to complete handshake
            conn.send_ack(&mut devices[0], gateway_mac, our_mac).ok()?;
            crate::kernel::uart_write_string("http_get: ACK sent\r\n");

            // Step 3: Send HTTP GET request
            let http_request = alloc::format!(
                "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path, host
            );

            crate::kernel::uart_write_string(&alloc::format!("http_get: Sending HTTP request: {}\r\n", http_request));
            conn.send_data(&mut devices[0], gateway_mac, our_mac, http_request.as_bytes()).ok()?;
            crate::kernel::uart_write_string("http_get: HTTP request sent, waiting for response...\r\n");

            // Step 4: Receive HTTP response
            let mut response = String::new();
            let mut no_data_count = 0;
            let mut connection_closed_by_server = false;
            let mut fin_already_acked = false;  // Track if we've already ACKed the FIN
            for _ in 0..10000 {  // Increased iterations
                let mut rx_buffer = [0u8; 1526];
                if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                    no_data_count = 0;  // Reset timeout counter when we get data

                    if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                        let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                        // Handle ARP
                        if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                            if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                    let arp_reply = crate::system::net::network::build_arp_reply(
                                        our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                    let _ = devices[0].transmit(&arp_reply);
                                }
                            }
                        }
                        // Handle TCP
                        else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                            if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                    if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                        if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                            let flags = u16::from_be(tcp_hdr.data_offset_flags) & 0x1FF;
                                            let has_fin = flags & crate::system::net::network::TCP_FLAG_FIN != 0;

                                            // First, collect any data in this packet
                                            let mut need_ack = false;
                                            if !tcp_data.is_empty() {
                                                // Check if packet contains only null bytes (likely TCP options/padding bug)
                                                let all_nulls = tcp_data.iter().all(|&b| b == 0);
                                                if all_nulls {
                                                    crate::kernel::uart_write_string(&alloc::format!("http_get: WARNING: Ignoring packet with {} null bytes (NOT ACKing)\r\n", tcp_data.len()));
                                                    // Don't add null bytes to response, and DON'T ACK them
                                                    // The null bytes aren't real data, so ACKing them causes us to skip real bytes!
                                                } else {
                                                    // Show first 20 bytes for debugging
                                                    let preview_len = tcp_data.len().min(20);
                                                    let preview: alloc::vec::Vec<u8> = tcp_data[..preview_len].to_vec();
                                                    crate::kernel::uart_write_string(&alloc::format!("http_get: Packet has {} bytes, first {} bytes: {:?}\r\n", tcp_data.len(), preview_len, preview));

                                                    if let Ok(text) = core::str::from_utf8(tcp_data) {
                                                        response.push_str(text);
                                                        crate::kernel::uart_write_string(&alloc::format!("http_get: Added {} bytes data, total now: {}\r\n", tcp_data.len(), response.len()));
                                                    } else {
                                                        crate::kernel::uart_write_string(&alloc::format!("http_get: WARNING: Skipped {} bytes (invalid UTF-8)\r\n", tcp_data.len()));
                                                    }
                                                    // Update ACK number for the data
                                                    conn.ack_num = conn.ack_num.wrapping_add(tcp_data.len() as u32);
                                                    need_ack = true;
                                                }
                                            }

                                            // Then, if FIN flag is set AND we haven't ACKed it yet, ACK it (FIN consumes 1 sequence number)
                                            if has_fin && !fin_already_acked {
                                                crate::kernel::uart_write_string("http_get: Received FIN from server\r\n");
                                                conn.ack_num = conn.ack_num.wrapping_add(1);
                                                connection_closed_by_server = true;
                                                fin_already_acked = true;  // Mark FIN as processed
                                                need_ack = true;
                                            } else if has_fin && !tcp_data.is_empty() {
                                                // If FIN already ACKed but there's new data, still need to ACK the data
                                                need_ack = true;
                                            }

                                            // Send ONE ACK for both data and FIN (if present)
                                            if need_ack {
                                                let _ = conn.send_ack(&mut devices[0], gateway_mac, our_mac);
                                            }

                                            // If we received FIN and have some response, break after a short delay
                                            if connection_closed_by_server && !response.is_empty() {
                                                // Wait a bit more to ensure all data arrived
                                                if no_data_count > 100 {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    no_data_count += 1;
                    // Increase timeout threshold significantly (3000 iterations instead of 500)
                    // Only break after receiving no data for a while AND we have some response
                    if no_data_count > 3000 && !response.is_empty() {
                        crate::kernel::uart_write_string(&alloc::format!("http_get: Timeout after {} iterations with no data\r\n", no_data_count));
                        break;
                    }
                    // If server closed connection and we haven't received new data in a while, break
                    if connection_closed_by_server && no_data_count > 100 {
                        crate::kernel::uart_write_string("http_get: Server closed connection, finishing up\r\n");
                        break;
                    }
                }
                crate::kernel::drivers::timer::delay_ms(1);  // 1ms delay between checks
            }

            // Close our side of the connection properly if not already closed
            if conn.state == crate::system::net::tcp::TcpState::Established {
                crate::kernel::uart_write_string("http_get: Closing connection\r\n");
                let _ = conn.close(&mut devices[0], gateway_mac, our_mac);
                // Wait briefly for FIN-ACK
                for _ in 0..100 {
                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            if crate::system::net::network::be16_to_cpu(frame.ethertype) == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                        if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                            if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                                let _ = conn.handle_segment(&tcp_hdr, tcp_data);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    crate::kernel::drivers::timer::delay_ms(1);  // 1ms delay between checks
                }
            }

            // Drain receive queue briefly to remove any stale packets
            // Exit early if no packets are arriving
            crate::kernel::uart_write_string("http_get: Draining receive queue...\r\n");
            let start_time = crate::kernel::drivers::timer::get_time_ms();
            let mut drained = 0;
            let mut no_packet_count = 0;
            // Drain for up to 1000ms, but exit early if no packets for 100ms
            while crate::kernel::drivers::timer::get_time_ms() - start_time < 1000 {
                let mut rx_buffer = [0u8; 1526];
                if let Ok(_) = devices[0].receive(&mut rx_buffer) {
                    drained += 1;
                    no_packet_count = 0;  // Reset counter when packet received
                } else {
                    no_packet_count += 1;
                    if no_packet_count > 50 {  // 50 * 2ms = 100ms without packets
                        break;  // Exit early - no more packets coming
                    }
                }
                crate::kernel::drivers::timer::delay_ms(2);
            }
            crate::kernel::uart_write_string(&alloc::format!("http_get: Drained {} packets\r\n", drained));

            // Replenish receive buffers after draining to ensure next connection has buffers
            let _ = devices[0].add_receive_buffers(8);
            crate::kernel::uart_write_string("http_get: Replenished 8 receive buffers\r\n");

            // Step 5: Extract HTML body from HTTP response
            crate::kernel::uart_write_string(&alloc::format!("http_get: Received {} bytes\r\n", response.len()));

            if response.is_empty() {
                crate::kernel::uart_write_string("http_get: No response received\r\n");
                return None;
            }

            crate::kernel::uart_write_string("http_get: Extracting HTML body\r\n");
            crate::kernel::uart_write_string(&alloc::format!("http_get: Full response:\r\n{}\r\n--- END RESPONSE ---\r\n", response));

            // Find the blank line that separates headers from body
            let result = if let Some(body_start) = response.find("\r\n\r\n") {
                crate::kernel::uart_write_string(&alloc::format!("http_get: Found CRLF separator at position {}\r\n", body_start));
                Some(response[body_start + 4..].to_string())
            } else if let Some(body_start) = response.find("\n\n") {
                crate::kernel::uart_write_string(&alloc::format!("http_get: Found LF separator at position {}\r\n", body_start));
                Some(response[body_start + 2..].to_string())
            } else {
                crate::kernel::uart_write_string("http_get: No separator found, returning whole response\r\n");
                Some(response)
            };

            crate::kernel::uart_write_string(&alloc::format!("http_get: Extracted body length: {}\r\n", result.as_ref().map(|s| s.len()).unwrap_or(0)));

            // Debug: Print the actual body bytes to verify integrity
            if let Some(ref body) = result {
                crate::kernel::uart_write_string(&alloc::format!("http_get: Body first 100 chars: {}\r\n",
                    if body.len() > 100 { &body[..100] } else { body }));
                crate::kernel::uart_write_string(&alloc::format!("http_get: Body last 100 chars: {}\r\n",
                    if body.len() > 100 { &body[body.len()-100..] } else { body }));
            }

            crate::kernel::uart_write_string("http_get: Done!\r\n");
            result
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
                    self.layout_node(node, x, y, max_width, &Color::BLACK, false, false, 2, "");
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
                let char_width = CHAR_WIDTH * font_size;
                let char_height = CHAR_HEIGHT * font_size;

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
                        font_size,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                        element_id: element_id.to_string(),
                    });

                    current_x += word_width + char_width;
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

        // Determine color, style, and font size
        let color = parent_color;
        let bold = parent_bold || matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong");
        let italic = parent_italic || matches!(tag, "i" | "em");
        let font_size = match tag {
            "h1" => 5,
            "h2" => 4,
            "h3" => 3,
            "h4" => 2,
            "h5" => 2,
            "h6" => 2,
            _ => parent_font_size,
        };

        // Handle special tags
        match tag {
            "br" => {
                return (x, current_y + CHAR_HEIGHT + 2);
            }
            "hr" => {
                // Horizontal rule - draw a line across the page
                // Add spacing before
                if !self.layout.is_empty() {
                    current_y += CHAR_HEIGHT + 4;
                }

                // Draw horizontal line using dashes
                let line_width = max_width.saturating_sub(20); // Leave 10px margin on each side
                let num_dashes = line_width / (CHAR_WIDTH * font_size);
                let hr_line = alloc::format!("{}", "-".repeat(num_dashes));

                self.layout.push(LayoutBox {
                    x: x + 10,
                    y: current_y,
                    width: num_dashes * CHAR_WIDTH * font_size,
                    height: CHAR_HEIGHT * font_size,
                    text: hr_line,
                    color: Color::new(128, 128, 128), // Gray
                    font_size,
                    is_link: false,
                    link_url: String::new(),
                    bold: false,
                    italic: false,
                    element_id: element_id.to_string(),
                });

                // Add spacing after
                current_y += CHAR_HEIGHT * font_size + 4;
                return (x, current_y);
            }
            "a" => {
                // Hyperlink - render children with link color
                let link_url = elem.attributes.get("href").cloned().unwrap_or_default();
                let link_color = Color::new(0, 0, 255); // Blue

                for child in &node.children {
                    let start_idx = self.layout.len();
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, &link_color, bold, italic, font_size, element_id);

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
                    current_y += font_size * CHAR_HEIGHT; // Extra spacing before heading (scales with size)
                }

                for child in &node.children {
                    let (new_x, new_y) = self.layout_node(child, current_x, current_y, max_width, color, bold, italic, font_size, element_id);
                    current_x = new_x;
                    current_y = new_y;
                }

                // Add height of the text + spacing after (2x for text height + bottom spacing)
                current_y += 2 * font_size * CHAR_HEIGHT;
                return (x, current_y);
            }
            "ul" | "ol" => {
                // Lists - use small fixed indent to prevent excessive nesting
                const LIST_INDENT: usize = 32; // Small fixed indent per nesting level

                // Add extra spacing before nested lists (x > 10 means we're indented)
                if !self.layout.is_empty() && x > 10 {
                    current_y += CHAR_HEIGHT * font_size / 2; // Extra space before nested list
                }

                for (i, child) in node.children.iter().enumerate() {
                    // Save the starting Y position for this list item
                    let list_item_y = current_y;

                    // Add bullet or number (use ASCII * since bullet • is not in ASCII)
                    let bullet = if tag == "ul" { "* " } else { &alloc::format!("{}. ", i + 1) };
                    let bullet_width = bullet.len() * CHAR_WIDTH * font_size;

                    // Layout the list item content first to get its starting position
                    let content_start_idx = self.layout.len();
                    let (_, new_y) = self.layout_node(child, current_x + LIST_INDENT, list_item_y, max_width - LIST_INDENT, color, bold, italic, font_size, element_id);

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
                        height: CHAR_HEIGHT * font_size,
                        text: bullet.to_string(),
                        color: *color,
                        font_size,
                        is_link: false,
                        link_url: String::new(),
                        bold,
                        italic,
                        element_id: element_id.to_string(),
                    });

                    current_y = new_y + CHAR_HEIGHT * font_size + 2;
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
            let (new_x, new_y) = self.layout_node(child, child_x, current_y, max_width, color, bold, italic, font_size, element_id);
            current_x = new_x;
            current_y = new_y;
        }

        // Block elements end with newline
        if is_block {
            (x, current_y + CHAR_HEIGHT * font_size + 2)
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
        self.draw_text(fb, fb_width, fb_height, win_x + win_width - 80, win_y + 7, "[<]", &Color::new(100, 100, 100), 2);

        // Forward button
        self.draw_text(fb, fb_width, fb_height, win_x + win_width - 50, win_y + 7, "[>]", &Color::new(100, 100, 100), 2);

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

    /// Draw text
    fn draw_text(&self, fb: &mut [u32], fb_width: usize, fb_height: usize, x: usize, y: usize, text: &str, color: &Color, font_size: usize) {
        let mut current_x = x;
        for ch in text.chars() {
            if ch.is_ascii() {
                let glyph = FONT_8X8[ch as usize];
                // Scale the 8x8 bitmap by font_size
                for row in 0..8 {
                    for col in 0..8 {
                        if (glyph[row] & (1 << (7 - col))) != 0 {
                            // Draw a font_size x font_size block for each pixel in the glyph
                            for dy in 0..font_size {
                                for dx in 0..font_size {
                                    let fb_x = current_x + col * font_size + dx;
                                    let fb_y = y + row * font_size + dy;
                                    if fb_x < fb_width && fb_y < fb_height {
                                        fb[fb_y * fb_width + fb_x] = color.to_u32();
                                    }
                                }
                            }
                        }
                    }
                }
            }
            current_x += CHAR_WIDTH * font_size;
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
                // Ctrl+L - focus address bar
                self.url_focused = true;
                self.url_input.set_text(&self.url);
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

            if rel_x > win_width - 80 && rel_x < win_width - 60 {
                // Back button
                self.go_back();
            } else if rel_x > win_width - 50 && rel_x < win_width - 30 {
                // Forward button
                self.go_forward();
            } else if rel_x >= input_x && rel_x < input_x + input_width
                   && rel_y >= input_y && rel_y < input_y + input_height {
                // Click inside URL input field - focus it
                self.url_focused = true;
                self.url_input.set_text(&self.url);
                // Handle mouse down for cursor positioning
                self.url_input.handle_mouse_down(rel_x as i32, (input_x + 4) as i32);
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
