#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

// File server PID (well-known - file server should be spawned as PID 2)
const FILE_SERVER_PID: u32 = 2;

// Bump allocator for userspace
const HEAP_SIZE: usize = 256 * 1024; // 256KB heap

struct BumpAllocator {
    heap: UnsafeCell<[u8; HEAP_SIZE]>,
    next: UnsafeCell<usize>,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap: UnsafeCell::new([0; HEAP_SIZE]),
            next: UnsafeCell::new(0),
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        let next = *self.next.get();
        let aligned = (next + align - 1) & !(align - 1);
        let new_next = aligned + size;

        if new_next > HEAP_SIZE {
            return core::ptr::null_mut();
        }

        *self.next.get() = new_next;
        self.heap.get().cast::<u8>().add(aligned)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

// Console constants
const CONSOLE_WIDTH: usize = 64;  // 64 characters wide
const CONSOLE_HEIGHT: usize = 38; // 38 lines tall
const CHAR_WIDTH: u32 = 16;       // 16 pixels per char (8x8 font scaled 2x)
const CHAR_HEIGHT: u32 = 16;      // 16 pixels per char
const LINE_SPACING: u32 = 4;      // Extra pixels between lines
const LINE_HEIGHT: u32 = CHAR_HEIGHT + LINE_SPACING; // 20 pixels per line

// Console colors
const FG_COLOR: u32 = 0xFFFFFFFF; // White text
const BG_COLOR: u32 = 0xFF000000; // Black background
const CURSOR_COLOR: u32 = 0xFF00FF00; // Green cursor

// Command buffer size
const MAX_COMMAND_LEN: usize = 128;

// Pending write buffer size
const MAX_WRITE_DATA: usize = 200;

/// Console state (local to Terminal process, NOT shared memory)
/// Synchronization happens at the pixel_buffer level via sync_and_notify()
struct Console {
    buffer: [[u8; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
    cursor_x: usize,
    cursor_y: usize,
}

impl Console {
    const fn new() -> Self {
        Self {
            buffer: [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
            cursor_x: 0,
            cursor_y: 0,
        }
    }

    /// Write a single character to the console
    fn write_char(&mut self, ch: u8) {
        match ch {
            b'\n' => {
                self.cursor_x = 0;
                self.cursor_y += 1;
                if self.cursor_y >= CONSOLE_HEIGHT {
                    self.scroll_up();
                    self.cursor_y = CONSOLE_HEIGHT - 1;
                }
            }
            b'\r' => {
                self.cursor_x = 0;
            }
            8 | 127 => {
                // Backspace
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.buffer[self.cursor_y][self.cursor_x] = b' ';
                }
            }
            _ => {
                // Regular character
                if self.cursor_x >= CONSOLE_WIDTH {
                    self.cursor_x = 0;
                    self.cursor_y += 1;
                    if self.cursor_y >= CONSOLE_HEIGHT {
                        self.scroll_up();
                        self.cursor_y = CONSOLE_HEIGHT - 1;
                    }
                }

                self.buffer[self.cursor_y][self.cursor_x] = ch;
                self.cursor_x += 1;
            }
        }
    }

    /// Write a string to the console
    fn write_string(&mut self, s: &str) {
        for ch in s.bytes() {
            self.write_char(ch);
        }
    }

    /// Scroll all lines up by one
    fn scroll_up(&mut self) {
        for y in 1..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                self.buffer[y - 1][x] = self.buffer[y][x];
            }
        }
        // Clear last line
        for x in 0..CONSOLE_WIDTH {
            self.buffer[CONSOLE_HEIGHT - 1][x] = b' ';
        }
    }

    /// Clear the console
    fn clear(&mut self) {
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                self.buffer[y][x] = b' ';
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Render console to a pixel buffer (ARGB format)
    /// This writes to shared memory (pixel_buffer), so caller must call sync_and_notify() after
    fn render_to_buffer(&self, pixel_buffer: &mut [u32], buffer_width: usize, buffer_height: usize, draw_backgrounds: bool) {
        // Draw console text
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                let ch = self.buffer[y][x];

                let cell_x = (x as i32) * CHAR_WIDTH as i32;
                let cell_y = (y as i32) * LINE_HEIGHT as i32;

                // Draw background for this character cell
                if draw_backgrounds {
                    for dy in 0..CHAR_HEIGHT {
                        for dx in 0..CHAR_WIDTH {
                            let px = cell_x + dx as i32;
                            let py = cell_y + dy as i32;
                            if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                                let idx = (py as usize * buffer_width) + px as usize;
                                if idx < pixel_buffer.len() {
                                    pixel_buffer[idx] = BG_COLOR;
                                }
                            }
                        }
                    }
                }

                // Draw character on top of background (if not space)
                if ch != b' ' {
                    librost::graphics::draw_char(
                        pixel_buffer,
                        buffer_width,
                        buffer_height,
                        cell_x,
                        cell_y,
                        ch,
                        FG_COLOR
                    );
                }
            }
        }

