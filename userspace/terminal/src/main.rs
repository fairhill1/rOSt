#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

// File server PID (well-known - file server should be spawned as PID 2)
const FILE_SERVER_PID: u32 = 2;

// Request ID counter for IPC
static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(1);

// Pending write data (for write command - store data until we get file descriptor from open)
static mut PENDING_WRITE_DATA: [u8; 200] = [0; 200];
static PENDING_WRITE_LEN: AtomicUsize = AtomicUsize::new(0);
static PENDING_WRITE_FD: AtomicU32 = AtomicU32::new(0);

// Shared buffer info (from WM's WindowCreated message)
static mut PIXEL_BUFFER: *mut u32 = core::ptr::null_mut();
static BUFFER_WIDTH: AtomicU32 = AtomicU32::new(0);
static BUFFER_HEIGHT: AtomicU32 = AtomicU32::new(0);

// Bump allocator for userspace
const HEAP_SIZE: usize = 256 * 1024; // 256KB heap (larger than WM since we store console buffer + command history)

// Store colors as separate atomics to prevent compiler optimization issues
static FG_COLOR: AtomicU32 = AtomicU32::new(0xFFFFFFFF); // White text
static BG_COLOR: AtomicU32 = AtomicU32::new(0xFF000000); // Black background

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

/// Console state
struct Console {
    buffer: [[u8; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
    cursor_x: AtomicUsize,
    cursor_y: AtomicUsize,
}

impl Console {
    const fn new() -> Self {
        Self {
            buffer: [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
            cursor_x: AtomicUsize::new(0),
            cursor_y: AtomicUsize::new(0),
        }
    }

    /// Write a single character to the console
    fn write_char(&mut self, ch: u8) {
        match ch {
            b'\n' => {
                self.cursor_x.store(0, Ordering::SeqCst);
                let y = self.cursor_y.load(Ordering::SeqCst) + 1;
                self.cursor_y.store(y, Ordering::SeqCst);
                if y >= CONSOLE_HEIGHT {
                    self.scroll_up();
                    self.cursor_y.store(CONSOLE_HEIGHT - 1, Ordering::SeqCst);
                }
            }
            b'\r' => {
                self.cursor_x.store(0, Ordering::SeqCst);
            }
            8 | 127 => {
                // Backspace
                let x = self.cursor_x.load(Ordering::SeqCst);
                if x > 0 {
                    let new_x = x - 1;
                    self.cursor_x.store(new_x, Ordering::SeqCst);
                    let y = self.cursor_y.load(Ordering::SeqCst);
                    unsafe {
                        core::ptr::write_volatile(&mut self.buffer[y][new_x], b' ');
                    }
                }
            }
            _ => {
                // Regular character
                let mut x = self.cursor_x.load(Ordering::SeqCst);
                let y = self.cursor_y.load(Ordering::SeqCst);

                if x >= CONSOLE_WIDTH {
                    x = 0;
                    let new_y = y + 1;
                    if new_y >= CONSOLE_HEIGHT {
                        self.scroll_up();
                        self.cursor_y.store(CONSOLE_HEIGHT - 1, Ordering::SeqCst);
                    } else {
                        self.cursor_y.store(new_y, Ordering::SeqCst);
                    }
                    // Write character on new line
                    unsafe {
                        core::ptr::write_volatile(&mut self.buffer[new_y.min(CONSOLE_HEIGHT - 1)][x], ch);
                    }
                } else {
                    // Normal write - y doesn't change
                    unsafe {
                        core::ptr::write_volatile(&mut self.buffer[y][x], ch);
                    }
                }

                x += 1;
                self.cursor_x.store(x, Ordering::SeqCst);
                // Note: cursor_y only updated in wrap case above
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
            self.buffer[y - 1] = self.buffer[y];
        }
        // Clear last line in-place to avoid stack overflow
        for x in 0..CONSOLE_WIDTH {
            unsafe {
                core::ptr::write_volatile(&mut self.buffer[CONSOLE_HEIGHT - 1][x], b' ');
            }
        }
    }

    /// Clear the console
    fn clear(&mut self) {
        // Clear buffer in-place to avoid stack overflow from temporary array
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                unsafe {
                    core::ptr::write_volatile(&mut self.buffer[y][x], b' ');
                }
            }
        }
        self.cursor_x.store(0, Ordering::SeqCst);
        self.cursor_y.store(0, Ordering::SeqCst);
    }

    /// Render console to a pixel buffer (ARGB format)
    /// buffer_width and buffer_height are in pixels
    fn render_to_buffer(&self, pixel_buffer: &mut [u32], buffer_width: usize, buffer_height: usize) {
        let bg_color = BG_COLOR.load(Ordering::SeqCst);
        let fg_color = FG_COLOR.load(Ordering::SeqCst);

        // Clear to background color
        for pixel in pixel_buffer.iter_mut() {
            *pixel = bg_color;
        }

        // Draw console text using bitmap font
        for y in 0..CONSOLE_HEIGHT {
            let line_y = (y as i32) * LINE_HEIGHT as i32;

            // Draw each character in this line
            for x in 0..CONSOLE_WIDTH {
                let ch = self.buffer[y][x];
                if ch != b' ' {
                    let char_x = (x as i32) * CHAR_WIDTH as i32;
                    librost::graphics::draw_char(
                        pixel_buffer,
                        buffer_width,
                        buffer_height,
                        char_x,
                        line_y,
                        ch,
                        fg_color
                    );
                }
            }
        }

        // Draw cursor (solid block)
        let cursor_x_pos = self.cursor_x.load(Ordering::SeqCst);
        let cursor_y_pos = self.cursor_y.load(Ordering::SeqCst);

        if cursor_x_pos < CONSOLE_WIDTH && cursor_y_pos < CONSOLE_HEIGHT {
            let cursor_x = (cursor_x_pos as i32) * CHAR_WIDTH as i32;
            let cursor_y = (cursor_y_pos as i32) * LINE_HEIGHT as i32;

            // Draw a filled rectangle for the cursor
            // Use volatile writes because pixel_buffer is shared memory (WM reads it)
            for dy in 0..CHAR_HEIGHT {
                for dx in 0..CHAR_WIDTH {
                    let px = cursor_x + dx as i32;
                    let py = cursor_y + dy as i32;
                    if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                        let idx = (py as usize * buffer_width) + px as usize;
                        if idx < pixel_buffer.len() {
                            unsafe {
                                core::ptr::write_volatile(&mut pixel_buffer[idx], 0xFF00FF00); // Green cursor
                            }
                        }
                    }
                }
            }
        }
    }
}

static mut CONSOLE: Console = Console::new();

// Shell command buffer
const MAX_COMMAND_LEN: usize = 128;
static mut COMMAND_BUFFER: [u8; MAX_COMMAND_LEN] = [0; MAX_COMMAND_LEN];
static COMMAND_POS: AtomicUsize = AtomicUsize::new(0);

/// Handle shell input character
unsafe fn handle_shell_input(ch: u8) {
    match ch {
        b'\n' | b'\r' => {
            // Execute command
            CONSOLE.write_string("\n");
            execute_command();
            COMMAND_POS.store(0, Ordering::SeqCst);
            // Clear command buffer by resetting each byte individually (avoid stack overflow)
            for i in 0..MAX_COMMAND_LEN {
                COMMAND_BUFFER[i] = 0;
            }
            show_prompt();
        }
        8 | 127 => {
            // Backspace
            let pos = COMMAND_POS.load(Ordering::SeqCst);
            if pos > 0 {
                COMMAND_POS.store(pos - 1, Ordering::SeqCst);
                COMMAND_BUFFER[pos - 1] = 0;
                CONSOLE.write_char(8); // Backspace in console
            }
        }
        _ => {
            // Regular character
            let pos = COMMAND_POS.load(Ordering::SeqCst);
            if pos < MAX_COMMAND_LEN - 1 {
                COMMAND_BUFFER[pos] = ch;
                COMMAND_POS.store(pos + 1, Ordering::SeqCst);
                CONSOLE.write_char(ch);
            }
        }
    }
}

/// Show command prompt
unsafe fn show_prompt() {
    CONSOLE.write_string("> ");
}

/// Execute the current command
unsafe fn execute_command() {
    let pos = COMMAND_POS.load(Ordering::SeqCst);
    if pos == 0 {
        return;
    }

    // Parse command WITHOUT allocating (bump allocator never frees!)
    let cmd_str = core::str::from_utf8(&COMMAND_BUFFER[..pos])
        .unwrap_or("")
        .trim();

    if cmd_str.is_empty() {
        return;
    }

    // Split by whitespace using fixed-size array instead of Vec
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
        "help" => cmd_help(),
        "clear" => cmd_clear(),
        "ls" => cmd_ls(),
        "create" => cmd_create(&parts, part_count),
        "write" => cmd_write(&parts, part_count),
        _ => {
            CONSOLE.write_string("Unknown command: ");
            CONSOLE.write_string(parts[0]);
            CONSOLE.write_string("\nType 'help' for available commands\n");
        }
    }
}

