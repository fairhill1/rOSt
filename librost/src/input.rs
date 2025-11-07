//! Input event utilities for userspace applications
//!
//! Provides evdev keycode to ASCII conversion for keyboard input.

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

// Modifier key bits
const MOD_LEFT_SHIFT: u8 = 1 << 1;
const MOD_RIGHT_SHIFT: u8 = 1 << 5;

/// Convert evdev keycode to ASCII character
///
/// Takes a raw evdev keycode and modifier bits, returns the ASCII character.
/// Returns None if the key doesn't map to a printable ASCII character.
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
