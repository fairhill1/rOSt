// USB HID (Human Interface Device) Driver - Keyboard and Mouse Support
// MICROKERNEL VERSION: Only event types, conversion tables, and event queue
// All GUI/window management logic moved to userspace

extern crate alloc;
use alloc::collections::VecDeque;

// Linux evdev key codes to ASCII mapping (for VirtIO keyboard)
// Based on linux/input-event-codes.h
const EVDEV_TO_ASCII: [u8; 256] = [
    // 0-9 (KEY_ESC = 1)
    0, 27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',
    // 10-19 (14 = KEY_BACKSPACE)
    b'9', b'0', b'-', b'=', 8, 0, b'q', b'w', b'e', b'r',
    // 20-29
    b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0,
    // 30-39 (KEY_A = 30)
    b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';',
    // 40-49
    b'\'', b'`', 0, b'\\', b'z', b'x', b'c', b'v', b'b', b'n',
    // 50-59
    b'm', b',', b'.', b'/', 0, 0, 0, b' ', 0, 0,
    // 60-255 (function keys, etc.)
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
    0, 0, 0, 0, // Add 4 more elements to make exactly 256
];

// USB HID scan codes to ASCII mapping (for USB keyboards)
const SCANCODE_TO_ASCII: [u8; 256] = [
    // 0x00-0x0F
    0, 0, 0, 0, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l',
    // 0x10-0x1F
    b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v', b'w', b'x', b'y', b'z', b'1', b'2',
    // 0x20-0x2F
    b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'\n', 0, 0, 0, b' ', b'-', b'=', b'[',
    // 0x30-0x3F
    b']', b'\\', 0, b';', b'\'', b'`', b',', b'.', b'/', 0, 0, 0, 0, 0, 0, 0,
    // 0x40-0xFF (mostly unused for basic keyboard)
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

// Modifier key bits
const MOD_LEFT_CTRL: u8 = 1 << 0;
const MOD_LEFT_SHIFT: u8 = 1 << 1;
const MOD_LEFT_ALT: u8 = 1 << 2;
const MOD_LEFT_GUI: u8 = 1 << 3;
const MOD_RIGHT_CTRL: u8 = 1 << 4;
const MOD_RIGHT_SHIFT: u8 = 1 << 5;
const MOD_RIGHT_ALT: u8 = 1 << 6;
const MOD_RIGHT_GUI: u8 = 1 << 7;

// Input event types
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    KeyPressed { key: u8, modifiers: u8 },
    KeyReleased { key: u8, modifiers: u8 },
    MouseMove { x_delta: i8, y_delta: i8 },
    MouseButton { button: u8, pressed: bool },
    MouseWheel { delta: i8 },
}

// Global input event queue for XHCI/USB HID events
static mut INPUT_EVENT_QUEUE: Option<VecDeque<InputEvent>> = None;

/// Add an input event to the global event queue
pub fn queue_input_event(event: InputEvent) {
    unsafe {
        if let Some(ref mut queue) = INPUT_EVENT_QUEUE {
            queue.push_back(event);
        }
    }
}

/// Get the next input event from the global event queue
pub fn get_input_event() -> Option<InputEvent> {
    unsafe {
        if let Some(ref mut queue) = INPUT_EVENT_QUEUE {
            queue.pop_front()
        } else {
            None
        }
    }
}

/// Convert scan code to ASCII character
pub fn scancode_to_ascii(scancode: u8, modifiers: u8) -> Option<u8> {
    if scancode == 0 || scancode as usize >= SCANCODE_TO_ASCII.len() {
        return None;
    }

    let mut ascii = SCANCODE_TO_ASCII[scancode as usize];
    if ascii == 0 {
        return None;
    }

    // Handle shift modifier for letters and symbols
    if (modifiers & (MOD_LEFT_SHIFT | MOD_RIGHT_SHIFT)) != 0 {
        if ascii >= b'a' && ascii <= b'z' {
            ascii = ascii - b'a' + b'A'; // Convert to uppercase
        } else {
            // Handle shifted symbols
            ascii = match ascii {
                b'1' => b'!',
                b'2' => b'@',
                b'3' => b'#',
                b'4' => b'$',
                b'5' => b'%',
                b'6' => b'^',
                b'7' => b'&',
                b'8' => b'*',
                b'9' => b'(',
                b'0' => b')',
                b'-' => b'_',
                b'=' => b'+',
                b'[' => b'{',
                b']' => b'}',
                b'\\' => b'|',
                b';' => b':',
                b'\'' => b'"',
                b'`' => b'~',
                b',' => b'<',
                b'.' => b'>',
                b'/' => b'?',
                _ => ascii,
            };
        }
    }

    Some(ascii)
}

/// Convert Linux evdev key code to ASCII (for VirtIO keyboard)
pub fn evdev_to_ascii(keycode: u8, modifiers: u8) -> Option<u8> {
    if keycode == 0 || keycode as usize >= EVDEV_TO_ASCII.len() {
        return None;
    }

    let mut ascii = EVDEV_TO_ASCII[keycode as usize];
    if ascii == 0 {
        return None;
    }

    // Handle shift modifier for letters and symbols
    if (modifiers & (MOD_LEFT_SHIFT | MOD_RIGHT_SHIFT)) != 0 {
        if ascii >= b'a' && ascii <= b'z' {
            ascii = ascii - b'a' + b'A'; // Convert to uppercase
        } else {
            // Handle shifted symbols (same as scancode_to_ascii)
            ascii = match ascii {
                b'1' => b'!',
                b'2' => b'@',
                b'3' => b'#',
                b'4' => b'$',
                b'5' => b'%',
                b'6' => b'^',
                b'7' => b'&',
                b'8' => b'*',
                b'9' => b'(',
                b'0' => b')',
                b'-' => b'_',
                b'=' => b'+',
                b'[' => b'{',
                b']' => b'}',
                b'\\' => b'|',
                b';' => b':',
                b'\'' => b'"',
                b'`' => b'~',
                b',' => b'<',
                b'.' => b'>',
                b'/' => b'?',
                _ => ascii,
            };
        }
    }

    Some(ascii)
}

/// Initialize USB HID subsystem
pub fn init_usb_hid() {
    crate::kernel::uart_write_string("Initializing USB HID subsystem...\r\n");

    // Initialize input event queue for XHCI events
    unsafe {
        INPUT_EVENT_QUEUE = Some(VecDeque::new());
    }

    crate::kernel::uart_write_string("USB HID subsystem ready for XHCI keyboard events!\r\n");
}
