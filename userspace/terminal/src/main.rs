#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering};

// File server PID (well-known - file server should be spawned as PID 2)
const FILE_SERVER_PID: u32 = 2;

// Request ID counter for IPC
static REQUEST_COUNTER: AtomicU32 = AtomicU32::new(1);

// Pending write data (for write command - store data until we get file descriptor from open)
static mut PENDING_WRITE_DATA: [u8; 200] = [0; 200];
static PENDING_WRITE_LEN: AtomicUsize = AtomicUsize::new(0);
static PENDING_WRITE_FD: AtomicU32 = AtomicU32::new(0);

// Shared buffer info (from WM's WindowCreated message)
// CRITICAL: Use unsafe statics with volatile access to prevent compiler optimization
static mut PIXEL_BUFFER: *mut u32 = core::ptr::null_mut();
static mut BUFFER_WIDTH: u32 = 0;
static mut BUFFER_HEIGHT: u32 = 0;
static mut BUFFER_LEN: usize = 0; // Total pixel count

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
/// Uses AtomicU8 for buffer to properly express that this memory is visible
/// across process boundaries (Terminal → WM via shared memory IPC)
struct Console {
    buffer: [[AtomicU8; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
    cursor_x: AtomicUsize,
    cursor_y: AtomicUsize,
}

impl Console {
    const fn new() -> Self {
        // SAFETY: AtomicU8::new is const, but array initialization requires manual construction
        const EMPTY_CELL: AtomicU8 = AtomicU8::new(b' ');
        const EMPTY_ROW: [AtomicU8; CONSOLE_WIDTH] = [EMPTY_CELL; CONSOLE_WIDTH];

        Self {
            buffer: [EMPTY_ROW; CONSOLE_HEIGHT],
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
                    self.buffer[y][new_x].store(b' ', Ordering::Release);
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
                    self.buffer[new_y.min(CONSOLE_HEIGHT - 1)][x].store(ch, Ordering::Release);
                } else {
                    // Normal write - y doesn't change
                    self.buffer[y][x].store(ch, Ordering::Release);
                }

                x += 1;
                self.cursor_x.store(x, Ordering::SeqCst);
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
                let ch = self.buffer[y][x].load(Ordering::Acquire);
                self.buffer[y - 1][x].store(ch, Ordering::Release);
            }
        }
        // Clear last line
        for x in 0..CONSOLE_WIDTH {
            self.buffer[CONSOLE_HEIGHT - 1][x].store(b' ', Ordering::Release);
        }
    }

    /// Clear the console
    fn clear(&mut self) {
        // REALITY CHECK: Even with AtomicU8, LLVM can still optimize away loop iterations
        // AtomicU8 tells compiler "this memory is shared" but doesn't prevent loop unrolling/elimination
        // We need BOTH: Atomics (for semantics) + black_box (to force all iterations)
        let h = core::hint::black_box(CONSOLE_HEIGHT);
        let w = core::hint::black_box(CONSOLE_WIDTH);
        for y in 0..h {
            for x in 0..w {
                self.buffer[y][x].store(b' ', Ordering::Release);
            }
        }
        self.cursor_x.store(0, Ordering::SeqCst);
        self.cursor_y.store(0, Ordering::SeqCst);
    }

