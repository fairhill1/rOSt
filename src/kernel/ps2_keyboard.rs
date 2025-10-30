// PS/2 Keyboard Driver - Simple and reliable keyboard input
// This bypasses USB/XHCI complexity and works directly with PS/2 controller

use crate::kernel::uart_write_string;
use crate::kernel::usb_hid::{InputEvent, queue_input_event, scancode_to_ascii};

// PS/2 Controller ports
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;
const PS2_COMMAND_PORT: u16 = 0x64;

// PS/2 Status register bits
const PS2_STATUS_OUTPUT_FULL: u8 = 1 << 0;  // Output buffer has data
const PS2_STATUS_INPUT_FULL: u8 = 1 << 1;   // Input buffer has data
const PS2_STATUS_SYSTEM: u8 = 1 << 2;       // System flag
const PS2_STATUS_COMMAND: u8 = 1 << 3;      // Command/data flag
const PS2_STATUS_TIMEOUT: u8 = 1 << 6;      // Timeout error
const PS2_STATUS_PARITY: u8 = 1 << 7;       // Parity error

// PS/2 Commands
const PS2_CMD_DISABLE_PORT1: u8 = 0xAD;
const PS2_CMD_DISABLE_PORT2: u8 = 0xA7;
const PS2_CMD_ENABLE_PORT1: u8 = 0xAE;
const PS2_CMD_ENABLE_PORT2: u8 = 0xA8;
const PS2_CMD_READ_CONFIG: u8 = 0x20;
const PS2_CMD_WRITE_CONFIG: u8 = 0x60;

// Keyboard scan codes (make codes)
const KEY_ESCAPE: u8 = 0x01;
const KEY_BACKSPACE: u8 = 0x0E;
const KEY_TAB: u8 = 0x0F;
const KEY_ENTER: u8 = 0x1C;
const KEY_LEFT_CTRL: u8 = 0x1D;
const KEY_LEFT_SHIFT: u8 = 0x2A;
const KEY_RIGHT_SHIFT: u8 = 0x36;
const KEY_LEFT_ALT: u8 = 0x38;
const KEY_SPACE: u8 = 0x39;
const KEY_CAPS_LOCK: u8 = 0x3A;

// Keyboard state tracking
static mut SHIFT_PRESSED: bool = false;
static mut CTRL_PRESSED: bool = false;
static mut ALT_PRESSED: bool = false;

