#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

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
                    unsafe {
                        core::ptr::write_volatile(&mut self.buffer[self.cursor_y][self.cursor_x], b' ');
                    }
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
                // Use volatile write to prevent compiler optimization
                unsafe {
                    core::ptr::write_volatile(&mut self.buffer[self.cursor_y][self.cursor_x], ch);
                }
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
            self.buffer[y - 1] = self.buffer[y];
        }
        self.buffer[CONSOLE_HEIGHT - 1] = [b' '; CONSOLE_WIDTH];
    }

    /// Clear the console
    fn clear(&mut self) {
        self.buffer = [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT];
        self.cursor_x = 0;
        self.cursor_y = 0;
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
        if self.cursor_x < CONSOLE_WIDTH && self.cursor_y < CONSOLE_HEIGHT {
            let cursor_x = (self.cursor_x as i32) * CHAR_WIDTH as i32;
            let cursor_y = (self.cursor_y as i32) * LINE_HEIGHT as i32;

            // Draw a filled rectangle for the cursor
            for dy in 0..CHAR_HEIGHT {
                for dx in 0..CHAR_WIDTH {
                    let px = cursor_x + dx as i32;
                    let py = cursor_y + dy as i32;
                    if px >= 0 && px < buffer_width as i32 && py >= 0 && py < buffer_height as i32 {
                        let idx = (py as usize * buffer_width) + px as usize;
                        if idx < pixel_buffer.len() {
                            pixel_buffer[idx] = 0xFF00FF00; // Green cursor
                        }
                    }
                }
            }
        }
    }
}

static mut CONSOLE: Console = Console::new();

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
        CONSOLE.cursor_x = 0;
        CONSOLE.cursor_y = 0;

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
            // Parse message from WM
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
                            let ch = event.key;

                            unsafe {
                                CONSOLE.write_char(ch);

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
                    _ => {
                        // Ignore other messages
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
