// Simple interactive shell for file operations

use crate::kernel::filesystem::SimpleFilesystem;
use crate::kernel::virtio_blk::VirtioBlkDevice;
use crate::kernel::uart_write_string;
extern crate alloc;
use alloc::string::String;

const MAX_COMMAND_LEN: usize = 128;

pub struct Shell {
    command_buffer: [u8; MAX_COMMAND_LEN],
    cursor_pos: usize,
    filesystem: Option<SimpleFilesystem>,
    device_index: Option<usize>,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            command_buffer: [0; MAX_COMMAND_LEN],
            cursor_pos: 0,
            filesystem: None,
            device_index: None,
        }
    }

    pub fn set_filesystem(&mut self, fs: SimpleFilesystem, device_idx: usize) {
        self.filesystem = Some(fs);
        self.device_index = Some(device_idx);
    }

    pub fn show_prompt(&self) {
        uart_write_string("> ");
    }

    pub fn handle_char(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => {
                // Execute command
                uart_write_string("\r\n");
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
                    uart_write_string("\x08 \x08"); // Backspace, space, backspace
                }
            }
            _ => {
                // Regular character
                if self.cursor_pos < MAX_COMMAND_LEN - 1 {
                    self.command_buffer[self.cursor_pos] = ch;
                    self.cursor_pos += 1;
                    // Echo the character
                    unsafe {
                        core::ptr::write_volatile(0x09000000 as *mut u8, ch);
                    }
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
            "write" => self.cmd_write(&parts),
            "clear" => self.cmd_clear(),
            _ => {
                uart_write_string("Unknown command: ");
                uart_write_string(parts[0]);
                uart_write_string("\r\nType 'help' for available commands\r\n");
            }
        }
    }

    fn cmd_help(&self) {
        uart_write_string("Available commands:\r\n");
        uart_write_string("  ls                    - List files\r\n");
        uart_write_string("  cat <filename>        - Show file contents\r\n");
        uart_write_string("  create <name> <size>  - Create a file\r\n");
        uart_write_string("  rm <filename>         - Delete a file\r\n");
        uart_write_string("  write <file> <text>   - Write text to file\r\n");
        uart_write_string("  clear                 - Clear screen\r\n");
        uart_write_string("  help                  - Show this help\r\n");
    }

    fn cmd_ls(&mut self) {
        if let Some(ref fs) = self.filesystem {
            let files = fs.list_files();
            if files.is_empty() {
                uart_write_string("No files\r\n");
            } else {
                uart_write_string(&alloc::format!("{} file(s):\r\n", files.len()));
                for file in files {
                    uart_write_string(&alloc::format!(
                        "  {} - {} bytes\r\n",
                        file.get_name(),
                        file.get_size_bytes()
                    ));
                }
            }
        } else {
            uart_write_string("Filesystem not mounted\r\n");
        }
    }

    fn cmd_cat(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            uart_write_string("Usage: cat <filename>\r\n");
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
                                        uart_write_string(text);
                                        uart_write_string("\r\n");
                                    } else {
                                        uart_write_string("(binary file)\r\n");
                                    }
                                }
                                Err(e) => uart_write_string(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            uart_write_string("File not found\r\n");
                        }
                    } else {
                        uart_write_string("Block device not available\r\n");
                    }
                } else {
                    uart_write_string("Block devices not initialized\r\n");
                }
            }
        } else {
            uart_write_string("Filesystem not mounted\r\n");
        }
    }

    fn cmd_create(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            uart_write_string("Usage: create <filename> <size>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        if let Ok(size) = parts[2].parse::<u32>() {
                            match fs.create_file(device, filename, size) {
                                Ok(()) => uart_write_string(&alloc::format!(
                                    "Created '{}' ({} bytes)\r\n", filename, size
                                )),
                                Err(e) => uart_write_string(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            uart_write_string("Invalid size\r\n");
                        }
                    } else {
                        uart_write_string("Block device not available\r\n");
                    }
                } else {
                    uart_write_string("Block devices not initialized\r\n");
                }
            }
        } else {
            uart_write_string("Filesystem not mounted\r\n");
        }
    }

    fn cmd_rm(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            uart_write_string("Usage: rm <filename>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        match fs.delete_file(device, filename) {
                            Ok(()) => uart_write_string(&alloc::format!("Deleted '{}'\r\n", filename)),
                            Err(e) => uart_write_string(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        uart_write_string("Block device not available\r\n");
                    }
                } else {
                    uart_write_string("Block devices not initialized\r\n");
                }
            }
        } else {
            uart_write_string("Filesystem not mounted\r\n");
        }
    }

    fn cmd_write(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            uart_write_string("Usage: write <filename> <text...>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];
                        let text = parts[2..].join(" ");

                        match fs.write_file(device, filename, text.as_bytes()) {
                            Ok(()) => uart_write_string(&alloc::format!(
                                "Wrote {} bytes to '{}'\r\n", text.len(), filename
                            )),
                            Err(e) => uart_write_string(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        uart_write_string("Block device not available\r\n");
                    }
                } else {
                    uart_write_string("Block devices not initialized\r\n");
                }
            }
        } else {
            uart_write_string("Filesystem not mounted\r\n");
        }
    }

    fn cmd_clear(&self) {
        // ANSI escape sequence to clear screen
        uart_write_string("\x1b[2J\x1b[H");
    }
}
