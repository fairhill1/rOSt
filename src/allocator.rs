use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

const HEAP_SIZE: usize = 1024 * 1024; // 1MB heap
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
static mut HEAP_POS: usize = 0;

struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        
        // Align the position
        let start = (HEAP_POS + align - 1) & !(align - 1);
        let end = start + size;
        
        if end > HEAP_SIZE {
            return ptr::null_mut(); // Out of memory
        }
        
        HEAP_POS = end;
        HEAP.as_mut_ptr().add(start)
    }
    
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't free memory
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[alloc_error_handler]
fn alloc_error_handler(layout: Layout) -> ! {
    let uart = crate::uart::Uart::new(0x0900_0000);
    uart.puts("Allocation error: size=");
    uart.put_hex(layout.size() as u64);
    uart.puts(" align=");
    uart.put_hex(layout.align() as u64);
    uart.puts("\n");
    panic!("Allocation error");
}

pub fn init() {
    // Nothing to do for bump allocator
}