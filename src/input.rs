use core::sync::atomic::{AtomicUsize, Ordering};

const BUFFER_SIZE: usize = 256;

pub struct InputBuffer {
    buffer: [u8; BUFFER_SIZE],
    read_pos: AtomicUsize,
    write_pos: AtomicUsize,
}

impl InputBuffer {
    pub const fn new() -> Self {
        InputBuffer {
            buffer: [0; BUFFER_SIZE],
            read_pos: AtomicUsize::new(0),
            write_pos: AtomicUsize::new(0),
        }
    }
    
    pub fn push(&mut self, byte: u8) -> bool {
        let write = self.write_pos.load(Ordering::Relaxed);
        let next_write = (write + 1) % BUFFER_SIZE;
        let read = self.read_pos.load(Ordering::Relaxed);
        
        if next_write == read {
            return false; // Buffer full
        }
        
        self.buffer[write] = byte;
        self.write_pos.store(next_write, Ordering::Release);
        true
    }
    
    pub fn pop(&mut self) -> Option<u8> {
        let read = self.read_pos.load(Ordering::Relaxed);
        let write = self.write_pos.load(Ordering::Acquire);
        
        if read == write {
            return None; // Buffer empty
        }
        
        let byte = self.buffer[read];
        let next_read = (read + 1) % BUFFER_SIZE;
        self.read_pos.store(next_read, Ordering::Release);
        Some(byte)
    }
    
    pub fn is_empty(&self) -> bool {
        self.read_pos.load(Ordering::Relaxed) == self.write_pos.load(Ordering::Acquire)
    }
}

pub static mut INPUT_BUFFER: InputBuffer = InputBuffer::new();

pub fn handle_uart_input(uart: &crate::uart::Uart) {
    while let Some(byte) = uart.getc() {
        unsafe {
            // Echo the character back
            uart.putc(byte);
            
            // Handle special characters
            match byte {
                b'\r' | b'\n' => {
                    uart.puts("\n");
                    INPUT_BUFFER.push(b'\n');
                }
                0x7f | 0x08 => { // Backspace or DEL
                    // Move cursor back, print space, move back again
                    uart.puts("\x08 \x08");
                }
                _ if byte >= 0x20 && byte < 0x7f => {
                    // Printable ASCII
                    INPUT_BUFFER.push(byte);
                }
                _ => {} // Ignore other characters
            }
        }
    }
    
    uart.clear_interrupt();
}