// Simple interactive shell for file operations

use crate::kernel::filesystem::SimpleFilesystem;
use crate::kernel::uart_write_string;
use crate::kernel::console;
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
                                    crate::kernel::file_explorer::refresh_all_explorers();
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
                                crate::kernel::file_explorer::refresh_all_explorers();
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
                                crate::kernel::file_explorer::refresh_all_explorers();
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
                                crate::kernel::file_explorer::refresh_all_explorers();
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
        if crate::kernel::window_manager::has_focused_editor() {
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
                                        let editor_id = crate::kernel::editor::create_editor_with_content(
                                            filename,
                                            text
                                        );

                                        // Open editor window
                                        let window = crate::kernel::window_manager::Window::new(
                                            0, 0, 640, 480,
                                            &alloc::format!("Text Editor - {}", filename),
                                            crate::kernel::window_manager::WindowContent::Editor,
                                            editor_id
                                        );
                                        crate::kernel::window_manager::add_window(window);

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
                    match crate::kernel::filesystem::SimpleFilesystem::mount(device) {
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
