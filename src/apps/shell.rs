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
            "download" | "dl" => self.cmd_download(&parts),
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
        self.write_output("  download <url>        - Download file (e.g. download 10.0.2.2:8888/font.ttf)\r\n");
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
        self.write_output("ARP is now handled internally by smoltcp.\r\n");
        self.write_output("The network stack automatically manages ARP resolution.\r\n");
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
            if let Some(ref mut stack) = crate::kernel::NETWORK_STACK.as_mut() {
                match crate::system::net::helpers::ping(stack, target_ip, 5000) {
                    Ok(rtt_ms) => {
                        self.write_output(&alloc::format!(
                            "Reply from {}: time={}ms\r\n",
                            target_ip_str, rtt_ms
                        ));
                    }
                    Err(e) => {
                        self.write_output(&alloc::format!("Ping failed: {}\r\n", e));
                    }
                }
            } else {
                self.write_output("Network stack not initialized\r\n");
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
            if let Some(ref mut stack) = crate::kernel::NETWORK_STACK.as_mut() {
                match crate::system::net::helpers::dns_lookup(stack, domain, 5000) {
                    Ok(addresses) => {
                        self.write_output(&alloc::format!("Resolved {} to:\r\n", domain));
                        for addr in addresses {
                            self.write_output(&alloc::format!("  {}.{}.{}.{}\r\n",
                                addr[0], addr[1], addr[2], addr[3]));
                        }
                    }
                    Err(e) => {
                        self.write_output(&alloc::format!("DNS lookup failed: {}\r\n", e));
                    }
                }
            } else {
                self.write_output("Network stack not initialized\r\n");
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
            if let Some(ref mut stack) = crate::kernel::NETWORK_STACK.as_mut() {
                // Use smoltcp http_get helper
                match crate::system::net::helpers::http_get(stack, domain, "/", 80, 10000) {
                    Ok(response_data) => {
                        self.write_output("---\r\n");
                        if let Ok(text) = core::str::from_utf8(&response_data) {
                            self.write_output(text);
                        } else {
                            self.write_output(&alloc::format!("Received {} bytes (binary data)\r\n", response_data.len()));
                        }
                        self.write_output("\r\n---\r\n");
                    }
                    Err(e) => {
                        self.write_output(&alloc::format!("HTTP request failed: {}\r\n", e));
                    }
                }
            } else {
                self.write_output("Network stack not initialized\r\n");
            }
        }
    }

    fn cmd_download(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            self.write_output("Usage: download <host:port/path>\r\n");
            self.write_output("Example: download 10.0.2.2:8888/font.ttf\r\n");
            return;
        }

        let mut url = parts[1];

        // Strip http:// or https:// prefix if present
        if url.starts_with("http://") {
            url = &url[7..];
        } else if url.starts_with("https://") {
            url = &url[8..];
        }

        // Parse URL: host:port/path
        let (host, port, path) = if let Some(slash_pos) = url.find('/') {
            let host_port = &url[..slash_pos];
            let path = &url[slash_pos..];

            if let Some(colon_pos) = host_port.find(':') {
                let host = &host_port[..colon_pos];
                let port = host_port[colon_pos + 1..].parse::<u16>().unwrap_or(80);
                (host, port, path)
            } else {
                (host_port, 80, path)
            }
        } else {
            self.write_output("Invalid URL format. Expected: host:port/path\r\n");
            return;
        };

        // Extract filename from path
        let mut filename = if let Some(last_slash) = path.rfind('/') {
            &path[last_slash + 1..]
        } else {
            path
        };

        if filename.is_empty() {
            self.write_output("Cannot extract filename from URL\r\n");
            return;
        }

        // SimpleFS has 8-character filename limit - truncate if needed
        let final_filename = if filename.len() > 8 {
            // Try to preserve extension
            if let Some(dot_pos) = filename.rfind('.') {
                let ext = &filename[dot_pos..]; // includes the dot
                let name = &filename[..dot_pos];

                // If extension is reasonable length (<=4 chars including dot), keep it
                if ext.len() <= 4 && ext.len() < filename.len() {
                    let max_name_len = 8 - ext.len();
                    if name.len() > max_name_len {
                        alloc::format!("{}{}", &name[..max_name_len], ext)
                    } else {
                        alloc::string::String::from(filename)
                    }
                } else {
                    // Extension too long, just truncate whole filename
                    alloc::string::String::from(&filename[..8])
                }
            } else {
                // No extension, just truncate
                alloc::string::String::from(&filename[..8])
            }
        } else {
            alloc::string::String::from(filename)
        };

        if final_filename != filename {
            self.write_output(&alloc::format!("Note: Filename truncated to '{}' (8 char limit)\r\n", final_filename));
        }

        self.write_output(&alloc::format!("Downloading http://{}:{}{}\r\n", host, port, path));
        self.write_output(&alloc::format!("Saving to: {}\r\n", final_filename));

        // Download file using smoltcp
        let response_data = unsafe {
            if let Some(ref mut stack) = crate::kernel::NETWORK_STACK.as_mut() {
                match crate::system::net::helpers::http_get(stack, host, path, port, 30000) {
                    Ok(response_data) => {
                        self.write_output(&alloc::format!("Downloaded {} bytes (with headers)\r\n", response_data.len()));
                        response_data
                    }
                    Err(e) => {
                        self.write_output(&alloc::format!("Download failed: {}\r\n", e));
                        return;
                    }
                }
            } else {
                self.write_output("Network stack not initialized\r\n");
                return;
            }
        };

        // Extract body from HTTP response (strip headers)
        let data = if let Some(body_start) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
            let body = &response_data[body_start + 4..];
            self.write_output(&alloc::format!("Body size: {} bytes\r\n", body.len()));
            body
        } else {
            self.write_output("Warning: Could not find HTTP header separator, saving entire response\r\n");
            &response_data[..]
        };

        // Save to filesystem
        let result = if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        // Create file with appropriate size
                        match fs.create_file(device, &final_filename, data.len() as u32) {
                            Ok(()) => {
                                // Write data to file
                                match fs.write_file(device, &final_filename, &data) {
                                    Ok(()) => {
                                        // Refresh file explorers
                                        crate::gui::widgets::file_explorer::refresh_all_explorers();
                                        Ok(data.len())
                                    }
                                    Err(e) => Err(alloc::format!("Failed to write file: {}", e))
                                }
                            }
                            Err(e) => Err(alloc::format!("Failed to create file: {}", e))
                        }
                    } else {
                        Err(alloc::string::String::from("Block device not available"))
                    }
                } else {
                    Err(alloc::string::String::from("Block devices not initialized"))
                }
            }
        } else {
            Err(alloc::string::String::from("Filesystem not mounted"))
        };

        // Output results
        match result {
            Ok(size) => {
                self.write_output(&alloc::format!(
                    "Successfully saved {} bytes to '{}'\r\n",
                    size, final_filename
                ));
            }
            Err(e) => {
                self.write_output(&alloc::format!("{}\r\n", e));
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
