#![no_std]
#![no_main]

extern crate alloc;
use librost::*;
use csv_core::{Reader, ReadFieldResult};

// Simple bump allocator for userspace
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const HEAP_SIZE: usize = 64 * 1024; // 64KB heap

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

// Constants for rendering
const CELL_WIDTH: usize = 150;
const CELL_HEIGHT: usize = 30;
const HEADER_HEIGHT: usize = 40;
const BORDER_COLOR: u32 = 0xFF_44_44_44;
const HEADER_BG: u32 = 0xFF_2D_2D_30;
const CELL_BG: u32 = 0xFF_1E_1E_1E;
const TEXT_COLOR: u32 = 0xFF_CC_CC_CC;
const SELECTED_BG: u32 = 0xFF_26_4F_78;

// CSV data storage (static allocation for no_std)
const MAX_ROWS: usize = 100;
const MAX_COLS: usize = 20;
const MAX_CELL_LEN: usize = 64;

struct CsvData {
    cells: [[[u8; MAX_CELL_LEN]; MAX_COLS]; MAX_ROWS],
    cell_lens: [[usize; MAX_COLS]; MAX_ROWS],
    rows: usize,
    cols: usize,
}

impl CsvData {
    const fn new() -> Self {
        Self {
            cells: [[[0; MAX_CELL_LEN]; MAX_COLS]; MAX_ROWS],
            cell_lens: [[0; MAX_COLS]; MAX_ROWS],
            rows: 0,
            cols: 0,
        }
    }

    fn set_cell(&mut self, row: usize, col: usize, data: &[u8]) {
        if row >= MAX_ROWS || col >= MAX_COLS {
            return;
        }

        let len = core::cmp::min(data.len(), MAX_CELL_LEN - 1);
        self.cells[row][col][..len].copy_from_slice(&data[..len]);
        self.cells[row][col][len] = 0; // Null terminator
        self.cell_lens[row][col] = len;

        if row >= self.rows {
            self.rows = row + 1;
        }
        if col >= self.cols {
            self.cols = col + 1;
        }
    }

    fn get_cell(&self, row: usize, col: usize) -> &[u8] {
        if row >= self.rows || col >= self.cols {
            return &[];
        }
        &self.cells[row][col][..self.cell_lens[row][col]]
    }
}