    /// Render console to a pixel buffer (ARGB format)
    /// buffer_width and buffer_height are in pixels
    /// draw_backgrounds: if true, fill cell backgrounds (for incremental updates); if false, assume buffer is already cleared
    fn render_to_buffer(&self, pixel_buffer: &mut [u32], buffer_width: usize, buffer_height: usize, draw_backgrounds: bool) {
        let bg_color = BG_COLOR.load(Ordering::SeqCst);
        let fg_color = FG_COLOR.load(Ordering::SeqCst);

        // Draw console text - atomic loads properly handle cross-process visibility
        for y in 0..CONSOLE_HEIGHT {
            for x in 0..CONSOLE_WIDTH {
                let ch = self.buffer[y][x].load(Ordering::Acquire);

                // Calculate character cell position
                let cell_x = (x as i32) * CHAR_WIDTH as i32;
                let cell_y = (y as i32) * LINE_HEIGHT as i32;

                // Draw background for this character cell (clears cursor/old text)
                // Only needed for incremental updates, not after full-screen clear
                if draw_backgrounds {
                    for dy in 0..CHAR_HEIGHT {
                        for dx in 0..CHAR_WIDTH {
                            let px = cell_x + dx as i32;
                            let py = cell_y + dy as i32;
                            if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                                let idx = (py as usize * buffer_width) + px as usize;
                                if idx < pixel_buffer.len() {
                                    // Shared memory writes to pixel buffer
                                    unsafe { core::ptr::write_volatile(&mut pixel_buffer[idx], bg_color); }
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
                        fg_color
                    );
                }
            }
        }

        // Draw cursor
        let cursor_x_pos = self.cursor_x.load(Ordering::SeqCst);
        let cursor_y_pos = self.cursor_y.load(Ordering::SeqCst);

        if cursor_x_pos < CONSOLE_WIDTH && cursor_y_pos < CONSOLE_HEIGHT {
            let cursor_x = (cursor_x_pos as i32) * CHAR_WIDTH as i32;
            let cursor_y = (cursor_y_pos as i32) * LINE_HEIGHT as i32;

            for dy in 0..CHAR_HEIGHT {
                for dx in 0..CHAR_WIDTH {
                    let px = cursor_x + dx as i32;
                    let py = cursor_y + dy as i32;
                    if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                        let idx = (py as usize * buffer_width) + px as usize;
                        if idx < pixel_buffer.len() {
                            unsafe { core::ptr::write_volatile(&mut pixel_buffer[idx], 0xFF00FF00); }
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

    // Bounds check to prevent panic
    if pos == 0 || pos > MAX_COMMAND_LEN {
        COMMAND_POS.store(0, Ordering::SeqCst);
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
    show_prompt();

    // Re-render cleared buffer to pixels and notify WM
    let buffer_width = unsafe { core::ptr::read_volatile(&BUFFER_WIDTH) } as usize;
    let buffer_height = unsafe { core::ptr::read_volatile(&BUFFER_HEIGHT) } as usize;
    let buffer_len = unsafe { core::ptr::read_volatile(&BUFFER_LEN) };

    if !PIXEL_BUFFER.is_null() && buffer_len > 0 {
        let pixel_buffer = core::slice::from_raw_parts_mut(PIXEL_BUFFER, buffer_len);

        // Clear entire screen to black
        let bg_color = BG_COLOR.load(Ordering::SeqCst);
        for i in 0..pixel_buffer.len() {
            unsafe {
                core::ptr::write_volatile(&mut pixel_buffer[i], bg_color);
            }
        }

        // Render cleared console (just prompt and cursor)
        CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height, false);

        let wm_pid = 1; // WM is always PID 1
        let my_pid = getpid() as usize;
        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
    }
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
    // Note: Console buffer is already initialized with spaces via AtomicU8 const initialization
    unsafe {
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
                            print_debug("W=");
                            // Print width as hex
                            for i in (0..8).rev() {
                                let digit = (width >> (i * 4)) & 0xF;
                                let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                                print_debug(core::str::from_utf8(&[ch]).unwrap_or("?"));
                            }
                            print_debug(" H=");
                            // Print height as hex
                            for i in (0..8).rev() {
                                let digit = (height >> (i * 4)) & 0xF;
                                let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                                print_debug(core::str::from_utf8(&[ch]).unwrap_or("?"));
                            }
                            print_debug(" shmid=");
                            if shm_id >= 0 && shm_id < 10 {
                                let s = [b'0' + shm_id as u8];
                                print_debug(core::str::from_utf8(&s).unwrap_or("?"));
                            }
                            print_debug("\r\n");

                            // Map the WM's shared memory buffer
                            print_debug("Terminal: Mapping WM's buffer\r\n");
                            let shmem_ptr = shm_map(shm_id);

                            if shmem_ptr.is_null() {
                                print_debug("Terminal: Failed to map WM's shared memory\r\n");
                                exit(1);
                            }

                            print_debug("Terminal: Buffer mapped successfully\r\n");

                            // Sanity check dimensions (max 16MB buffer = 4M pixels)
                            let max_pixels: u32 = 4 * 1024 * 1024; // 4M pixels = 16MB
                            if width == 0 || height == 0 || width > 4096 || height > 4096 || (width * height) > max_pixels {
                                print_debug("Terminal: ERROR - Invalid buffer dimensions!\r\n");
                                exit(1);
                            }

                            // Store buffer dimensions
                            buffer_width = width as usize;
                            buffer_height = height as usize;

                            // Store in statics so cmd_clear can access (use volatile to prevent optimization)
                            let total_pixels = (width as usize) * (height as usize);
                            unsafe {
                                core::ptr::write_volatile(&mut BUFFER_WIDTH, width);
                                core::ptr::write_volatile(&mut BUFFER_HEIGHT, height);
                                core::ptr::write_volatile(&mut BUFFER_LEN, total_pixels);
                                core::ptr::write_volatile(&mut PIXEL_BUFFER, shmem_ptr as *mut u32);
                            }

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
                                    buffer_height,
                                    true
                                );
                            }

                            print_debug("Terminal: Initial render complete\r\n");

                            // Request WM to redraw (show our initial content)
                            // CRITICAL: Use sync_and_notify to ensure memory barrier
                            let redraw_msg = KernelToWM::RequestRedraw {
                                id: my_pid,
                            };
                            librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
                            print_debug("Terminal: RequestRedraw sent to WM\r\n");
                        }
                    }
                    WMToKernel::RouteInput { event, .. } => {
                        // Handle keyboard input
                        if event.event_type == 1 { // KeyPressed
                            // Convert evdev keycode to ASCII
                            if let Some(ascii) = librost::input::evdev_to_ascii(event.key, event.modifiers) {
                                print_debug("KEY:");
                                if ascii >= 32 && ascii < 127 {
                                    let s = [ascii];
                                    if let Ok(s) = core::str::from_utf8(&s) {
                                        print_debug(s);
                                    }
                                }
                                print_debug(" BUF=");
                                if pixel_buffer.is_empty() {
                                    print_debug("EMPTY");
                                } else {
                                    print_debug("OK");
                                }
                                print_debug(" ");

                                unsafe {
                                    handle_shell_input(ascii);

                                    // Only render if buffer is initialized
                                    if !pixel_buffer.is_empty() {
                                        print_debug("RENDER ");

                                        // Re-render to buffer
                                        CONSOLE.render_to_buffer(
                                            pixel_buffer,
                                            buffer_width,
                                            buffer_height,
                                            true
                                        );

                                        // CRITICAL: Use sync_and_notify to ensure memory barrier + IPC
                                        let redraw_msg = KernelToWM::RequestRedraw {
                                            id: my_pid,
                                        };
                                        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
                                        print_debug("SENT ");
                                    } else {
                                        print_debug("SKIP(empty) ");
                                    }
                                }
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
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height, true);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
                    }
                    FSToApp::CreateSuccess { .. } => {
                        unsafe {
                            CONSOLE.write_string("File created successfully\n");
                            show_prompt();
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height, true);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
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
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height, true);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
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
                            CONSOLE.render_to_buffer(pixel_buffer, buffer_width, buffer_height, true);
                        }
                        let redraw_msg = KernelToWM::RequestRedraw { id: my_pid };
                        librost::sync_and_notify(wm_pid, &redraw_msg.to_bytes());
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
fn panic(info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in terminal: ");
    if let Some(location) = info.location() {
        print_debug(location.file());
        print_debug(":");
        // Print line number digit by digit
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
