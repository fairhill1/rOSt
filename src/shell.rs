use crate::uart::Uart;
use alloc::vec::Vec;

const COMMAND_BUFFER_SIZE: usize = 256;

pub struct Shell {
    command_buffer: [u8; COMMAND_BUFFER_SIZE],
    buffer_pos: usize,
    uart: Uart,
}

impl Shell {
    pub fn new(uart: Uart) -> Self {
        Shell {
            command_buffer: [0; COMMAND_BUFFER_SIZE],
            buffer_pos: 0,
            uart,
        }
    }
    
    pub fn run(&mut self) -> ! {
        self.uart.puts("\n\nRust OS Shell v0.1\n");
        self.uart.puts("Type 'help' for available commands\n\n");
        self.print_prompt();
        
        loop {
            // Check for input from the buffer
            unsafe {
                while let Some(byte) = crate::input::INPUT_BUFFER.pop() {
                    self.handle_char(byte);
                }
            }
            
            // Let CPU rest
            unsafe { core::arch::asm!("wfi") };
        }
    }
    
    fn print_prompt(&self) {
        let path = crate::filesystem::pwd();
        self.uart.puts("rust-os:");
        self.uart.puts(&path);
        self.uart.puts("$ ");
    }
    
    fn handle_char(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.uart.puts("\n");
                self.execute_command();
                self.command_buffer = [0; COMMAND_BUFFER_SIZE];
                self.buffer_pos = 0;
                self.print_prompt();
            }
            0x7f | 0x08 => { // Backspace
                if self.buffer_pos > 0 {
                    self.buffer_pos -= 1;
                    self.command_buffer[self.buffer_pos] = 0;
                }
            }
            _ => {
                if self.buffer_pos < COMMAND_BUFFER_SIZE - 1 {
                    self.command_buffer[self.buffer_pos] = byte;
                    self.buffer_pos += 1;
                }
            }
        }
    }
    
    fn execute_command(&mut self) {
        let command = core::str::from_utf8(&self.command_buffer[..self.buffer_pos])
            .unwrap_or("");
        
        let command = command.trim();
        if command.is_empty() {
            return;
        }
        
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        
        match parts[0] {
            "help" => {
                self.uart.puts("Available commands:\n");
                self.uart.puts("System:\n");
                self.uart.puts("  help     - Show this help message\n");
                self.uart.puts("  hello    - Print a greeting\n");
                self.uart.puts("  uptime   - Show system uptime\n");
                self.uart.puts("  echo     - Echo arguments back\n");
                self.uart.puts("  clear    - Clear the screen\n");
                self.uart.puts("  meminfo  - Show memory information\n");
                self.uart.puts("  translate - Translate virtual address\n");
                self.uart.puts("  reboot   - Reboot the system\n");
                self.uart.puts("Tasks:\n");
                self.uart.puts("  tasks    - List all tasks\n");
                self.uart.puts("  spawn    - Spawn demo tasks\n");
                self.uart.puts("  schedule - Run scheduler once\n");
                self.uart.puts("Filesystem:\n");
                self.uart.puts("  ls       - List directory contents\n");
                self.uart.puts("  cd       - Change directory\n");
                self.uart.puts("  pwd      - Print working directory\n");
                self.uart.puts("  mkdir    - Create directory\n");
                self.uart.puts("  cat      - Show file contents\n");
                self.uart.puts("  echo     - Create file (echo text > file)\n");
                self.uart.puts("  rm       - Remove file or directory\n");
                self.uart.puts("Graphics:\n");
                self.uart.puts("  gui      - Start GUI mode\n");
                self.uart.puts("  demo     - Graphics demo\n");
            }
            "hello" => {
                self.uart.puts("Hello from Rust OS!\n");
            }
            "uptime" => {
                unsafe {
                    let mut counter: u64;
                    core::arch::asm!("mrs {}, cntpct_el0", out(reg) counter);
                    let mut freq: u64;
                    core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
                    let seconds = counter / freq;
                    self.uart.puts("Uptime: ");
                    self.uart.put_hex(seconds);
                    self.uart.puts(" seconds\n");
                }
            }
            "echo" => {
                for (i, part) in parts.iter().skip(1).enumerate() {
                    if i > 0 {
                        self.uart.putc(b' ');
                    }
                    self.uart.puts(part);
                }
                self.uart.puts("\n");
            }
            "clear" => {
                // ANSI escape sequence to clear screen
                self.uart.puts("\x1b[2J\x1b[H");
            }
            "meminfo" => {
                unsafe {
                    // Read system registers for memory info
                    let mut tcr: u64;
                    let mut ttbr0: u64;
                    let mut sctlr: u64;
                    
                    core::arch::asm!("mrs {}, tcr_el1", out(reg) tcr);
                    core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ttbr0);
                    core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr);
                    
                    self.uart.puts("Memory Management Status:\n");
                    self.uart.puts("  MMU: ");
                    if sctlr & 1 != 0 {
                        self.uart.puts("Enabled\n");
                    } else {
                        self.uart.puts("Disabled\n");
                    }
                    
                    self.uart.puts("  Page Table Base: ");
                    self.uart.put_hex(ttbr0 & !0xFFF);
                    self.uart.puts("\n");
                    
                    self.uart.puts("  TCR_EL1: ");
                    self.uart.put_hex(tcr);
                    self.uart.puts("\n");
                    
                    self.uart.puts("  Caches: ");
                    if sctlr & (1 << 2) != 0 {
                        self.uart.puts("D-cache ");
                    }
                    if sctlr & (1 << 12) != 0 {
                        self.uart.puts("I-cache ");
                    }
                    self.uart.puts("\n");
                }
            }
            "translate" => {
                if parts.len() < 2 {
                    self.uart.puts("Usage: translate <hex_address>\n");
                    return;
                }
                
                // Parse hex address
                let addr_str = parts[1];
                let mut addr: u64 = 0;
                
                for c in addr_str.chars() {
                    addr <<= 4;
                    match c {
                        '0'..='9' => addr |= (c as u64) - ('0' as u64),
                        'a'..='f' => addr |= (c as u64) - ('a' as u64) + 10,
                        'A'..='F' => addr |= (c as u64) - ('A' as u64) + 10,
                        _ => {
                            self.uart.puts("Invalid hex address\n");
                            return;
                        }
                    }
                }
                
                self.uart.puts("Virtual: ");
                self.uart.put_hex(addr);
                
                // Simple translation for 1GB blocks
                if addr < 0x80000000 {
                    let paddr = addr; // Identity mapping
                    self.uart.puts(" -> Physical: ");
                    self.uart.put_hex(paddr);
                    self.uart.puts(" (identity mapped)\n");
                } else {
                    self.uart.puts(" -> Not mapped\n");
                }
            }
            "tasks" => {
                crate::scheduler::list_tasks();
            }
            "spawn" => {
                self.uart.puts("Spawning demo tasks...\n");
                crate::scheduler::add_task("demo_task_1", crate::scheduler::task1, 8192);
                crate::scheduler::add_task("demo_task_2", crate::scheduler::task2, 8192);
                self.uart.puts("Demo tasks spawned. Use 'schedule' to run them.\n");
            }
            "schedule" => {
                self.uart.puts("Running scheduler...\n");
                crate::scheduler::schedule();
                self.uart.puts("Scheduler cycle complete.\n");
            }
            "ls" => {
                match crate::filesystem::ls() {
                    Ok(entries) => {
                        if entries.is_empty() {
                            self.uart.puts("(empty directory)\n");
                        } else {
                            for (name, file_type, size) in entries {
                                match file_type {
                                    crate::filesystem::FileType::Directory => {
                                        self.uart.puts("[DIR]  ");
                                    }
                                    crate::filesystem::FileType::File => {
                                        self.uart.puts("[FILE] ");
                                    }
                                }
                                self.uart.puts(&name);
                                if matches!(file_type, crate::filesystem::FileType::File) {
                                    self.uart.puts(" (");
                                    self.uart.put_hex(size as u64);
                                    self.uart.puts(" bytes)");
                                }
                                self.uart.puts("\n");
                            }
                        }
                    }
                    Err(e) => {
                        self.uart.puts("Error: ");
                        self.uart.puts(e);
                        self.uart.puts("\n");
                    }
                }
            }
            "pwd" => {
                let path = crate::filesystem::pwd();
                self.uart.puts(&path);
                self.uart.puts("\n");
            }
            "cd" => {
                if parts.len() < 2 {
                    self.uart.puts("Usage: cd <directory>\n");
                    return;
                }
                
                match crate::filesystem::cd(parts[1]) {
                    Ok(()) => {
                        // Success, no output needed
                    }
                    Err(e) => {
                        self.uart.puts("cd: ");
                        self.uart.puts(e);
                        self.uart.puts("\n");
                    }
                }
            }
            "mkdir" => {
                if parts.len() < 2 {
                    self.uart.puts("Usage: mkdir <directory>\n");
                    return;
                }
                
                match crate::filesystem::mkdir(parts[1]) {
                    Ok(()) => {
                        self.uart.puts("Directory created\n");
                    }
                    Err(e) => {
                        self.uart.puts("mkdir: ");
                        self.uart.puts(e);
                        self.uart.puts("\n");
                    }
                }
            }
            "cat" => {
                if parts.len() < 2 {
                    self.uart.puts("Usage: cat <filename>\n");
                    return;
                }
                
                match crate::filesystem::read_file(parts[1]) {
                    Ok(data) => {
                        // Convert bytes to string and print
                        if let Ok(text) = core::str::from_utf8(data) {
                            self.uart.puts(text);
                        } else {
                            self.uart.puts("(binary file - ");
                            self.uart.put_hex(data.len() as u64);
                            self.uart.puts(" bytes)\n");
                        }
                    }
                    Err(e) => {
                        self.uart.puts("cat: ");
                        self.uart.puts(e);
                        self.uart.puts("\n");
                    }
                }
            }
            "rm" => {
                if parts.len() < 2 {
                    self.uart.puts("Usage: rm <file_or_directory>\n");
                    return;
                }
                
                match crate::filesystem::remove(parts[1]) {
                    Ok(()) => {
                        self.uart.puts("Removed\n");
                    }
                    Err(e) => {
                        self.uart.puts("rm: ");
                        self.uart.puts(e);
                        self.uart.puts("\n");
                    }
                }
            }
            "demo" => {
                self.uart.puts("Running graphics demo...\n");
                if let Some(fb) = crate::graphics::get_framebuffer_mut() {
                    // Clear screen with gradient
                    for y in 0..fb.height() {
                        for x in 0..fb.width() {
                            let r = (x * 255 / fb.width()) as u8;
                            let g = (y * 255 / fb.height()) as u8;
                            let b = 128;
                            fb.put_pixel(x, y, crate::graphics::Color::new(r, g, b));
                        }
                    }
                    
                    // Draw some shapes
                    fb.fill_rect(100, 100, 200, 100, crate::graphics::Color::RED);
                    fb.draw_circle(300, 150, 50, crate::graphics::Color::YELLOW);
                    fb.draw_circle(500, 150, 30, crate::graphics::Color::GREEN);
                    
                    // Draw text
                    fb.draw_string(50, 50, "Rust OS Graphics Demo!", crate::graphics::Color::WHITE);
                    fb.draw_string(50, 300, "Hello GUI World!", crate::graphics::Color::BLACK);
                    fb.draw_string(50, 320, "Framebuffer Working!", crate::graphics::Color::BLUE);
                    
                    self.uart.puts("Graphics demo complete! Check your display.\n");
                } else {
                    self.uart.puts("Graphics not available\n");
                }
            }
            "gui" => {
                self.uart.puts("Starting GUI mode...\n");
                self.uart.puts("(GUI mode not implemented yet)\n");
                self.uart.puts("Try 'demo' to see graphics!\n");
            }
            "reboot" => {
                self.uart.puts("Rebooting...\n");
                unsafe {
                    // Trigger a system reset
                    core::arch::asm!("wfe");
                }
            }
            _ => {
                self.uart.puts("Unknown command: ");
                self.uart.puts(command);
                self.uart.puts("\nType 'help' for available commands\n");
            }
        }
    }
}