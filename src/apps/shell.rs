// Simple interactive shell for file operations

use crate::system::fs::filesystem::SimpleFilesystem;
use crate::kernel::uart_write_string;
use crate::gui::widgets::console;
extern crate alloc;

const MAX_COMMAND_LEN: usize = 128;

pub struct Shell {
    command_buffer: [u8; MAX_COMMAND_LEN],
    cursor_pos: usize,
    pub filesystem: Option<SimpleFilesystem>,
    pub device_index: Option<usize>,
    console_id: usize, // ID of the console instance for this shell
}

impl Shell {
    pub fn new(console_id: usize) -> Self {
        Shell {
            command_buffer: [0; MAX_COMMAND_LEN],
            cursor_pos: 0,
            filesystem: None,
            device_index: None,
            console_id,
        }
    }

    // Helper function to write to both UART and GUI console
    fn write_output(&self, s: &str) {
        uart_write_string(s); // Keep UART for debugging
        console::write_string(self.console_id, s); // Display in GUI
    }

    pub fn set_filesystem(&mut self, fs: SimpleFilesystem, device_idx: usize) {
        self.filesystem = Some(fs);
        self.device_index = Some(device_idx);
    }

    pub fn show_prompt(&self) {
        self.write_output("> ");
    }

    pub fn handle_char(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => {
                // Execute command
                self.write_output("\r\n");
                self.execute_command();
                self.cursor_pos = 0;
                self.command_buffer = [0; MAX_COMMAND_LEN];
                self.show_prompt();
            }
            8 | 127 => {
                // Backspace
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.command_buffer[self.cursor_pos] = 0;
                    self.write_output("\x08 \x08"); // Backspace, space, backspace
                }
            }
            _ => {
                // Regular character
                if self.cursor_pos < MAX_COMMAND_LEN - 1 {
                    self.command_buffer[self.cursor_pos] = ch;
                    self.cursor_pos += 1;
                    // Echo the character to UART and GUI
                    unsafe {
                        core::ptr::write_volatile(0x09000000 as *mut u8, ch);
                    }
                    console::write_char(self.console_id, ch);
                }
            }
        }
    }

    fn execute_command(&mut self) {
        // Copy command to avoid borrow issues
        let mut cmd_copy = [0u8; MAX_COMMAND_LEN];
        cmd_copy[..self.cursor_pos].copy_from_slice(&self.command_buffer[..self.cursor_pos]);

        let cmd_str = core::str::from_utf8(&cmd_copy[..self.cursor_pos])
            .unwrap_or("")
            .trim();

        if cmd_str.is_empty() {
            return;
        }

        let parts: alloc::vec::Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "help" => self.cmd_help(),
            "ls" => self.cmd_ls(),
            "cat" => self.cmd_cat(&parts),
            "create" => self.cmd_create(&parts),
            "rm" => self.cmd_rm(&parts),
            "rename" | "mv" => self.cmd_rename(&parts),
            "write" => self.cmd_write(&parts),
            "edit" => self.cmd_edit(&parts),
            "clear" => self.cmd_clear(),
            "ifconfig" => self.cmd_ifconfig(),
            "ping" => self.cmd_ping(&parts),
            "arp" => self.cmd_arp(),
            "nslookup" | "dig" => self.cmd_nslookup(&parts),
            "http" | "wget" => self.cmd_http(&parts),
            "httptest" => self.cmd_httptest(),
            _ => {
                self.write_output("Unknown command: ");
                self.write_output(parts[0]);
                self.write_output("\r\nType 'help' for available commands\r\n");
            }
        }
    }

    fn cmd_help(&self) {
        self.write_output("Available commands:\r\n");
        self.write_output("  ls                    - List files\r\n");
        self.write_output("  cat <filename>        - Show file contents\r\n");
        self.write_output("  create <name> <size>  - Create a file\r\n");
        self.write_output("  rm <filename>         - Delete a file\r\n");
        self.write_output("  rename <old> <new>    - Rename a file\r\n");
        self.write_output("  write <file> <text>   - Write text to file\r\n");
        self.write_output("  edit <filename>       - Open file in editor\r\n");
        self.write_output("  clear                 - Clear screen\r\n");
        self.write_output("\r\nNetwork commands:\r\n");
        self.write_output("  ifconfig              - Show network configuration\r\n");
        self.write_output("  ping <ip>             - Ping a host (e.g. ping 8.8.8.8)\r\n");
        self.write_output("  nslookup <domain>     - Resolve domain to IP (e.g. nslookup google.com)\r\n");
        self.write_output("  http <url>            - HTTP GET request (e.g. http example.com)\r\n");
        self.write_output("  arp                   - Show ARP cache\r\n");
        self.write_output("  help                  - Show this help\r\n");
    }

    fn cmd_ls(&mut self) {
        if let Some(ref fs) = self.filesystem {
            let files = fs.list_files();
            if files.is_empty() {
                self.write_output("No files\r\n");
            } else {
                self.write_output(&alloc::format!("{} file(s):\r\n", files.len()));
                for file in files {
                    self.write_output(&alloc::format!(
                        "  {} - {} bytes\r\n",
                        file.get_name(),
                        file.get_size_bytes()
                    ));
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_cat(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: cat <filename>\r\n");
            return;
        }

        if let (Some(ref fs), Some(idx)) = (&self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        // Get file size
                        let files = fs.list_files();
                        let file = files.iter().find(|f| f.get_name() == filename);

                        if let Some(file) = file {
                            let size = file.get_size_bytes() as usize;
                            let mut buffer = alloc::vec![0u8; size];

                            match fs.read_file(device, filename, &mut buffer) {
                                Ok(bytes_read) => {
                                    if let Ok(text) = core::str::from_utf8(&buffer[..bytes_read]) {
                                        self.write_output(text);
                                        self.write_output("\r\n");
                                    } else {
                                        self.write_output("(binary file)\r\n");
                                    }
                                }
                                Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            self.write_output("File not found\r\n");
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_create(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            self.write_output("Usage: create <filename> <size>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        if let Ok(size) = parts[2].parse::<u32>() {
                            match fs.create_file(device, filename, size) {
                                Ok(()) => {
                                    self.write_output(&alloc::format!(
                                        "Created '{}' ({} bytes)\r\n", filename, size
                                    ));
                                    // Refresh all open file explorers to show the new file
                                    crate::gui::widgets::file_explorer::refresh_all_explorers();
                                }
                                Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            self.write_output("Invalid size\r\n");
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_rm(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: rm <filename>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        match fs.delete_file(device, filename) {
                            Ok(()) => {
                                self.write_output(&alloc::format!("Deleted '{}'\r\n", filename));
                                // Refresh all open file explorers to remove the deleted file
                                crate::gui::widgets::file_explorer::refresh_all_explorers();
                            }
                            Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_rename(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            self.write_output("Usage: rename <old_name> <new_name>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let old_name = parts[1];
                        let new_name = parts[2];

                        match fs.rename_file(device, old_name, new_name) {
                            Ok(()) => {
                                self.write_output(&alloc::format!(
                                    "Renamed '{}' to '{}'\r\n", old_name, new_name
                                ));
                                // Refresh all open file explorers to show the renamed file
                                crate::gui::widgets::file_explorer::refresh_all_explorers();
                            }
                            Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_write(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            self.write_output("Usage: write <filename> <text...>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];
                        let text = parts[2..].join(" ");

                        match fs.write_file(device, filename, text.as_bytes()) {
                            Ok(()) => {
                                self.write_output(&alloc::format!(
                                    "Wrote {} bytes to '{}'\r\n", text.len(), filename
                                ));
                                // Refresh all open file explorers to update file sizes
                                crate::gui::widgets::file_explorer::refresh_all_explorers();
                            }
                            Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_edit(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: edit <filename>\r\n");
            return;
        }

        let filename = parts[1];

        // Check if the editor window already exists
        if crate::gui::window_manager::has_focused_editor() {
            self.write_output("Editor window is already open\r\n");
            return;
        }

        if let (Some(ref fs), Some(idx)) = (&self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        // Check if file exists
                        let files = fs.list_files();
                        let file = files.iter().find(|f| f.get_name() == filename);

                        if let Some(file) = file {
                            let size = file.get_size_bytes() as usize;
                            let mut buffer = alloc::vec![0u8; size];

                            match fs.read_file(device, filename, &mut buffer) {
                                Ok(bytes_read) => {
                                    // Find the actual content length (stop at first null byte or end)
                                    let actual_len = buffer[..bytes_read].iter()
                                        .position(|&b| b == 0)
                                        .unwrap_or(bytes_read);

                                    if let Ok(text) = core::str::from_utf8(&buffer[..actual_len]) {
                                        // Create editor instance with file content
                                        let editor_id = crate::gui::widgets::editor::create_editor_with_content(
                                            filename,
                                            text
                                        );

                                        // Open editor window
                                        let window = crate::gui::window_manager::Window::new(
                                            0, 0, 640, 480,
                                            &alloc::format!("Text Editor - {}", filename),
                                            crate::gui::window_manager::WindowContent::Editor,
                                            editor_id
                                        );
                                        crate::gui::window_manager::add_window(window);

                                        self.write_output(&alloc::format!("Opened '{}' in editor\r\n", filename));
                                    } else {
                                        self.write_output("Cannot edit binary file\r\n");
                                    }
                                }
                                Err(e) => self.write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            self.write_output("File not found\r\n");
                        }
                    } else {
                        self.write_output("Block device not available\r\n");
                    }
                } else {
                    self.write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            self.write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_clear(&self) {
        // Clear UART terminal with ANSI escape sequence
        uart_write_string("\x1b[2J\x1b[H");
        // Clear GUI console
        console::clear(self.console_id);
    }

    fn cmd_ifconfig(&self) {
        unsafe {
            if let Some(ref devices) = crate::kernel::NET_DEVICES {
                if devices.is_empty() {
                    self.write_output("No network devices found\r\n");
                    return;
                }

                let mac = devices[0].mac_address();
                let ip = crate::kernel::OUR_IP;
                let gateway = crate::kernel::GATEWAY_IP;

                self.write_output("eth0:\r\n");
                self.write_output(&alloc::format!("  MAC: {}\r\n",
                    crate::system::net::network::format_mac(mac)));
                self.write_output(&alloc::format!("  IP: {}\r\n",
                    crate::system::net::network::format_ip(ip)));
                self.write_output(&alloc::format!("  Gateway: {}\r\n",
                    crate::system::net::network::format_ip(gateway)));
            } else {
                self.write_output("Network not initialized\r\n");
            }
        }
    }

    fn cmd_arp(&self) {
        unsafe {
            if let Some(ref cache) = crate::kernel::ARP_CACHE {
                self.write_output("ARP Cache:\r\n");
                // This is a simple implementation - in a real OS we'd iterate the cache
                self.write_output("  (ARP cache display not yet implemented)\r\n");
            } else {
                self.write_output("ARP cache not initialized\r\n");
            }
        }
    }

    fn cmd_ping(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: ping <ip address>\r\n");
            self.write_output("Example: ping 8.8.8.8\r\n");
            return;
        }

        let target_ip_str = parts[1];
        let target_ip = match crate::system::net::network::parse_ip(target_ip_str) {
            Some(ip) => ip,
            None => {
                self.write_output("Invalid IP address\r\n");
                return;
            }
        };

        self.write_output(&alloc::format!("PING {} ...\r\n", target_ip_str));

        unsafe {
            if let (Some(ref mut devices), Some(ref mut cache)) =
                (crate::kernel::NET_DEVICES.as_mut(), crate::kernel::ARP_CACHE.as_mut()) {

                if devices.is_empty() {
                    self.write_output("No network device available\r\n");
                    return;
                }

                let our_mac = devices[0].mac_address();
                let our_ip = crate::kernel::OUR_IP;
                let gateway_ip = crate::kernel::GATEWAY_IP;

                // Determine target MAC (use gateway for non-local addresses)
                let use_gateway = target_ip[0] != 10; // Simple check - not same /8
                let arp_target = if use_gateway { gateway_ip } else { target_ip };

                // QEMU user networking doesn't respond to ARP, so hardcode gateway MAC
                // QEMU uses MAC format: 52:55:0a:00:02:02 for IP 10.0.2.2
                let target_mac = if arp_target == gateway_ip {
                    // Hardcoded QEMU user-mode gateway MAC
                    self.write_output("Using QEMU gateway MAC (user-mode networking doesn't do ARP)\r\n");
                    [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02]
                } else {
                    // For non-gateway addresses, try ARP
                    if let Some(mac) = cache.lookup(arp_target) {
                        mac
                    } else {
                        // Send ARP request
                        self.write_output(&alloc::format!("Resolving {} via ARP...\r\n",
                            crate::system::net::network::format_ip(arp_target)));

                        let arp_request = crate::system::net::network::build_arp_request(
                            our_mac, our_ip, arp_target);

                        if let Err(e) = devices[0].transmit(&arp_request) {
                            self.write_output(&alloc::format!("Failed to send ARP: {}\r\n", e));
                            return;
                        }

                        // Wait for ARP reply (simple polling with timeout)
                        let mut found_mac = None;
                        for _ in 0..1000 {
                            let mut rx_buffer = [0u8; 1526];
                            if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                                if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                                    if crate::system::net::network::be16_to_cpu(frame.ethertype) == crate::system::net::network::ETHERTYPE_ARP {
                                        if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                            if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REPLY {
                                                if arp.sender_ip == arp_target {
                                                    cache.add(arp.sender_ip, arp.sender_mac);
                                                    found_mac = Some(arp.sender_mac);
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Small delay
                            for _ in 0..10000 {
                                core::arch::asm!("nop");
                            }
                        }

                        match found_mac {
                            Some(mac) => mac,
                            None => {
                                self.write_output("ARP timeout - no response\r\n");
                                return;
                            }
                        }
                    }
                };

                // Now send ICMP echo request
                let icmp_payload = b"rOSt ping!";
                let icmp_packet = crate::system::net::network::build_icmp_echo_request(
                    1234, 1, icmp_payload);
                let ip_packet = crate::system::net::network::build_ipv4(
                    our_ip, target_ip, crate::system::net::network::IP_PROTO_ICMP, &icmp_packet, 1);
                let eth_frame = crate::system::net::network::build_ethernet(
                    target_mac, our_mac, crate::system::net::network::ETHERTYPE_IPV4, &ip_packet);

                self.write_output(&alloc::format!(
                    "Sending to MAC {} IP {}\r\n",
                    crate::system::net::network::format_mac(target_mac),
                    crate::system::net::network::format_ip(target_ip)
                ));

                if let Err(e) = devices[0].transmit(&eth_frame) {
                    self.write_output(&alloc::format!("Failed to send ping: {}\r\n", e));
                    return;
                }

                self.write_output("Ping sent, waiting for reply...\r\n");

                // Replenish receive buffers before waiting for reply
                if let Err(e) = devices[0].add_receive_buffers(16) {
                    self.write_output(&alloc::format!("Warning: Failed to add more RX buffers: {}\r\n", e));
                }

                // Wait for ICMP echo reply
                for _ in 0..2000 {

                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                            // Handle ARP requests
                            if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                    let arp_op = crate::system::net::network::be16_to_cpu(arp.operation);
                                    if arp_op == crate::system::net::network::ARP_REQUEST {
                                        // Is this ARP request for us?
                                        if arp.target_ip == our_ip {
                                            self.write_output(&alloc::format!(
                                                "[ARP] Got request from {} - sending reply\r\n",
                                                crate::system::net::network::format_ip(arp.sender_ip)
                                            ));

                                            // Send ARP reply
                                            let arp_reply = crate::system::net::network::build_arp_reply(
                                                our_mac, our_ip, arp.sender_mac, arp.sender_ip
                                            );
                                            let _ = devices[0].transmit(&arp_reply);
                                        }
                                    }
                                }
                            }
                            // Handle IPv4 packets
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_ICMP {
                                        if let Some((icmp_hdr, _)) = crate::system::net::network::parse_icmp(ip_payload) {
                                            if icmp_hdr.icmp_type == crate::system::net::network::ICMP_ECHO_REPLY {
                                                self.write_output(&alloc::format!(
                                                    "Reply from {}: seq={}\r\n",
                                                    crate::system::net::network::format_ip(ip_hdr.src_ip),
                                                    crate::system::net::network::be16_to_cpu(icmp_hdr.sequence)
                                                ));
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Small delay
                    for _ in 0..10000 {
                        core::arch::asm!("nop");
                    }
                }

                self.write_output("Request timeout - no reply\r\n");
            } else {
                self.write_output("Network not initialized\r\n");
            }
        }
    }

    fn cmd_nslookup(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: nslookup <domain>\r\n");
            self.write_output("Example: nslookup google.com\r\n");
            return;
        }

        let domain = parts[1];
        self.write_output(&alloc::format!("Resolving {} ...\r\n", domain));

        unsafe {
            if let Some(ref mut devices) = crate::kernel::NET_DEVICES.as_mut() {
                if devices.is_empty() {
                    self.write_output("No network device available\r\n");
                    return;
                }

                let our_mac = devices[0].mac_address();
                let our_ip = crate::kernel::OUR_IP;
                let gateway_ip = crate::kernel::GATEWAY_IP;

                // Use Google's public DNS server
                let dns_server = [8, 8, 8, 8];

                // Hardcoded QEMU gateway MAC (user-mode networking doesn't do ARP)
                let gateway_mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

                // Build DNS query
                static mut QUERY_ID: u16 = 1;
                let query_id = QUERY_ID;
                QUERY_ID = QUERY_ID.wrapping_add(1);

                let dns_query = crate::system::net::dns::build_dns_query(
                    domain, crate::system::net::dns::DNS_TYPE_A, query_id);

                // Build UDP packet (source port 12345, dest port 53)
                let udp_packet = crate::system::net::network::build_udp(
                    our_ip, dns_server, 12345, 53, &dns_query);

                // Build IPv4 packet
                let ip_packet = crate::system::net::network::build_ipv4(
                    our_ip, dns_server,
                    crate::system::net::network::IP_PROTO_UDP,
                    &udp_packet, query_id);

                // Build Ethernet frame
                let eth_frame = crate::system::net::network::build_ethernet(
                    gateway_mac, our_mac, crate::system::net::network::ETHERTYPE_IPV4, &ip_packet);

                // Send the DNS query
                if let Err(e) = devices[0].transmit(&eth_frame) {
                    self.write_output(&alloc::format!("Failed to send DNS query: {}\r\n", e));
                    return;
                }

                self.write_output("DNS query sent, waiting for response...\r\n");

                // Replenish receive buffers
                if let Err(e) = devices[0].add_receive_buffers(16) {
                    self.write_output(&alloc::format!("Warning: Failed to add RX buffers: {}\r\n", e));
                }

                // Wait for DNS response
                for _ in 0..2000 {
                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                            // Handle ARP requests (QEMU needs to learn our MAC)
                            if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                    let arp_op = crate::system::net::network::be16_to_cpu(arp.operation);
                                    if arp_op == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                        let arp_reply = crate::system::net::network::build_arp_reply(
                                            our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                        let _ = devices[0].transmit(&arp_reply);
                                    }
                                }
                            }
                            // Handle IPv4 packets
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    // Check if this is UDP
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_UDP {
                                        if let Some((udp_hdr, udp_payload)) = crate::system::net::network::parse_udp(ip_payload) {
                                            let src_port = crate::system::net::network::be16_to_cpu(udp_hdr.src_port);
                                            let dst_port = crate::system::net::network::be16_to_cpu(udp_hdr.dst_port);

                                            // Check if this is a DNS response (from port 53 to our port 12345)
                                            if src_port == 53 && dst_port == 12345 {
                                                // Parse DNS response
                                                if let Some(addresses) = crate::system::net::dns::parse_dns_response(udp_payload) {
                                                    if addresses.is_empty() {
                                                        self.write_output("No A records found\r\n");
                                                    } else {
                                                        self.write_output(&alloc::format!("Resolved {} to:\r\n", domain));
                                                        for addr in addresses {
                                                            self.write_output(&alloc::format!("  {}\r\n",
                                                                crate::system::net::network::format_ip(addr)));
                                                        }
                                                    }
                                                    return;
                                                } else {
                                                    self.write_output("Failed to parse DNS response\r\n");
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Small delay
                    for _ in 0..10000 {
                        core::arch::asm!("nop");
                    }
                }

                self.write_output("DNS timeout - no response\r\n");
            } else {
                self.write_output("Network not initialized\r\n");
            }
        }
    }

    fn cmd_http(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: http <domain>\r\n");
            self.write_output("Example: http example.com\r\n");
            return;
        }

        let domain = parts[1];
        self.write_output(&alloc::format!("HTTP GET http://{}/\r\n", domain));

        unsafe {
            if let Some(ref mut devices) = crate::kernel::NET_DEVICES.as_mut() {
                if devices.is_empty() {
                    self.write_output("No network device available\r\n");
                    return;
                }

                let our_mac = devices[0].mac_address();
                let our_ip = crate::kernel::OUR_IP;
                let gateway_mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];

                // Step 1: Resolve domain name to IP (or parse if already an IP)
                let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(domain) {
                    // Already an IP address, use it directly
                    self.write_output(&alloc::format!("Using IP {}...\r\n", domain));
                    ip
                } else {
                    // Need to resolve via DNS
                    self.write_output(&alloc::format!("Resolving {}...\r\n", domain));

                    let dns_server = [8, 8, 8, 8];
                    static mut HTTP_QUERY_ID: u16 = 100;
                    let query_id = HTTP_QUERY_ID;
                    HTTP_QUERY_ID = HTTP_QUERY_ID.wrapping_add(1);

                    let dns_query = crate::system::net::dns::build_dns_query(
                        domain, crate::system::net::dns::DNS_TYPE_A, query_id);
                    let udp_packet = crate::system::net::network::build_udp(
                        our_ip, dns_server, 12345, 53, &dns_query);
                    let ip_packet = crate::system::net::network::build_ipv4(
                        our_ip, dns_server,
                        crate::system::net::network::IP_PROTO_UDP,
                        &udp_packet, query_id);
                    let eth_frame = crate::system::net::network::build_ethernet(
                        gateway_mac, our_mac, crate::system::net::network::ETHERTYPE_IPV4, &ip_packet);

                    if let Err(e) = devices[0].transmit(&eth_frame) {
                        self.write_output(&alloc::format!("Failed to send DNS query: {}\r\n", e));
                        return;
                    }

                    // Replenish receive buffers
                    let _ = devices[0].add_receive_buffers(16);

                    // Wait for DNS response
                    let mut resolved_ip = None;
                    for _ in 0..2000 {
                        let mut rx_buffer = [0u8; 1526];
                        if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                            if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                                let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                                // Handle ARP requests
                                if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                    if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                        if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                            let arp_reply = crate::system::net::network::build_arp_reply(
                                                our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                            let _ = devices[0].transmit(&arp_reply);
                                        }
                                    }
                                }
                                // Handle IPv4 packets
                                else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                    if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                        if ip_hdr.protocol == crate::system::net::network::IP_PROTO_UDP {
                                            if let Some((udp_hdr, udp_payload)) = crate::system::net::network::parse_udp(ip_payload) {
                                                if crate::system::net::network::be16_to_cpu(udp_hdr.src_port) == 53 {
                                                    if let Some(addresses) = crate::system::net::dns::parse_dns_response(udp_payload) {
                                                        if !addresses.is_empty() {
                                                            resolved_ip = Some(addresses[0]);
                                                            self.write_output(&alloc::format!("Resolved to {}\r\n",
                                                                crate::system::net::network::format_ip(addresses[0])));
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

                        for _ in 0..10000 {
                            core::arch::asm!("nop");
                        }
                    }

                    match resolved_ip {
                        Some(ip) => ip,
                        None => {
                            self.write_output("DNS resolution failed\r\n");
                            return;
                        }
                    }
                };

                // Step 2: Establish TCP connection
                self.write_output("Establishing TCP connection...\r\n");

                // Use a simple local port
                static mut HTTP_LOCAL_PORT: u16 = 50000;
                let local_port = HTTP_LOCAL_PORT;
                HTTP_LOCAL_PORT = HTTP_LOCAL_PORT.wrapping_add(1);

                let mut conn = crate::system::net::tcp::TcpConnection::new(
                    our_ip, server_ip, local_port, 80);

                // Send SYN
                if let Err(e) = conn.connect(&mut devices[0], gateway_mac, our_mac) {
                    self.write_output(&alloc::format!("Failed to send SYN: {}\r\n", e));
                    return;
                }

                // Wait for SYN-ACK
                let mut connection_established = false;
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
                            // Handle TCP
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                        if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                            // Check if this is for our connection
                                            if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                                if let Ok(()) = conn.handle_segment(&tcp_hdr, tcp_data) {
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

                    for _ in 0..10000 {
                        core::arch::asm!("nop");
                    }
                }

                if !connection_established {
                    self.write_output("Connection failed - no SYN-ACK received\r\n");
                    return;
                }

                self.write_output("Connected!\r\n");

                // Send ACK to complete handshake
                if let Err(e) = conn.send_ack(&mut devices[0], gateway_mac, our_mac) {
                    self.write_output(&alloc::format!("Failed to send ACK: {}\r\n", e));
                    return;
                }

                // Step 3: Send HTTP GET request
                let http_request = alloc::format!(
                    "GET / HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
                    domain
                );

                self.write_output("Sending HTTP request...\r\n");

                if let Err(e) = conn.send_data(&mut devices[0], gateway_mac, our_mac, http_request.as_bytes()) {
                    self.write_output(&alloc::format!("Failed to send HTTP request: {}\r\n", e));
                    return;
                }

                // Step 4: Receive HTTP response
                self.write_output("Waiting for response...\r\n");
                self.write_output("---\r\n");

                let mut response_received = false;
                for _ in 0..5000 {
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
                            // Handle TCP
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                        if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                            // Check if this is for our connection
                                            if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                                // Update connection state
                                                let _ = conn.handle_segment(&tcp_hdr, tcp_data);

                                                // Display response data
                                                if !tcp_data.is_empty() {
                                                    if let Ok(text) = core::str::from_utf8(tcp_data) {
                                                        self.write_output(text);
                                                    }
                                                    response_received = true;

                                                    // Update ACK number for received data
                                                    conn.ack_num = conn.ack_num.wrapping_add(tcp_data.len() as u32);

                                                    // Send ACK for received data
                                                    let _ = conn.send_ack(&mut devices[0], gateway_mac, our_mac);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    for _ in 0..10000 {
                        core::arch::asm!("nop");
                    }
                }

                if !response_received {
                    self.write_output("No response received\r\n");
                } else {
                    self.write_output("\r\n---\r\n");
                }

                // Close connection
                let _ = conn.close(&mut devices[0], gateway_mac, our_mac);

            } else {
                self.write_output("Network not initialized\r\n");
            }
        }
    }

    fn cmd_httptest(&mut self) {
        self.write_output("HTTP TEST: Connecting to QEMU gateway (10.0.2.2:8888)\r\n");
        self.write_output("Start HTTP server on host: python3 -m http.server 8888\r\n");
        self.write_output("Then start QEMU with: -netdev user,id=net0,hostfwd=tcp:10.0.2.2:8888-:8888\r\n\r\n");

        unsafe {
            if let Some(ref mut devices) = crate::kernel::NET_DEVICES.as_mut() {
                if devices.is_empty() {
                    self.write_output("No network device available\r\n");
                    return;
                }

                let our_mac = devices[0].mac_address();
                let our_ip = crate::kernel::OUR_IP;
                let gateway_mac = [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02];
                let server_ip = [10, 0, 2, 2];  // QEMU gateway
                let server_port = 8888u16;

                self.write_output("Establishing TCP connection...\r\n");

                static mut TEST_LOCAL_PORT: u16 = 60000;
                let local_port = TEST_LOCAL_PORT;
                TEST_LOCAL_PORT = TEST_LOCAL_PORT.wrapping_add(1);

                let mut conn = crate::system::net::tcp::TcpConnection::new(
                    our_ip, server_ip, local_port, server_port);

                // Send SYN
                if let Err(e) = conn.connect(&mut devices[0], gateway_mac, our_mac) {
                    self.write_output(&alloc::format!("Failed to send SYN: {}\r\n", e));
                    return;
                }

                // Replenish RX buffers
                let _ = devices[0].add_receive_buffers(16);

                // Wait for SYN-ACK
                let mut connection_established = false;
                for _ in 0..3000 {
                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                            if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                    if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                        let arp_reply = crate::system::net::network::build_arp_reply(
                                            our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                        let _ = devices[0].transmit(&arp_reply);
                                    }
                                }
                            }
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                        if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                            if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                                if let Ok(()) = conn.handle_segment(&tcp_hdr, tcp_data) {
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
                    for _ in 0..5000 { core::arch::asm!("nop"); }
                }

                if !connection_established {
                    self.write_output("Connection failed - no SYN-ACK\r\n");
                    self.write_output("Did you start the HTTP server and configure port forwarding?\r\n");
                    return;
                }

                self.write_output("Connected! Sending HTTP GET...\r\n");

                // Send ACK to complete handshake
                if let Err(e) = conn.send_ack(&mut devices[0], gateway_mac, our_mac) {
                    self.write_output(&alloc::format!("Failed to send ACK: {}\r\n", e));
                    return;
                }

                // Send HTTP GET request
                let http_request = "GET / HTTP/1.0\r\nHost: localhost:8888\r\nConnection: close\r\n\r\n";

                if let Err(e) = conn.send_data(&mut devices[0], gateway_mac, our_mac, http_request.as_bytes()) {
                    self.write_output(&alloc::format!("Failed to send HTTP request: {}\r\n", e));
                    return;
                }

                // Replenish RX buffers again
                let _ = devices[0].add_receive_buffers(16);

                self.write_output("Waiting for HTTP response...\r\n---\r\n");

                let mut got_response = false;
                for _ in 0..10000 {
                    let mut rx_buffer = [0u8; 1526];
                    if let Ok(len) = devices[0].receive(&mut rx_buffer) {
                        if let Some((frame, payload)) = crate::system::net::network::parse_ethernet(&rx_buffer[..len]) {
                            let ethertype = crate::system::net::network::be16_to_cpu(frame.ethertype);

                            if ethertype == crate::system::net::network::ETHERTYPE_ARP {
                                if let Some(arp) = crate::system::net::network::parse_arp(payload) {
                                    if crate::system::net::network::be16_to_cpu(arp.operation) == crate::system::net::network::ARP_REQUEST && arp.target_ip == our_ip {
                                        let arp_reply = crate::system::net::network::build_arp_reply(
                                            our_mac, our_ip, arp.sender_mac, arp.sender_ip);
                                        let _ = devices[0].transmit(&arp_reply);
                                    }
                                }
                            }
                            else if ethertype == crate::system::net::network::ETHERTYPE_IPV4 {
                                if let Some((ip_hdr, ip_payload)) = crate::system::net::network::parse_ipv4(payload) {
                                    if ip_hdr.protocol == crate::system::net::network::IP_PROTO_TCP {
                                        if let Some((tcp_hdr, tcp_data)) = crate::system::net::network::parse_tcp(ip_payload) {
                                            if crate::system::net::network::be16_to_cpu(tcp_hdr.dst_port) == local_port {
                                                let _ = conn.handle_segment(&tcp_hdr, tcp_data);

                                                if !tcp_data.is_empty() {
                                                    got_response = true;
                                                    if let Ok(text) = core::str::from_utf8(tcp_data) {
                                                        self.write_output(text);
                                                    } else {
                                                        self.write_output(&alloc::format!("[Binary data: {} bytes]\r\n", tcp_data.len()));
                                                    }

                                                    conn.ack_num = conn.ack_num.wrapping_add(tcp_data.len() as u32);
                                                    let _ = conn.send_ack(&mut devices[0], gateway_mac, our_mac);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for _ in 0..5000 { core::arch::asm!("nop"); }
                }

                if got_response {
                    self.write_output("\r\n---\r\nSUCCESS!\r\n");
                } else {
                    self.write_output("\r\n---\r\nNo response received\r\n");
                }

                let _ = conn.close(&mut devices[0], gateway_mac, our_mac);

            } else {
                self.write_output("Network not initialized\r\n");
            }
        }
    }
}

// Global shell instances
static mut SHELLS: alloc::vec::Vec<Shell> = alloc::vec::Vec::new();

/// Create a new shell instance for a console
pub fn create_shell(console_id: usize) {
    unsafe {
        let mut shell = Shell::new(console_id);

        // Initialize filesystem if block device is available
        if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
            if !devices.is_empty() {
                let device_idx = 0;
                if let Some(device) = devices.get_mut(device_idx) {
                    match crate::system::fs::filesystem::SimpleFilesystem::mount(device) {
                        Ok(fs) => {
                            shell.set_filesystem(fs, device_idx);
                            shell.write_output("Filesystem mounted\r\n");
                        }
                        Err(e) => {
                            shell.write_output(&alloc::format!("Failed to mount: {}\r\n", e));
                        }
                    }
                }
            }
        }

        shell.show_prompt();
        SHELLS.push(shell);
    }
}

/// Remove a shell instance
pub fn remove_shell(id: usize) {
    unsafe {
        if id < SHELLS.len() {
            SHELLS.remove(id);
        }
    }
}

/// Get a shell instance by ID
pub fn get_shell(id: usize) -> Option<&'static mut Shell> {
    unsafe {
        SHELLS.get_mut(id)
    }
}
