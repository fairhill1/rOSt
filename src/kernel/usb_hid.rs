// USB HID (Human Interface Device) Driver - Keyboard and Mouse Support

extern crate alloc;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use crate::kernel::uart_write_string;

// USB HID Class Codes
const USB_CLASS_HID: u8 = 0x03;
const USB_SUBCLASS_BOOT: u8 = 0x01;
const USB_PROTOCOL_KEYBOARD: u8 = 0x01;
const USB_PROTOCOL_MOUSE: u8 = 0x02;

// HID Report Descriptor Types
const HID_REPORT_INPUT: u8 = 0x01;
const HID_REPORT_OUTPUT: u8 = 0x02;
const HID_REPORT_FEATURE: u8 = 0x03;

// USB HID Request Types
const HID_GET_REPORT: u8 = 0x01;
const HID_GET_IDLE: u8 = 0x02;
const HID_GET_PROTOCOL: u8 = 0x03;
const HID_SET_REPORT: u8 = 0x09;
const HID_SET_IDLE: u8 = 0x0A;
const HID_SET_PROTOCOL: u8 = 0x0B;

// Linux evdev key codes to ASCII mapping (for VirtIO keyboard)
// Based on linux/input-event-codes.h
const EVDEV_TO_ASCII: [u8; 256] = [
    // 0-9
    0, 0, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8',
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

// Keyboard input state
#[derive(Clone, Copy, Debug)]
pub struct KeyboardState {
    pub modifiers: u8,
    pub keys: [u8; 6],  // Up to 6 simultaneous key presses
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self {
            modifiers: 0,
            keys: [0; 6],
        }
    }
}

// Mouse input state  
#[derive(Clone, Copy, Debug)]
pub struct MouseState {
    pub buttons: u8,    // Button states (bit 0=left, 1=right, 2=middle)
    pub x_delta: i8,    // X movement delta
    pub y_delta: i8,    // Y movement delta
    pub wheel_delta: i8, // Scroll wheel delta
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            buttons: 0,
            x_delta: 0,
            y_delta: 0, 
            wheel_delta: 0,
        }
    }
}

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

// Global shell instance
pub static mut SHELL: Option<crate::kernel::shell::Shell> = None;

// USB HID Device representation
pub struct UsbHidDevice {
    device_type: HidDeviceType,
    endpoint_addr: u8,
    last_keyboard_state: KeyboardState,
    last_mouse_state: MouseState,
}

#[derive(Clone, Copy, Debug)]
pub enum HidDeviceType {
    Keyboard,
    Mouse,
    Unknown,
}

impl UsbHidDevice {
    pub fn new(device_type: HidDeviceType, endpoint_addr: u8) -> Self {
        Self {
            device_type,
            endpoint_addr,
            last_keyboard_state: KeyboardState::default(),
            last_mouse_state: MouseState::default(),
        }
    }
    
    /// Process raw HID input data and return input events
    pub fn process_input_data(&mut self, data: &[u8]) -> Option<InputEvent> {
        match self.device_type {
            HidDeviceType::Keyboard => self.process_keyboard_data(data),
            HidDeviceType::Mouse => self.process_mouse_data(data),
            HidDeviceType::Unknown => None,
        }
    }
    
    /// Process keyboard HID report (8 bytes standard)
    fn process_keyboard_data(&mut self, data: &[u8]) -> Option<InputEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let new_state = KeyboardState {
            modifiers: data[0],
            keys: [data[2], data[3], data[4], data[5], data[6], data[7]],
        };
        
        // Check for new key presses
        for &key in &new_state.keys {
            if key != 0 && !self.last_keyboard_state.keys.contains(&key) {
                self.last_keyboard_state = new_state;
                return Some(InputEvent::KeyPressed { 
                    key, 
                    modifiers: new_state.modifiers 
                });
            }
        }
        
        // Check for key releases
        for &key in &self.last_keyboard_state.keys {
            if key != 0 && !new_state.keys.contains(&key) {
                self.last_keyboard_state = new_state;
                return Some(InputEvent::KeyReleased { 
                    key, 
                    modifiers: new_state.modifiers 
                });
            }
        }
        