        // Draw cursor
        if self.cursor_x < CONSOLE_WIDTH && self.cursor_y < CONSOLE_HEIGHT {
            let cursor_x = (self.cursor_x as i32) * CHAR_WIDTH as i32;
            let cursor_y = (self.cursor_y as i32) * LINE_HEIGHT as i32;

            for dy in 0..CHAR_HEIGHT {
                for dx in 0..CHAR_WIDTH {
                    let px = cursor_x + dx as i32;
                    let py = cursor_y + dy as i32;
                    if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                        let idx = (py as usize * buffer_width) + px as usize;
                        if idx < pixel_buffer.len() {
                            pixel_buffer[idx] = CURSOR_COLOR;
                        }
                    }
                }
            }
        }
    }
}

/// Shared memory window buffer from WM
struct WindowBuffer {
    pixels: &'static mut [u32],
    width: usize,
    height: usize,
}

/// Pending write operation state
struct PendingWrite {
    data: [u8; MAX_WRITE_DATA],
    len: usize,
    fd: Option<u32>,
}

/// Main terminal application state
struct TerminalApp {
    console: Console,
    command_buffer: [u8; MAX_COMMAND_LEN],
    command_pos: usize,
    window: Option<WindowBuffer>,
    pending_write: Option<PendingWrite>,
    request_counter: AtomicU32,
}

impl TerminalApp {
    fn new() -> Self {
        Self {
            console: Console::new(),
            command_buffer: [0; MAX_COMMAND_LEN],
            command_pos: 0,
            window: None,
            pending_write: None,
            request_counter: AtomicU32::new(1),
        }
    }

    fn next_request_id(&self) -> u32 {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Show command prompt
    fn show_prompt(&mut self) {
        self.console.write_string("> ");
    }

    /// Handle keyboard input
    fn handle_keyboard_input(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => {
                self.console.write_string("\n");
                self.execute_command();
                self.command_pos = 0;
                self.command_buffer.fill(0);
                self.show_prompt();
            }
            8 | 127 => {
                // Backspace
                if self.command_pos > 0 {
                    self.command_pos -= 1;
                    self.command_buffer[self.command_pos] = 0;
                    self.console.write_char(8);
                }
            }
            _ => {
                // Regular character
                if self.command_pos < MAX_COMMAND_LEN - 1 {
                    self.command_buffer[self.command_pos] = ch;
                    self.command_pos += 1;
                    self.console.write_char(ch);
                }
            }
        }

        self.render_and_notify();
    }

    /// Render console to buffer and notify WM
    fn render_and_notify(&mut self) {
        if let Some(ref mut window) = self.window {
            self.console.render_to_buffer(
                window.pixels,
                window.width,
                window.height,
                true
            );

            let wm_pid = 1;
            let my_pid = getpid() as usize;
            let redraw_msg = KernelToWM::RequestRedraw(RequestRedrawMsg {
                msg_type: msg_types::KERNEL_REQUEST_REDRAW,
                _pad1: [0; 7],
                id: my_pid,
            });
            librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
        }
    }

    /// Clear screen and re-render
    fn clear_screen(&mut self) {
        self.console.clear();
        self.show_prompt();

        if let Some(ref mut window) = self.window {
            // Clear entire screen to black
            for i in 0..window.pixels.len() {
                window.pixels[i] = BG_COLOR;
            }

            // Render cleared console
            self.console.render_to_buffer(window.pixels, window.width, window.height, false);

            let wm_pid = 1;
            let my_pid = getpid() as usize;
            let redraw_msg = KernelToWM::RequestRedraw(RequestRedrawMsg {
                msg_type: msg_types::KERNEL_REQUEST_REDRAW,
                _pad1: [0; 7],
                id: my_pid,
            });
            librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
        }
    }

