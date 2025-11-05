#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

// Bump allocator for userspace
const HEAP_SIZE: usize = 256 * 1024; // 256KB heap (larger than WM since we store console buffer + command history)

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
    fg_color: u32,
    bg_color: u32,
}

impl Console {
    const fn new() -> Self {
        Self {
            buffer: [[b' '; CONSOLE_WIDTH]; CONSOLE_HEIGHT],
            cursor_x: 0,
            cursor_y: 0,
            fg_color: 0xFFFFFFFF, // White text
            bg_color: 0xFF000000, // Black background
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
        // Clear to background color
        for pixel in pixel_buffer.iter_mut() {
            *pixel = self.bg_color;
        }

        // TODO: Render text characters using bitmap font
        // For now, just show black background to verify window works
    }
}

static mut CONSOLE: Console = Console::new();

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt Terminal (EL0) ===\r\n");
    print_debug("Initializing...\r\n");

    // Initialize console (DISABLED - causes panic with bump allocator)
    // unsafe {
    //     CONSOLE.write_string("rOSt Terminal v0.1\r\n");
    //     CONSOLE.write_string("Type 'help' for available commands\r\n");
    //     CONSOLE.write_string("\r\n> ");
    // }

    // Get window dimensions - make it large enough for fullscreen
    // TODO: Query actual screen size from kernel
    let window_width = 1920u32;
    let window_height = 1048u32;  // 1080 - 32 (menu bar)

    print_debug("About to create shared memory\r\n");

    // Create shared memory for rendering (ARGB pixels)
    let fb_size = (window_width * window_height * 4) as usize; // 4 bytes per pixel

    print_debug("Calling shm_create\r\n");
    let shmem_id = shm_create(fb_size);
    print_debug("shm_create returned\r\n");

    if shmem_id < 0 {
        print_debug("Failed to create shared memory\r\n");
        exit(1);
    }

    print_debug("Created shared memory for framebuffer\r\n");

    // Map shared memory
    let shmem_ptr = shm_map(shmem_id as i32);
    if shmem_ptr.is_null() {
        print_debug("Failed to map shared memory\r\n");
        exit(1);
    }

    let pixel_buffer = unsafe {
        core::slice::from_raw_parts_mut(shmem_ptr as *mut u32, (window_width * window_height) as usize)
    };

    print_debug("Mapped shared memory\r\n");

    // Render initial frame
    unsafe {
        CONSOLE.render_to_buffer(pixel_buffer, window_width as usize, window_height as usize);
    }

    print_debug("Rendered initial frame\r\n");

    // Send CreateWindow IPC to WM (PID 1)
    let wm_pid = 1;
    let mut title = [0u8; 64];
    let title_str = b"Terminal";
    title[..title_str.len()].copy_from_slice(title_str);

    let create_window_msg = KernelToWM::CreateWindow {
        id: shmem_id as usize,  // Use shared memory ID as window ID
        x: 50,
        y: 100,
        width: window_width,
        height: window_height,
        title,
        title_len: title_str.len(),
    };

    let msg_bytes = create_window_msg.to_bytes();
    let result = send_message(wm_pid, &msg_bytes);

    if result < 0 {
        print_debug("Failed to send CreateWindow message to WM\r\n");
    } else {
        print_debug("CreateWindow message sent to WM\r\n");
    }

    print_debug("Terminal initialized successfully\r\n");

    // Main event loop - wait for input events from WM
    loop {
        // TODO: Receive input events from WM
        // TODO: Update console based on input
        // TODO: Re-render and notify WM of updates
        yield_now();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in terminal!\r\n");
    exit(1);
}
