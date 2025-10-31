// Simple interactive shell for file operations

use crate::kernel::filesystem::SimpleFilesystem;
use crate::kernel::uart_write_string;
use crate::kernel::console;
extern crate alloc;

const MAX_COMMAND_LEN: usize = 128;

// Helper function to write to both UART and GUI console
fn write_output(s: &str) {
    uart_write_string(s); // Keep UART for debugging
    console::write_string(s); // Display in GUI
}

pub struct Shell {
    command_buffer: [u8; MAX_COMMAND_LEN],
    cursor_pos: usize,
    pub filesystem: Option<SimpleFilesystem>,
    pub device_index: Option<usize>,
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
        write_output("> ");
    }

    pub fn handle_char(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => {
                // Execute command
                write_output("\r\n");
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
                    write_output("\x08 \x08"); // Backspace, space, backspace
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
                    console::write_char(ch);
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
            "edit" => self.cmd_edit(&parts),
            "clear" => self.cmd_clear(),
            _ => {
                write_output("Unknown command: ");
                write_output(parts[0]);
                write_output("\r\nType 'help' for available commands\r\n");
            }
        }
    }

    fn cmd_help(&self) {
        write_output("Available commands:\r\n");
        write_output("  ls                    - List files\r\n");
        write_output("  cat <filename>        - Show file contents\r\n");
        write_output("  create <name> <size>  - Create a file\r\n");
        write_output("  rm <filename>         - Delete a file\r\n");
        write_output("  write <file> <text>   - Write text to file\r\n");
        write_output("  edit <filename>       - Open file in editor\r\n");
        write_output("  clear                 - Clear screen\r\n");
        write_output("  help                  - Show this help\r\n");
    }

    fn cmd_ls(&mut self) {
        if let Some(ref fs) = self.filesystem {
            let files = fs.list_files();
            if files.is_empty() {
                write_output("No files\r\n");
            } else {
                write_output(&alloc::format!("{} file(s):\r\n", files.len()));
                for file in files {
                    write_output(&alloc::format!(
                        "  {} - {} bytes\r\n",
                        file.get_name(),
                        file.get_size_bytes()
                    ));
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_cat(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            write_output("Usage: cat <filename>\r\n");
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
                                        write_output(text);
                                        write_output("\r\n");
                                    } else {
                                        write_output("(binary file)\r\n");
                                    }
                                }
                                Err(e) => write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            write_output("File not found\r\n");
                        }
                    } else {
                        write_output("Block device not available\r\n");
                    }
                } else {
                    write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_create(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            write_output("Usage: create <filename> <size>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        if let Ok(size) = parts[2].parse::<u32>() {
                            match fs.create_file(device, filename, size) {
                                Ok(()) => write_output(&alloc::format!(
                                    "Created '{}' ({} bytes)\r\n", filename, size
                                )),
                                Err(e) => write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            write_output("Invalid size\r\n");
                        }
                    } else {
                        write_output("Block device not available\r\n");
                    }
                } else {
                    write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_rm(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            write_output("Usage: rm <filename>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];

                        match fs.delete_file(device, filename) {
                            Ok(()) => write_output(&alloc::format!("Deleted '{}'\r\n", filename)),
                            Err(e) => write_output(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        write_output("Block device not available\r\n");
                    }
                } else {
                    write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_write(&mut self, parts: &[&str]) {
        if parts.len() < 3 {
            write_output("Usage: write <filename> <text...>\r\n");
            return;
        }

        if let (Some(ref mut fs), Some(idx)) = (&mut self.filesystem, self.device_index) {
            unsafe {
                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                    if let Some(device) = devices.get_mut(idx) {
                        let filename = parts[1];
                        let text = parts[2..].join(" ");

                        match fs.write_file(device, filename, text.as_bytes()) {
                            Ok(()) => write_output(&alloc::format!(
                                "Wrote {} bytes to '{}'\r\n", text.len(), filename
                            )),
                            Err(e) => write_output(&alloc::format!("Error: {}\r\n", e)),
                        }
                    } else {
                        write_output("Block device not available\r\n");
                    }
                } else {
                    write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_edit(&mut self, parts: &[&str]) {
        if parts.len() < 2 {
            write_output("Usage: edit <filename>\r\n");
            return;
        }

        let filename = parts[1];

        // Check if the editor window already exists
        if crate::kernel::window_manager::has_focused_editor() {
            write_output("Editor window is already open\r\n");
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
                                        // Create editor with file content
                                        let editor = crate::kernel::editor::TextEditor::with_content(
                                            filename,
                                            text
                                        );
                                        crate::kernel::editor::set_editor(editor);

                                        // Open editor window
                                        let window = crate::kernel::window_manager::Window::new(
                                            0, 0, 640, 480,
                                            &alloc::format!("Text Editor - {}", filename),
                                            crate::kernel::window_manager::WindowContent::Editor
                                        );
                                        crate::kernel::window_manager::add_window(window);

                                        write_output(&alloc::format!("Opened '{}' in editor\r\n", filename));
                                    } else {
                                        write_output("Cannot edit binary file\r\n");
                                    }
                                }
                                Err(e) => write_output(&alloc::format!("Error: {}\r\n", e)),
                            }
                        } else {
                            write_output("File not found\r\n");
                        }
                    } else {
                        write_output("Block device not available\r\n");
                    }
                } else {
                    write_output("Block devices not initialized\r\n");
                }
            }
        } else {
            write_output("Filesystem not mounted\r\n");
        }
    }

    fn cmd_clear(&self) {
        // Clear UART terminal with ANSI escape sequence
        uart_write_string("\x1b[2J\x1b[H");
        // Clear GUI console
        console::clear();
    }
}