    /// Execute the current command
    fn execute_command(&mut self) {
        if self.command_pos == 0 || self.command_pos > MAX_COMMAND_LEN {
            return;
        }

        // Copy command buffer to avoid borrow conflicts
        let mut temp_buffer = [0u8; MAX_COMMAND_LEN];
        let cmd_len = self.command_pos;
        temp_buffer[..cmd_len].copy_from_slice(&self.command_buffer[..cmd_len]);

        // Parse command WITHOUT allocating
        let cmd_str = core::str::from_utf8(&temp_buffer[..cmd_len])
            .unwrap_or("")
            .trim();

        if cmd_str.is_empty() {
            return;
        }

        // Split by whitespace using fixed-size array
        const MAX_ARGS: usize = 8;
        let mut parts: [&str; MAX_ARGS] = [""; MAX_ARGS];
        let mut part_count = 0;

        for word in cmd_str.split_whitespace() {
            if part_count < MAX_ARGS {
                parts[part_count] = word;
                part_count += 1;
            }
        }

        if part_count == 0 {
            return;
        }

        // Execute command
        match parts[0] {
            "help" => self.cmd_help(),
            "clear" => self.clear_screen(),
            "ls" => self.cmd_ls(),
            "create" => self.cmd_create(&parts, part_count),
            "write" => self.cmd_write(&parts, part_count),
            _ => {
                self.console.write_string("Unknown command: ");
                self.console.write_string(parts[0]);
                self.console.write_string("\nType 'help' for available commands\n");
            }
        }
    }

    /// Help command
    fn cmd_help(&mut self) {
        self.console.write_string("Available commands:\n");
        self.console.write_string("  help                    - Show this help\n");
        self.console.write_string("  clear                   - Clear screen\n");
        self.console.write_string("  ls                      - List files\n");
        self.console.write_string("  create <name> <size>    - Create file (size in bytes)\n");
        self.console.write_string("  write <name> <text>     - Write text to file\n");
    }

    /// List files command
    fn cmd_ls(&mut self) {
        let request_id = self.next_request_id();

        let request = AppToFS::List(FSListMsg {
            msg_type: msg_types::FS_LIST,
            _pad1: [0; 3],
            request_id,
        });

        let request_bytes = request.to_bytes();
        let result = send_message(FILE_SERVER_PID, &request_bytes);

        if result < 0 {
            self.console.write_string("Error: Failed to send request to file server\n");
        }
    }

    /// Create file command
    fn cmd_create(&mut self, parts: &[&str], part_count: usize) {
        if part_count < 3 {
            self.console.write_string("Usage: create <name> <size>\n");
            return;
        }

        let filename = parts[1];
        let size_str = parts[2];

        // Parse size
        let mut size: u32 = 0;
        for ch in size_str.chars() {
            if ch >= '0' && ch <= '9' {
                size = size * 10 + (ch as u32 - '0' as u32);
            } else {
                self.console.write_string("Error: Invalid size (must be a number)\n");
                return;
            }
        }

        if filename.len() > 8 {
            self.console.write_string("Error: Filename too long (max 8 characters)\n");
            return;
        }

        let request_id = self.next_request_id();

        let mut filename_buf = [0u8; 128];
        let filename_bytes = filename.as_bytes();
        let len = core::cmp::min(filename_bytes.len(), 128);
        filename_buf[..len].copy_from_slice(&filename_bytes[..len]);

        let request = AppToFS::Create(FSCreateMsg {
            msg_type: msg_types::FS_CREATE,
            _pad1: [0; 3],
            request_id,
            filename_len: len,
            size,
            _pad2: [0; 4],
            filename: filename_buf,
        });

        let request_bytes = request.to_bytes();
        let result = send_message(FILE_SERVER_PID, &request_bytes);

        if result < 0 {
            self.console.write_string("Error: Failed to send request to file server\n");
        }
    }