/// Help command
unsafe fn cmd_help() {
    CONSOLE.write_string("Available commands:\n");
    CONSOLE.write_string("  help                    - Show this help\n");
    CONSOLE.write_string("  clear                   - Clear screen\n");
    CONSOLE.write_string("  ls                      - List files\n");
    CONSOLE.write_string("  create <name> <size>    - Create file (size in bytes)\n");
    CONSOLE.write_string("  write <name> <text>     - Write text to file\n");
}

/// Clear command
unsafe fn cmd_clear() {
    CONSOLE.clear();
}

/// List files command - uses file server IPC
unsafe fn cmd_ls() {
    let my_pid = getpid();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

    // Build List request
    let request = AppToFS::List {
        sender_pid: my_pid,
        request_id,
    };

    // Send request to file server (async - response will arrive in event loop)
    let request_bytes = request.to_bytes();
    let result = send_message(FILE_SERVER_PID, &request_bytes);

    if result < 0 {
        CONSOLE.write_string("Error: Failed to send request to file server\n");
    }
    // Response will be handled in main event loop
}

/// Create file command
unsafe fn cmd_create(parts: &[&str], part_count: usize) {
    if part_count < 3 {
        CONSOLE.write_string("Usage: create <name> <size>\n");
        return;
    }

    let filename = parts[1];
    let size_str = parts[2];

    // Parse size (simple decimal parsing)
    let mut size: u32 = 0;
    for ch in size_str.chars() {
        if ch >= '0' && ch <= '9' {
            size = size * 10 + (ch as u32 - '0' as u32);
        } else {
            CONSOLE.write_string("Error: Invalid size (must be a number)\n");
            return;
        }
    }

    if filename.len() > 8 {
        CONSOLE.write_string("Error: Filename too long (max 8 characters)\n");
        return;
    }

    let my_pid = getpid();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

    // Build filename array
    let mut filename_buf = [0u8; 128];
    let filename_bytes = filename.as_bytes();
    let len = core::cmp::min(filename_bytes.len(), 128);
    filename_buf[..len].copy_from_slice(&filename_bytes[..len]);

    // Build Create request
    let request = AppToFS::Create {
        sender_pid: my_pid,
        request_id,
        filename: filename_buf,
        filename_len: len,
        size,
    };

    // Send request to file server
    let request_bytes = request.to_bytes();
    let result = send_message(FILE_SERVER_PID, &request_bytes);

    if result < 0 {
        CONSOLE.write_string("Error: Failed to send request to file server\n");
    }
    // Response will be handled in main event loop
}

