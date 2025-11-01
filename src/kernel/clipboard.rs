// Shared clipboard for the entire OS

extern crate alloc;
use alloc::string::String;

/// Global clipboard (shared across all apps: editor, text inputs, etc.)
static mut CLIPBOARD: Option<String> = None;

/// Copy text to clipboard
pub fn copy(text: String) {
    unsafe {
        CLIPBOARD = Some(text);
    }
}

/// Get clipboard contents (returns None if clipboard is empty)
pub fn paste() -> Option<String> {
    unsafe {
        CLIPBOARD.clone()
    }
}

/// Clear clipboard
pub fn clear() {
    unsafe {
        CLIPBOARD = None;
    }
}

/// Check if clipboard has content
pub fn has_content() -> bool {
    unsafe {
        CLIPBOARD.is_some()
    }
}