/// PS/2 scan code to USB HID scan code conversion
/// PS/2 uses different scan codes than USB HID
const PS2_TO_HID_SCANCODE: [u8; 256] = [
    // 0x00-0x0F
    0, 0x29, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E, 0x2A, 0x2B,
    // 0x10-0x1F - Q,W,E,R,T,Y,U,I,O,P,[,],Enter,Ctrl,A,S
    0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30, 0x28, 0xE0, 0x04, 0x16,
    // 0x20-0x2F - D,F,G,H,J,K,L,;,',`,Shift,\,Z,X,C,V
    0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x35, 0xE1, 0x31, 0x1D, 0x1B, 0x06, 0x19,
    // 0x30-0x3F - B,N,M,comma,period,/,RShift,*,Alt,Space,CapsLock,F1-F5
    0x05, 0x11, 0x10, 0x36, 0x37, 0x38, 0xE5, 0x55, 0xE2, 0x2C, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E,
    // Rest filled with zeros for now
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

// On ARM64/QEMU virt machine, there's no PS/2 controller
// Instead, we'll implement a simple keyboard polling using UART input simulation
// This is more appropriate for ARM64 systems

/// Check UART for keyboard input (ARM64 approach)
fn check_uart_input() -> Option<u8> {
    const UART_BASE: u64 = 0x09000000;
    const UART_FR: u64 = UART_BASE + 0x18;  // Flag register
    const UART_DR: u64 = UART_BASE + 0x00;  // Data register
    
    unsafe {
        // Check if receive FIFO is not empty
        let flags = core::ptr::read_volatile(UART_FR as *const u32);
        if (flags & 0x10) == 0 {  // RXFE bit - receive FIFO empty
            let data = core::ptr::read_volatile(UART_DR as *const u32);
            Some((data & 0xFF) as u8)
        } else {
            None
        }
    }
}

/// Dummy functions for PS/2 ports (not used on ARM64)
fn ps2_read_port(_port: u16) -> u8 {
    0
}

fn ps2_write_port(_port: u16, _value: u8) {
    // No-op on ARM64
}

/// Wait for PS/2 controller to be ready for input
fn ps2_wait_input() -> bool {
    let mut timeout = 1000;
    while timeout > 0 {
        let status = ps2_read_port(PS2_STATUS_PORT);
        if (status & PS2_STATUS_INPUT_FULL) == 0 {
            return true;
        }
        timeout -= 1;
        // Small delay
        for _ in 0..100 { unsafe { core::arch::asm!("nop"); } }
    }
    false
}

/// Wait for PS/2 controller to have output ready
fn ps2_wait_output() -> bool {
    let mut timeout = 1000;
    while timeout > 0 {
        let status = ps2_read_port(PS2_STATUS_PORT);
        if (status & PS2_STATUS_OUTPUT_FULL) != 0 {
            return true;
        }
        timeout -= 1;
        // Small delay
        for _ in 0..100 { unsafe { core::arch::asm!("nop"); } }
    }
    false
}

/// Initialize keyboard input (ARM64 UART-based approach)
pub fn init_ps2_keyboard() -> bool {
    uart_write_string("Initializing keyboard input via UART...\r\n");
    
    // On ARM64 QEMU virt machine, keyboard input comes through UART serial console
    // No actual PS/2 controller to initialize
    
    // Reset modifier state
    unsafe {
        SHIFT_PRESSED = false;
        CTRL_PRESSED = false;
        ALT_PRESSED = false;
    }
    
    uart_write_string("UART keyboard input ready!\r\n");
    uart_write_string("You can type in the QEMU serial console (this terminal)\r\n");
    true
}

/// Check if keyboard input is available and process it
pub fn ps2_keyboard_poll() {
    // Check UART for input characters
    if let Some(ascii_char) = check_uart_input() {
        process_uart_char(ascii_char);
    }
}

/// Process a character received from UART
fn process_uart_char(ascii_char: u8) {
    uart_write_string("UART Key: '");
    unsafe {
        core::ptr::write_volatile(0x09000000 as *mut u8, ascii_char);
    }
    uart_write_string("' (0x");
    print_hex(ascii_char as u64);
    uart_write_string(")\r\n");
    
    // Convert ASCII to a fake HID scancode for the event system
    let fake_hid_scancode = ascii_to_fake_hid(ascii_char);
    let modifiers = 0; // No modifier detection from UART input
    
    // Generate key press and release events
    queue_input_event(InputEvent::KeyPressed { 
        key: fake_hid_scancode, 
        modifiers 
    });
    queue_input_event(InputEvent::KeyReleased { 
        key: fake_hid_scancode, 
        modifiers 
    });
}

/// Convert ASCII character to a fake HID scancode for event compatibility
fn ascii_to_fake_hid(ascii: u8) -> u8 {
    match ascii {
        b'a'..=b'z' => 0x04 + (ascii - b'a'), // HID a-z
        b'A'..=b'Z' => 0x04 + (ascii - b'A'), // HID A-Z (same scancodes)
        b'1'..=b'9' => 0x1E + (ascii - b'1'), // HID 1-9
        b'0' => 0x27,                         // HID 0
        b' ' => 0x2C,                         // HID space
        b'\r' | b'\n' => 0x28,               // HID enter
        b'\t' => 0x2B,                       // HID tab
        b'-' => 0x2D,                        // HID minus
        b'=' => 0x2E,                        // HID equals
        _ => 0x04,                           // Default to 'a' for unknown chars
    }
}

/// Process a PS/2 scan code (kept for compatibility but not used)
fn process_ps2_scancode(scancode: u8) {
    // Check for extended scan codes (0xE0 prefix)
    static mut EXTENDED_MODE: bool = false;
    
    unsafe {
        if scancode == 0xE0 {
            EXTENDED_MODE = true;
            return;
        }
        
        let is_extended = EXTENDED_MODE;
        EXTENDED_MODE = false;
        
        // Check if it's a release code (top bit set)
        let is_release = (scancode & 0x80) != 0;
        let base_scancode = scancode & 0x7F;
        
        // Handle modifier keys
        match base_scancode {
            KEY_LEFT_SHIFT | KEY_RIGHT_SHIFT => {
                SHIFT_PRESSED = !is_release;
                return;
            }
            KEY_LEFT_CTRL => {
                if !is_extended {
                    CTRL_PRESSED = !is_release;
                    return;
                }
            }
            KEY_LEFT_ALT => {
                if !is_extended {
                    ALT_PRESSED = !is_release;
                    return;
                }
            }
            _ => {}
        }
        
        // Convert PS/2 scancode to HID scancode for compatibility
        if base_scancode < PS2_TO_HID_SCANCODE.len() as u8 {
            let hid_scancode = PS2_TO_HID_SCANCODE[base_scancode as usize];
            if hid_scancode != 0 {
                // Create modifiers byte
                let modifiers = (if SHIFT_PRESSED { 0x02 } else { 0 }) |
                               (if CTRL_PRESSED { 0x01 } else { 0 }) |
                               (if ALT_PRESSED { 0x04 } else { 0 });
                
                if is_release {
                    queue_input_event(InputEvent::KeyReleased { 
                        key: hid_scancode, 
                        modifiers 
                    });
                } else {
                    queue_input_event(InputEvent::KeyPressed { 
                        key: hid_scancode, 
                        modifiers 
                    });
                    
                    // Debug output - show the key
                    if let Some(ascii) = scancode_to_ascii(hid_scancode, modifiers) {
                        uart_write_string("PS/2 Key: '");
                        unsafe {
                            core::ptr::write_volatile(0x09000000 as *mut u8, ascii);
                        }
                        uart_write_string("'\r\n");
                    } else {
                        uart_write_string("PS/2 Key: (non-printable) scancode=0x");
                        print_hex(hid_scancode as u64);
                        uart_write_string("\r\n");
                    }
                }
            }
        }
    }
}

/// Simple hex printing utility
fn print_hex(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    let mut buffer = [0u8; 16];
    let mut i = 0;
    let mut num = n;
    
    if num == 0 {
        uart_write_string("0");
        return;
    }
    
    while num > 0 {
        buffer[i] = hex_chars[(num % 16) as usize];
        num /= 16;
        i += 1;
    }
    
    // Reverse and print
    let mut result = [0u8; 16];
    for j in 0..i {
        result[j] = buffer[i - 1 - j];
    }
    
    let s = core::str::from_utf8(&result[0..i]).unwrap_or("?");
    uart_write_string(s);
}