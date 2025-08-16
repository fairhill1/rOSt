use core::ptr::{read_volatile, write_volatile};

// PL011 UART registers (offsets from base)
const UARTDR: usize = 0x000;     // Data register
const UARTFR: usize = 0x018;     // Flag register
const UARTIBRD: usize = 0x024;   // Integer baud rate
const UARTFBRD: usize = 0x028;   // Fractional baud rate
const UARTLCR_H: usize = 0x02C;  // Line control
const UARTCR: usize = 0x030;     // Control register
const UARTIMSC: usize = 0x038;   // Interrupt mask set/clear
const UARTRIS: usize = 0x03C;    // Raw interrupt status
const UARTMIS: usize = 0x040;    // Masked interrupt status
const UARTICR: usize = 0x044;    // Interrupt clear

// Flag register bits
const UARTFR_RXFE: u32 = 1 << 4; // Receive FIFO empty
const UARTFR_TXFF: u32 = 1 << 5; // Transmit FIFO full

// Control register bits
const UARTCR_UARTEN: u32 = 1 << 0;  // UART enable
const UARTCR_TXE: u32 = 1 << 8;     // Transmit enable
const UARTCR_RXE: u32 = 1 << 9;     // Receive enable

// Interrupt bits
const UARTIMSC_RXIM: u32 = 1 << 4;  // Receive interrupt mask
const UARTIMSC_TXIM: u32 = 1 << 5;  // Transmit interrupt mask

pub const UART_IRQ: u32 = 33;  // PL011 UART interrupt

pub struct Uart {
    base_address: usize,
}

impl Uart {
    pub const fn new(base_address: usize) -> Self {
        Uart { base_address }
    }
    
    pub fn init(&self) {
        unsafe {
            // Disable UART during configuration
            write_volatile((self.base_address + UARTCR) as *mut u32, 0);
            
            // Clear pending interrupts
            write_volatile((self.base_address + UARTICR) as *mut u32, 0x7FF);
            
            // Set baud rate (115200 for 24MHz UART clock)
            // Divisor = 24000000 / (16 * 115200) = 13.02
            // Integer part = 13, Fractional part = 0.02 * 64 = 1
            write_volatile((self.base_address + UARTIBRD) as *mut u32, 13);
            write_volatile((self.base_address + UARTFBRD) as *mut u32, 1);
            
            // Line control: 8 bits, no parity, 1 stop bit, FIFOs enabled
            write_volatile((self.base_address + UARTLCR_H) as *mut u32, 0x70);
            
            // Enable receive interrupt
            write_volatile((self.base_address + UARTIMSC) as *mut u32, UARTIMSC_RXIM);
            
            // Enable UART, receive and transmit
            write_volatile((self.base_address + UARTCR) as *mut u32, 
                         UARTCR_UARTEN | UARTCR_RXE | UARTCR_TXE);
        }
    }
    
    pub fn putc(&self, c: u8) {
        unsafe {
            // Wait until transmit FIFO is not full
            while (read_volatile((self.base_address + UARTFR) as *const u32) & UARTFR_TXFF) != 0 {
                core::hint::spin_loop();
            }
            write_volatile((self.base_address + UARTDR) as *mut u32, c as u32);
        }
    }
    
    pub fn getc(&self) -> Option<u8> {
        unsafe {
            // Check if receive FIFO is empty
            if (read_volatile((self.base_address + UARTFR) as *const u32) & UARTFR_RXFE) != 0 {
                None
            } else {
                Some(read_volatile((self.base_address + UARTDR) as *const u32) as u8)
            }
        }
    }
    
    pub fn clear_interrupt(&self) {
        unsafe {
            // Clear all interrupts
            write_volatile((self.base_address + UARTICR) as *mut u32, 0x7FF);
        }
    }
    
    pub fn puts(&self, s: &str) {
        for byte in s.bytes() {
            self.putc(byte);
        }
    }
    
    pub fn put_hex(&self, mut n: u64) {
        self.puts("0x");
        
        if n == 0 {
            self.putc(b'0');
            return;
        }
        
        let mut digits = [0u8; 16];
        let mut i = 0;
        
        while n > 0 {
            let digit = (n & 0xF) as u8;
            digits[i] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + (digit - 10)
            };
            n >>= 4;
            i += 1;
        }
        
        while i > 0 {
            i -= 1;
            self.putc(digits[i]);
        }
    }
}