    /// Write file command
    fn cmd_write(&mut self, parts: &[&str], part_count: usize) {
        if part_count < 3 {
            self.console.write_string("Usage: write <name> <text>\n");
            return;
        }

        let filename = parts[1];

        // Join remaining parts as text
        let mut text_buf = [0u8; MAX_WRITE_DATA];
        let mut pos = 0;

        for i in 2..part_count {
            if i > 2 {
                if pos < text_buf.len() {
                    text_buf[pos] = b' ';
                    pos += 1;
                }
            }

            let word_bytes = parts[i].as_bytes();
            let len = core::cmp::min(word_bytes.len(), text_buf.len() - pos);
            text_buf[pos..pos + len].copy_from_slice(&word_bytes[..len]);
            pos += len;
        }

        if filename.len() > 8 {
            self.console.write_string("Error: Filename too long (max 8 characters)\n");
            return;
        }

        // Store pending write
        self.pending_write = Some(PendingWrite {
            data: text_buf,
            len: pos,
            fd: None,
        });

        // Open the file
        let request_id = self.next_request_id();

        let mut filename_buf = [0u8; 128];
        let filename_bytes = filename.as_bytes();
        let name_len = core::cmp::min(filename_bytes.len(), 128);
        filename_buf[..name_len].copy_from_slice(&filename_bytes[..name_len]);

        let open_request = AppToFS::Open(FSOpenMsg {
            msg_type: msg_types::FS_OPEN,
            _pad1: [0; 3],
            request_id,
            filename_len: name_len,
            flags: 1, // Write flag
            _pad2: [0; 4],
            filename: filename_buf,
        });

        let request_bytes = open_request.to_bytes();
        let result = send_message(FILE_SERVER_PID, &request_bytes);

        if result < 0 {
            self.console.write_string("Error: Failed to send request to file server\n");
            self.pending_write = None;
        }
    }

    /// Handle WindowCreated message from WM
    fn handle_window_created(&mut self, window_id: usize, shm_id: i32, width: u32, height: u32) {
        let my_pid = getpid() as usize;
        if window_id != my_pid {
            return;
        }

        print_debug("Terminal: Received WindowCreated\r\n");

        // Map the WM's shared memory buffer
        let shmem_ptr = shm_map(shm_id);
        if shmem_ptr.is_null() {
            print_debug("Terminal: Failed to map WM's shared memory\r\n");
            exit(1);
        }

        // Sanity check dimensions
        let max_pixels: u32 = 4 * 1024 * 1024;
        if width == 0 || height == 0 || width > 4096 || height > 4096 || (width * height) > max_pixels {
            print_debug("Terminal: ERROR - Invalid buffer dimensions!\r\n");
            exit(1);
        }

        // Create slice from mapped buffer
        let pixels = unsafe {
            core::slice::from_raw_parts_mut(
                shmem_ptr as *mut u32,
                (width * height) as usize
            )
        };

        self.window = Some(WindowBuffer {
            pixels,
            width: width as usize,
            height: height as usize,
        });

        print_debug("Terminal: Buffer mapped, rendering...\r\n");

        // Render console to buffer
        self.render_and_notify();

        print_debug("Terminal: Initial render complete\r\n");
    }

    /// Handle file server responses
    fn handle_fs_response(&mut self, msg: FSToApp) {
        match msg {
            FSToApp::ListResponse(msg) => {
                if msg.files_len == 0 {
                    self.console.write_string("(no files)\n");
                } else {
                    let files_str = core::str::from_utf8(&msg.files[..msg.files_len]).unwrap_or("(invalid)");
                    self.console.write_string(files_str);
                    self.console.write_string("\n");
                }
                self.show_prompt();
                self.render_and_notify();
            }
            FSToApp::CreateSuccess(_) => {
                self.console.write_string("File created successfully\n");
                self.show_prompt();
                self.render_and_notify();
            }
            FSToApp::OpenSuccess(msg) => {
                let fd = msg.fd;
                // Get request_id before borrowing pending_write
                let request_id = self.next_request_id();

                // Check if this is for a pending write
                if let Some(ref mut pending) = self.pending_write {
                    pending.fd = Some(fd);

                    let write_request = AppToFS::Write(FSWriteMsg {
                        msg_type: msg_types::FS_WRITE,
                        _pad1: [0; 3],
                        request_id,
                        fd,
                        _pad2: [0; 4],
                        data_len: pending.len,
                        data: pending.data,
                    });

                    send_message(FILE_SERVER_PID, &write_request.to_bytes());
                }
            }
            FSToApp::WriteSuccess(msg) => {
                let bytes_written = msg.bytes_written;
                // Get request_id before borrowing pending_write
                let request_id = self.next_request_id();

                // Close the file
                if let Some(ref pending) = self.pending_write {
                    if let Some(fd) = pending.fd {
                        let close_request = AppToFS::Close(FSCloseMsg {
                            msg_type: msg_types::FS_CLOSE,
                            _pad1: [0; 3],
                            request_id,
                            fd,
                        });

                        send_message(FILE_SERVER_PID, &close_request.to_bytes());
                    }
                }

                self.pending_write = None;

                self.console.write_string("Wrote ");
                self.write_number(bytes_written as u32);
                self.console.write_string(" bytes\n");
                self.show_prompt();
                self.render_and_notify();
            }
            FSToApp::Error(msg) => {
                self.console.write_string("Error: File server returned error code ");
                if msg.error_code < 0 && msg.error_code > -100 {
                    self.console.write_char(b'-');
                    let code = (-msg.error_code) as u8;
                    if code >= 10 {
                        self.console.write_char(b'0' + code / 10);
                    }
                    self.console.write_char(b'0' + code % 10);
                }
                self.console.write_string("\n");
                self.show_prompt();
                self.render_and_notify();
            }
            _ => {
                // Ignore other messages
            }
        }
    }