fn render_spreadsheet(
    fb_width: usize,
    fb_height: usize,
    data: &CsvData,
    scroll_x: usize,
    scroll_y: usize,
    selected_row: usize,
    selected_col: usize,
) {
    // Clear screen
    draw_rect(0, 0, fb_width as u32, fb_height as u32, 0xFF_00_00_00);

    // Draw header
    draw_rect(0, 0, fb_width as u32, HEADER_HEIGHT as u32, HEADER_BG);
    draw_text_proper(10, 8, "CSV Viewer - Arrow keys to navigate, Q to quit", TEXT_COLOR);

    let start_y = HEADER_HEIGHT + 10;
    let visible_rows = (fb_height - start_y) / CELL_HEIGHT;
    let visible_cols = fb_width / CELL_WIDTH;

    // Draw grid
    for row_idx in 0..visible_rows {
        let data_row = scroll_y + row_idx;
        if data_row >= data.rows {
            break;
        }

        for col_idx in 0..visible_cols {
            let data_col = scroll_x + col_idx;
            if data_col >= data.cols {
                break;
            }

            let x = col_idx * CELL_WIDTH;
            let y = start_y + row_idx * CELL_HEIGHT;

            // Background color (highlight selected cell)
            let bg_color = if data_row == selected_row && data_col == selected_col {
                SELECTED_BG
            } else {
                CELL_BG
            };

            draw_rect(x as u32, y as u32, (CELL_WIDTH - 1) as u32, (CELL_HEIGHT - 1) as u32, bg_color);

            // Draw cell border
            draw_rect((x + CELL_WIDTH - 1) as u32, y as u32, 1, CELL_HEIGHT as u32, BORDER_COLOR);
            draw_rect(x as u32, (y + CELL_HEIGHT - 1) as u32, CELL_WIDTH as u32, 1, BORDER_COLOR);

            // Draw cell text
            let cell_data = data.get_cell(data_row, data_col);
            if !cell_data.is_empty() {
                if let Ok(text) = core::str::from_utf8(cell_data) {
                    draw_text_proper((x + 4) as i32, (y + 6) as i32, text, TEXT_COLOR);
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt CSV Viewer ===\r\n");
    print_debug("Running at EL0\r\n");

    // Parse CSV file
    static mut CSV_DATA: CsvData = CsvData::new();

    let data = unsafe { &mut CSV_DATA };

    // Read CSV file
    print_debug("Opening data.csv...\r\n");
    let fd = open("data.csv", 0); // O_RDONLY = 0

    if fd < 0 {
        print_debug("Failed to open data.csv, creating sample data...\r\n");

        // Create sample data
        data.set_cell(0, 0, b"Name");
        data.set_cell(0, 1, b"Age");
        data.set_cell(0, 2, b"City");
        data.set_cell(0, 3, b"Email");
        data.set_cell(1, 0, b"Alice");
        data.set_cell(1, 1, b"25");
        data.set_cell(1, 2, b"NYC");
        data.set_cell(1, 3, b"alice@example.com");
        data.set_cell(2, 0, b"Bob");
        data.set_cell(2, 1, b"30");
        data.set_cell(2, 2, b"SF");
        data.set_cell(2, 3, b"bob@example.com");
        data.set_cell(3, 0, b"Charlie");
        data.set_cell(3, 1, b"35");
        data.set_cell(3, 2, b"LA");
        data.set_cell(3, 3, b"charlie@example.com");
        data.set_cell(4, 0, b"Diana");
        data.set_cell(4, 1, b"28");
        data.set_cell(4, 2, b"Seattle");
        data.set_cell(4, 3, b"diana@example.com");
    } else {
        print_debug("Reading CSV data...\r\n");

        // Read file into buffer
        let mut file_buf = [0u8; 4096];
        let bytes_read = read(fd, &mut file_buf);
        close(fd);

        if bytes_read > 0 {
            print_debug("Parsing CSV...\r\n");

            // Parse CSV using csv-core
            let csv_data = &file_buf[..bytes_read as usize];
            let mut reader = Reader::new();
            let mut field_buf = [0u8; MAX_CELL_LEN];
            let mut row = 0;
            let mut col = 0;
            let mut pos = 0;

            loop {
                let (result, n_in, n_out) = reader.read_field(
                    &csv_data[pos..],
                    &mut field_buf
                );

                pos += n_in;

                match result {
                    ReadFieldResult::Field { record_end } => {
                        // Store the field
                        data.set_cell(row, col, &field_buf[..n_out]);

                        if record_end {
                            row += 1;
                            col = 0;
                            if row >= MAX_ROWS {
                                break;
                            }
                        } else {
                            col += 1;
                            if col >= MAX_COLS {
                                // Skip to next record
                                col = 0;
                                row += 1;
                            }
                        }
                    }
                    ReadFieldResult::End => {
                        break;
                    }
                    _ => {
                        if pos >= csv_data.len() {
                            break;
                        }
                    }
                }
            }

            print_debug("CSV parsing complete\r\n");
        } else {
            print_debug("Failed to read file\r\n");
        }
    }

    // Get framebuffer info
    let fb_info = match fb_info() {
        Some(info) => info,
        None => {
            print_debug("Failed to get framebuffer info\r\n");
            exit(1);
        }
    };

    let fb_width = fb_info.width as usize;
    let fb_height = fb_info.height as usize;

    // Navigation state
    let mut scroll_x = 0;
    let mut scroll_y = 0;
    let mut selected_row = 0;
    let mut selected_col = 0;

    print_debug("Starting main loop...\r\n");

    // Main loop
    loop {
        // Render
        render_spreadsheet(
            fb_width,
            fb_height,
            data,
            scroll_x,
            scroll_y,
            selected_row,
            selected_col,
        );

        fb_flush();

        // Handle input
        if let Some(event) = poll_event() {
            if event.event_type == 1 {  // KeyPressed
                match event.key {
                    0x71 => {  // 'q' - quit
                        print_debug("Exiting CSV viewer...\r\n");
                        exit(0);
                    }
                    103 => {  // Up arrow (KEY_UP)
                        if selected_row > 0 {
                            selected_row -= 1;
                            if selected_row < scroll_y {
                                scroll_y = selected_row;
                            }
                        }
                    }
                    108 => {  // Down arrow (KEY_DOWN)
                        if selected_row + 1 < data.rows {
                            selected_row += 1;
                            let visible_rows = (fb_height - HEADER_HEIGHT - 10) / CELL_HEIGHT;
                            if selected_row >= scroll_y + visible_rows {
                                scroll_y = selected_row - visible_rows + 1;
                            }
                        }
                    }
                    105 => {  // Left arrow (KEY_LEFT)
                        if selected_col > 0 {
                            selected_col -= 1;
                            if selected_col < scroll_x {
                                scroll_x = selected_col;
                            }
                        }
                    }
                    106 => {  // Right arrow (KEY_RIGHT)
                        if selected_col + 1 < data.cols {
                            selected_col += 1;
                            let visible_cols = fb_width / CELL_WIDTH;
                            if selected_col >= scroll_x + visible_cols {
                                scroll_x = selected_col - visible_cols + 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Small delay to avoid busy-waiting
        for _ in 0..50000 {
            unsafe { core::arch::asm!("nop"); }
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in CSV viewer!\r\n");
    exit(1);
}