        self.last_keyboard_state = new_state;
        None
    }
    
    /// Process mouse HID report (typically 3-4 bytes)
    fn process_mouse_data(&mut self, data: &[u8]) -> Option<InputEvent> {
        if data.len() < 3 {
            return None;
        }
        
        let new_state = MouseState {
            buttons: data[0],
            x_delta: data[1] as i8,
            y_delta: data[2] as i8,
            wheel_delta: if data.len() > 3 { data[3] as i8 } else { 0 },
        };
        
        // Check for mouse movement
        if new_state.x_delta != 0 || new_state.y_delta != 0 {
            self.last_mouse_state = new_state;
            return Some(InputEvent::MouseMove {
                x_delta: new_state.x_delta,
                y_delta: new_state.y_delta,
            });
        }
        
        // Check for button changes
        let button_changes = self.last_mouse_state.buttons ^ new_state.buttons;
        if button_changes != 0 {
            for button in 0..8 {
                let button_mask = 1 << button;
                if (button_changes & button_mask) != 0 {
                    let pressed = (new_state.buttons & button_mask) != 0;
                    self.last_mouse_state = new_state;
                    return Some(InputEvent::MouseButton { button, pressed });
                }
            }
        }
        
        // Check for wheel movement
        if new_state.wheel_delta != 0 {
            self.last_mouse_state = new_state;
            return Some(InputEvent::MouseWheel { delta: new_state.wheel_delta });
        }
        
        self.last_mouse_state = new_state;
        None
    }
}

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

/// Simulate keyboard input for testing purposes
pub fn simulate_keyboard_input() {
    uart_write_string("Simulating keyboard input events...\r\n");
    
    // Simulate pressing 'H', 'E', 'L', 'L', 'O'
    let hello_keys = [0x0B, 0x08, 0x0F, 0x0F, 0x12]; // H, E, L, L, O scan codes
    
    for &key in &hello_keys {
        queue_input_event(InputEvent::KeyPressed { key, modifiers: 0 });
        queue_input_event(InputEvent::KeyReleased { key, modifiers: 0 });
    }
    
    uart_write_string("Simulated keyboard events queued\r\n");
}

/// Process input events from the queue and update GUI
/// Returns (needs_full_redraw, needs_cursor_redraw)
pub fn test_input_events() -> (bool, bool) {
    let mut needs_full_redraw = false;
    let mut needs_cursor_redraw = false;

    // Process all queued input events and update cursor
    while let Some(event) = get_input_event() {
        match event {
            InputEvent::KeyPressed { key, modifiers } => {
                // Check if editor window is focused
                if crate::kernel::window_manager::has_focused_editor() {
                    // Check for Ctrl+S (save)
                    let is_ctrl = (modifiers & (MOD_LEFT_CTRL | MOD_RIGHT_CTRL)) != 0;

                    if is_ctrl && key == 31 { // KEY_S = 31 in evdev
                        // Handle save in editor
                        if let Some(editor) = crate::kernel::editor::get_editor() {
                            save_editor_file(editor);
                        }
                        needs_full_redraw = true;
                    } else {
                        // Arrow keys for editor navigation (Linux evdev codes)
                        match key {
                            103 => { // KEY_UP
                                if let Some(editor) = crate::kernel::editor::get_editor() {
                                    editor.move_up();
                                }
                                needs_full_redraw = true;
                            }
                            108 => { // KEY_DOWN
                                if let Some(editor) = crate::kernel::editor::get_editor() {
                                    editor.move_down();
                                }
                                needs_full_redraw = true;
                            }
                            105 => { // KEY_LEFT
                                if let Some(editor) = crate::kernel::editor::get_editor() {
                                    editor.move_left();
                                }
                                needs_full_redraw = true;
                            }
                            106 => { // KEY_RIGHT
                                if let Some(editor) = crate::kernel::editor::get_editor() {
                                    editor.move_right();
                                }
                                needs_full_redraw = true;
                            }
                            _ => {
                                // Regular text input
                                if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                                    if let Some(editor) = crate::kernel::editor::get_editor() {
                                        if ascii == b'\n' {
                                            editor.insert_newline();
                                        } else if ascii == 8 { // Backspace
                                            editor.delete_char();
                                        } else if ascii >= 32 && ascii < 127 { // Printable ASCII
                                            editor.insert_char(ascii as char);
                                        }
                                    }
                                    needs_full_redraw = true;
                                }
                            }
                        }
                    }
                } else if crate::kernel::window_manager::has_focused_terminal() {
                    // VirtIO keyboard uses Linux evdev codes
                    if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                        // Only pass to shell if terminal window is focused
                        unsafe {
                            if let Some(ref mut shell) = SHELL {
                                shell.handle_char(ascii);
                            }
                        }
                        needs_full_redraw = true; // Keyboard input requires full redraw
                    }
                }
            }
            InputEvent::KeyReleased { key: _, modifiers: _ } => {
                // uart_write_string("Key released\r\n");
            }
            InputEvent::MouseMove { x_delta, y_delta } => {
                // Move the cursor on screen
                crate::kernel::framebuffer::move_cursor(x_delta, y_delta);
                // Just update cursor, no window dragging in tiling mode
                needs_cursor_redraw = true;
            }
            InputEvent::MouseButton { button, pressed } => {
                if button == 0 && pressed { // Left mouse button press
                    let (cx, cy) = crate::kernel::framebuffer::get_cursor_pos();
                    // Check for window clicks (menu bar, close button, focus)
                    crate::kernel::window_manager::handle_mouse_click(cx, cy);
                    needs_full_redraw = true; // Clicks trigger a full redraw
                }
            }
            InputEvent::MouseWheel { .. } => {
            }
        }
    }
    (needs_full_redraw, needs_cursor_redraw)
}