/// Write file command
unsafe fn cmd_write(parts: &[&str], part_count: usize) {
    if part_count < 3 {
        CONSOLE.write_string("Usage: write <name> <text>\n");
        return;
    }

    let filename = parts[1];

    // Join remaining parts as text (everything after filename)
    let mut text_buf = [0u8; 200];
    let mut pos = 0;

    for i in 2..part_count {
        if i > 2 {
            // Add space between words
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
        CONSOLE.write_string("Error: Filename too long (max 8 characters)\n");
        return;
    }

    // First, open the file
    let my_pid = getpid();
    let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

    let mut filename_buf = [0u8; 128];
    let filename_bytes = filename.as_bytes();
    let name_len = core::cmp::min(filename_bytes.len(), 128);
    filename_buf[..name_len].copy_from_slice(&filename_bytes[..name_len]);

    let open_request = AppToFS::Open {
        sender_pid: my_pid,
        request_id,
        filename: filename_buf,
        filename_len: name_len,
        flags: 1, // Write flag
    };

    let request_bytes = open_request.to_bytes();
    let result = send_message(FILE_SERVER_PID, &request_bytes);

    if result < 0 {
        CONSOLE.write_string("Error: Failed to send request to file server\n");
        return;
    }

    // Store write data for when we get the file descriptor
    PENDING_WRITE_DATA[..pos].copy_from_slice(&text_buf[..pos]);
    PENDING_WRITE_LEN.store(pos, Ordering::SeqCst);

    // Response will be handled in main event loop
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Terminal (EL0) ===\r\n");
    print_debug("Initializing...\r\n");

    // Explicitly initialize console state
    unsafe {
        // Clear buffer with volatile writes to ensure it actually happens
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                core::ptr::write_volatile(&mut CONSOLE.buffer[y][x], b' ');
            }
        }

        // Reset cursor to known position
        CONSOLE.cursor_x.store(0, Ordering::SeqCst);
        CONSOLE.cursor_y.store(0, Ordering::SeqCst);

        CONSOLE.write_string("rOSt Terminal v0.1\n");
        CONSOLE.write_string("Type commands or text here\n");
        CONSOLE.write_string("\n> ");
    }

    // Don't create buffer at startup - wait for WM to tell us our dimensions
    // This avoids creating a full-screen buffer that immediately gets resized
    print_debug("About to send CreateWindow to WM\r\n");

    // Send CreateWindow IPC to WM (PID 1) with requested dimensions
    // WM will send us WindowResized with actual assigned dimensions
    let wm_pid = 1;
    let mut title = [0u8; 64];
    let title_str = b"Terminal";
    title[..title_str.len()].copy_from_slice(title_str);

    let my_pid = getpid() as usize;

    // Request full screen size (WM will tile it appropriately)
    let requested_width = 1920u32;
    let requested_height = 1048u32;

    let create_window_msg = KernelToWM::CreateWindow {
        id: my_pid,        // Window ID (globally unique)
        x: 0,
        y: 0,
        width: requested_width,
        height: requested_height,
        title,
        title_len: title_str.len(),
    };

    let msg_bytes = create_window_msg.to_bytes();
    let result = send_message(wm_pid, &msg_bytes);

    if result < 0 {
        print_debug("Failed to send CreateWindow message to WM\r\n");
        exit(1);
    }

    print_debug("CreateWindow message sent to WM\r\n");
    print_debug("Waiting for WindowCreated message from WM\r\n");

    // Wait for WM to send WindowCreated with buffer info
    let mut pixel_buffer: &mut [u32] = &mut [];
    let mut buffer_width: usize = 0;
    let mut buffer_height: usize = 0;

    // Main event loop - wait for WindowCreated, then handle events
    print_debug("Terminal: Entering event loop\r\n");
    let mut msg_buf = [0u8; 256];
    loop {
        let result = recv_message(&mut msg_buf, 1000); // 1 second timeout
        if result > 0 {
            // Try to parse as WM message first
            if let Some(msg) = WMToKernel::from_bytes(&msg_buf) {
                match msg {
                    WMToKernel::WindowCreated { window_id, shm_id, width, height } => {
                        if window_id == my_pid {
                            print_debug("Terminal: Received WindowCreated\r\n");

                            // Map the WM's shared memory buffer
                            print_debug("Terminal: Mapping WM's buffer\r\n");
                            let shmem_ptr = shm_map(shm_id);

                            if shmem_ptr.is_null() {
                                print_debug("Terminal: Failed to map WM's shared memory\r\n");
                                exit(1);
                            }

                            print_debug("Terminal: Buffer mapped successfully\r\n");

                            // Store buffer dimensions
                            buffer_width = width as usize;
                            buffer_height = height as usize;

                            // Create slice from mapped buffer
                            pixel_buffer = unsafe {
                                core::slice::from_raw_parts_mut(
                                    shmem_ptr as *mut u32,
                                    (width * height) as usize
                                )
                            };

                            print_debug("Terminal: Buffer ptr set, rendering...\r\n");

                            // Render console to buffer
                            unsafe {
                                CONSOLE.render_to_buffer(
                                    pixel_buffer,
                                    buffer_width,
                                    buffer_height
                                );
                            }

                            print_debug("Terminal: Initial render complete\r\n");

                            // Request WM to redraw (show our initial content)
                            let redraw_msg = KernelToWM::RequestRedraw {
                                id: my_pid,
                            };
                            let msg_bytes = redraw_msg.to_bytes();
                            let result = send_message(wm_pid, &msg_bytes);
                            if result < 0 {
                                print_debug("Terminal: Failed to send RequestRedraw to WM\r\n");
                            } else {
                                print_debug("Terminal: RequestRedraw sent to WM\r\n");
                            }
                        }
                    }
                    WMToKernel::RouteInput { event, .. } => {
                        // Handle keyboard input
                        if event.event_type == 1 { // KeyPressed
                            // Debug: print key code for Enter key specifically
                            if event.key == 28 {  // Enter is key code 28
                                print_debug("Terminal: Enter key pressed!\r\n");
                            }

                            // Convert evdev keycode to ASCII
                            if let Some(ascii) = librost::input::evdev_to_ascii(event.key, event.modifiers) {
                                unsafe {
                                    handle_shell_input(ascii);

                                    // Re-render to buffer
                                    CONSOLE.render_to_buffer(
                                        pixel_buffer,
                                        buffer_width,
                                        buffer_height
                                    );
                                }

                                // Request WM to redraw
                                let redraw_msg = KernelToWM::RequestRedraw {
                                    id: my_pid,
                                };
                                let msg_bytes = redraw_msg.to_bytes();
                                send_message(wm_pid, &msg_bytes);
                            }
                        }
                    }
                    _ => {
                        // Ignore other WM messages
                    }
                }
            } else if let Some(fs_msg) = FSToApp::from_bytes(&msg_buf) {
                // Handle file server response
                match fs_msg {
                    FSToApp::ListResponse { files, files_len, .. } => {
                        unsafe {
                            if files_len == 0 {
                                CONSOLE.write_string("(no files)\n");
                            } else {
                                // Display file list
                                let files_str = core::str::from_utf8(&files[..files_len]).unwrap_or("(invalid)");
                                CONSOLE.write_string(files_str);
                                CONSOLE.write_string("\n");
                            }

                            // Show prompt for next command
                            show_prompt();

                            // Re-render and request redraw
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        send_message(wm_pid, &redraw_msg.to_bytes());
                    }
                    FSToApp::CreateSuccess { .. } => {
                        unsafe {
                            CONSOLE.write_string("File created successfully\n");
                            show_prompt();
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        send_message(wm_pid, &redraw_msg.to_bytes());
                    }
                    FSToApp::OpenSuccess { fd, .. } => {
                        // Check if this is for a pending write
                        let write_len = PENDING_WRITE_LEN.load(Ordering::SeqCst);
                        if write_len > 0 {
                            // We have pending write data, send write request
                            PENDING_WRITE_FD.store(fd, Ordering::SeqCst);

                            let my_pid = getpid();
                            let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

                            let mut data_buf = [0u8; 200];
                            unsafe {
                                data_buf[..write_len].copy_from_slice(&PENDING_WRITE_DATA[..write_len]);
                            }

                            let write_request = AppToFS::Write {
                                sender_pid: my_pid,
                                request_id,
                                fd,
                                data: data_buf,
                                data_len: write_len,
                            };

                            let request_bytes = write_request.to_bytes();
                            send_message(FILE_SERVER_PID, &request_bytes);

                            // Clear pending write
                            PENDING_WRITE_LEN.store(0, Ordering::SeqCst);
                        }
                    }
                    FSToApp::WriteSuccess { bytes_written, .. } => {
                        // Close the file
                        let fd = PENDING_WRITE_FD.load(Ordering::SeqCst);
                        if fd > 0 {
                            let my_pid = getpid();
                            let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);

                            let close_request = AppToFS::Close {
                                sender_pid: my_pid,
                                request_id,
                                fd,
                            };

                            send_message(FILE_SERVER_PID, &close_request.to_bytes());
                            PENDING_WRITE_FD.store(0, Ordering::SeqCst);
                        }

                        unsafe {
                            CONSOLE.write_string("Wrote ");
                            // Simple number printing
                            if bytes_written >= 100 {
                                CONSOLE.write_char(b'0' + (bytes_written / 100) as u8);
                            }
                            if bytes_written >= 10 {
                                CONSOLE.write_char(b'0' + ((bytes_written / 10) % 10) as u8);
                            }
                            CONSOLE.write_char(b'0' + (bytes_written % 10) as u8);
                            CONSOLE.write_string(" bytes\n");
                            show_prompt();
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        send_message(wm_pid, &redraw_msg.to_bytes());
                    }
                    FSToApp::Error { error_code, .. } => {
                        unsafe {
                            CONSOLE.write_string("Error: File server returned error code ");
                            if error_code < 0 && error_code > -100 {
                                CONSOLE.write_char(b'-');
                                let code = (-error_code) as u8;
                                if code >= 10 {
                                    CONSOLE.write_char(b'0' + code / 10);
                                }
                                CONSOLE.write_char(b'0' + code % 10);
                            }
                            CONSOLE.write_string("\n");
                            show_prompt();
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        send_message(wm_pid, &redraw_msg.to_bytes());
                    }
                    _ => {
                        // Ignore other file server messages
                    }
                }
            }
        }

        // Yield CPU to other processes
        yield_now();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in terminal!\r\n");
    exit(1);
}