    /// Helper to write a number to the console
    fn write_number(&mut self, mut num: u32) {
        if num >= 100 {
            self.console.write_char(b'0' + (num / 100) as u8);
            num %= 100;
        }
        if num >= 10 {
            self.console.write_char(b'0' + (num / 10) as u8);
            num %= 10;
        }
        self.console.write_char(b'0' + num as u8);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Terminal (EL0) ===\r\n");
    print_debug("Initializing...\r\n");

    // Create terminal app (owns all state)
    let mut app = TerminalApp::new();

    app.console.write_string("rOSt Terminal v0.1\n");
    app.console.write_string("Type commands or text here\n");
    app.console.write_string("\n> ");

    // Send CreateWindow IPC to WM
    let wm_pid = 1;
    let mut title = [0u8; 64];
    let title_str = b"Terminal";
    title[..title_str.len()].copy_from_slice(title_str);

    let my_pid = getpid() as usize;
    print_debug("[TERM] My PID: ");
    if my_pid < 10 {
        let pid_str = [b'0' + my_pid as u8];
        print_debug(core::str::from_utf8(&pid_str).unwrap());
    }
    print_debug("\r\n");

    print_debug("[TERM] Creating CreateWindow message...\r\n");
    let create_window_msg = KernelToWM::CreateWindow(CreateWindowMsg {
        msg_type: msg_types::KERNEL_CREATE_WINDOW,
        _pad1: [0; 7],
        id: my_pid,
        x: 0,
        y: 0,
        width: 1920,
        height: 1048,
        title,
        title_len: title_str.len(),
    });

    print_debug("[TERM] Serializing message...\r\n");
    let msg_bytes = create_window_msg.to_bytes();

    print_debug("[TERM] Sending to WM (PID 1)...\r\n");
    let result = send_message(wm_pid, &msg_bytes);

    print_debug("[TERM] send_message result: ");
    if result < 0 {
        print_debug("FAILED!\r\n");
        exit(1);
    } else {
        print_debug("SUCCESS\r\n");
    }

    print_debug("[TERM] Waiting for WindowCreated response...\r\n");

    // Main event loop
    print_debug("Terminal: Entering event loop\r\n");
    let mut msg_buf = [0u8; 256];
    loop {
        let result = recv_message(&mut msg_buf, 1000);
        if result > 0 {
            // Try to parse as WM message first
            if let Some(msg) = WMToKernel::from_bytes(&msg_buf) {
                match msg {
                    WMToKernel::WindowCreated(msg) => {
                        app.handle_window_created(msg.window_id, msg.shm_id, msg.width, msg.height);
                    }
                    WMToKernel::RouteInput(msg) => {
                        if msg.event.event_type == 1 { // KeyPressed
                            if let Some(ascii) = librost::input::evdev_to_ascii(msg.event.key, msg.event.modifiers) {
                                app.handle_keyboard_input(ascii);
                            }
                        }
                    }
                    _ => {}
                }
            } else if let Some(fs_msg) = FSToApp::from_bytes(&msg_buf) {
                app.handle_fs_response(fs_msg);
            }
        }

        yield_now();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in terminal: ");
    if let Some(location) = info.location() {
        print_debug(location.file());
        print_debug(":");
        let line = location.line();
        if line >= 100 {
            let hundreds = (line / 100) as u8;
            let c = [b'0' + hundreds];
            print_debug(core::str::from_utf8(&c).unwrap_or("?"));
        }
        if line >= 10 {
            let tens = ((line / 10) % 10) as u8;
            let c = [b'0' + tens];
            print_debug(core::str::from_utf8(&c).unwrap_or("?"));
        }
        let ones = (line % 10) as u8;
        let c = [b'0' + ones];
        print_debug(core::str::from_utf8(&c).unwrap_or("?"));
    }
    if let Some(msg) = info.payload().downcast_ref::<&str>() {
        print_debug(" - ");
        print_debug(msg);
    }
    print_debug("\r\n");
    exit(1);
}