/// Save the editor file to disk
fn save_editor_file(editor: &mut crate::kernel::editor::TextEditor) {
    use crate::kernel::filesystem;

    // Check if we have a filename
    let filename = if let Some(name) = editor.get_filename() {
        name.to_string()
    } else {
        // For now, use a default name - in future we could prompt the user
        editor.set_filename("untitled");
        editor.set_status("Saving as 'untitled'...");
        String::from("untitled")
    };

    // Get file content
    let content = editor.get_text();
    let content_bytes = content.as_bytes();

    // Access the filesystem through the shell's device
    unsafe {
        if let Some(ref mut shell) = SHELL {
            if let Some(ref mut fs) = shell.filesystem {
                if let Some(device_idx) = shell.device_index {
                    if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                        if let Some(device) = devices.get_mut(device_idx) {
                            // Check if file exists
                            let files = fs.list_files();
                            let file_exists = files.iter().any(|f| f.get_name() == filename);

                            if !file_exists {
                                // Create file with appropriate size
                                let size = ((content_bytes.len() + 511) / 512) * 512; // Round up to sector
                                match fs.create_file(device, &filename, size as u32) {
                                    Ok(()) => {
                                        uart_write_string(&alloc::format!("Created file '{}'\r\n", filename));
                                    }
                                    Err(e) => {
                                        editor.set_status(&alloc::format!("Error creating file: {}", e));
                                        uart_write_string(&alloc::format!("Error creating file: {}\r\n", e));
                                        return;
                                    }
                                }
                            }

                            // Write content to file
                            match fs.write_file(device, &filename, content_bytes) {
                                Ok(()) => {
                                    editor.mark_saved();
                                    editor.set_status(&alloc::format!("Saved {} bytes to '{}'", content_bytes.len(), filename));
                                    uart_write_string(&alloc::format!("Saved {} bytes to '{}'\r\n", content_bytes.len(), filename));
                                }
                                Err(e) => {
                                    editor.set_status(&alloc::format!("Error saving: {}", e));
                                    uart_write_string(&alloc::format!("Error saving: {}\r\n", e));
                                }
                            }
                        } else {
                            editor.set_status("Block device not available");
                        }
                    } else {
                        editor.set_status("Block devices not initialized");
                    }
                } else {
                    editor.set_status("No device index");
                }
            } else {
                editor.set_status("Filesystem not mounted");
            }
        } else {
            editor.set_status("Shell not initialized");
        }
    }
}

/// Initialize USB HID subsystem
pub fn init_usb_hid() {
    uart_write_string("Initializing USB HID subsystem...\r\n");
    uart_write_string("Setting up XHCI interrupt handling...\r\n");

    // Initialize input event queue for XHCI events
    unsafe {
        INPUT_EVENT_QUEUE = Some(VecDeque::new());
    }

    uart_write_string("USB HID subsystem ready for XHCI keyboard events!\r\n");
}