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
                // Check if we're in delete confirmation mode
                if is_confirming_delete() {
                    // Handle y/n input for delete confirmation
                    if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                        if ascii == b'y' || ascii == b'Y' {
                            // Confirm deletion
                            confirm_delete_file();
                            needs_full_redraw = true;
                        } else if ascii == b'n' || ascii == b'N' || ascii == 27 { // n, N, or ESC
                            // Cancel deletion
                            cancel_delete_confirm();
                            needs_full_redraw = true;
                        }
                    }
                } else if is_prompting_filename() {
                    // Handle filename input
                    if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                        if ascii == b'\n' {
                            // Enter pressed - check if renaming or creating new file
                            if is_renaming() {
                                finish_rename_prompt_for_file_explorer();
                            } else {
                                finish_filename_prompt_for_file_explorer();
                            }
                            needs_full_redraw = true;
                        } else if ascii == 27 { // ESC
                            // Cancel the prompt (clears both rename and new file states)
                            cancel_filename_prompt();
                            unsafe { RENAME_OLD_FILENAME = None; }
                            needs_full_redraw = true;
                        } else if ascii == 8 { // Backspace
                            backspace_filename_prompt();
                            needs_full_redraw = true;
                        } else if ascii >= 32 && ascii < 127 { // Printable ASCII
                            add_to_filename_prompt(ascii as char);
                            needs_full_redraw = true;
                        }
                    }
                } else if let Some(editor_id) = crate::kernel::window_manager::get_focused_editor_id() {
                    // Check if we're prompting for a filename (old editor-specific code)
                    if false { // This branch is now dead code since we handle prompts globally above
                        // Handle filename input
                        if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                            if ascii == b'\n' {
                                // Enter pressed - finish the prompt and save
                                if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                    finish_filename_prompt(editor);
                                }
                                needs_full_redraw = true;
                            } else if ascii == 27 { // ESC
                                // Cancel the prompt
                                cancel_filename_prompt();
                                if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                    editor.set_status("Save cancelled");
                                }
                                needs_full_redraw = true;
                            } else if ascii == 8 { // Backspace
                                backspace_filename_prompt();
                                needs_full_redraw = true;
                            } else if ascii >= 32 && ascii < 127 { // Printable ASCII
                                add_to_filename_prompt(ascii as char);
                                needs_full_redraw = true;
                            }
                        }
                    } else {
                        // Normal editor input handling
                        // Check for Ctrl+S (save)
                        let is_ctrl = (modifiers & (MOD_LEFT_CTRL | MOD_RIGHT_CTRL)) != 0;

                        if is_ctrl && key == 30 { // KEY_A = 30 in evdev (Ctrl+A)
                            // Handle select all
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.select_all();
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 31 { // KEY_S = 31 in evdev (Ctrl+S)
                            // Handle save in editor
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                save_editor_file(editor);
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 46 { // KEY_C = 46 in evdev (Ctrl+C)
                            // Handle copy
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.copy();
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 45 { // KEY_X = 45 in evdev (Ctrl+X)
                            // Handle cut
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.cut();
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 47 { // KEY_V = 47 in evdev (Ctrl+V)
                            // Handle paste
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.paste();
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 44 { // KEY_Z = 44 in evdev (Ctrl+Z)
                            // Handle undo
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.undo();
                            }
                            needs_full_redraw = true;
                        } else if is_ctrl && key == 21 { // KEY_Y = 21 in evdev (Ctrl+Y)
                            // Handle redo
                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                editor.redo();
                            }
                            needs_full_redraw = true;
                        } else {
                            // Check for shift modifier
                            let is_shift = (modifiers & (MOD_LEFT_SHIFT | MOD_RIGHT_SHIFT)) != 0;

                            // Arrow keys for editor navigation (Linux evdev codes)
                            match key {
                                103 => { // KEY_UP
                                    if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                        if is_shift {
                                            editor.move_up_select();
                                        } else {
                                            editor.move_up();
                                        }
                                    }
                                    needs_full_redraw = true;
                                }
                                108 => { // KEY_DOWN
                                    if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                        if is_shift {
                                            editor.move_down_select();
                                        } else {
                                            editor.move_down();
                                        }
                                    }
                                    needs_full_redraw = true;
                                }
                                105 => { // KEY_LEFT
                                    if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                        if is_shift {
                                            editor.move_left_select();
                                        } else {
                                            editor.move_left();
                                        }
                                    }
                                    needs_full_redraw = true;
                                }
                                106 => { // KEY_RIGHT
                                    if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                        if is_shift {
                                            editor.move_right_select();
                                        } else {
                                            editor.move_right();
                                        }
                                    }
                                    needs_full_redraw = true;
                                }
                                _ => {
                                    // Regular text input (but not if Ctrl is held)
                                    if !is_ctrl {
                                        if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                                            if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                                                if ascii == b'\n' {
                                                    // If there's a status message, clear it and consume the Enter
                                                    if get_menu_status().is_some() {
                                                        clear_menu_status();
                                                    } else {
                                                        // No status message, insert newline normally
                                                        editor.insert_newline();
                                                    }
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
                        }
                    }
                } else if let Some(terminal_id) = crate::kernel::window_manager::get_focused_terminal_id() {
                    // VirtIO keyboard uses Linux evdev codes
                    if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                        // Pass input to the focused terminal's shell
                        if let Some(shell) = crate::kernel::shell::get_shell(terminal_id) {
                            shell.handle_char(ascii);
                        }
                        needs_full_redraw = true; // Keyboard input requires full redraw
                    }
                } else if let Some(explorer_id) = crate::kernel::window_manager::get_focused_file_explorer_id() {
                    // File explorer keyboard navigation
                    match key {
                        103 => { // KEY_UP
                            crate::kernel::file_explorer::move_selection_up(explorer_id);
                            needs_full_redraw = true;
                        }
                        108 => { // KEY_DOWN
                            crate::kernel::file_explorer::move_selection_down(explorer_id);
                            needs_full_redraw = true;
                        }
                        28 => { // KEY_ENTER
                            use crate::kernel::file_explorer::FileExplorerAction;
                            let action = crate::kernel::file_explorer::open_selected(explorer_id);

                            match action {
                                FileExplorerAction::OpenFile(filename) => {
                                    // Open file in a new editor window
                                    if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(explorer_id) {
                                        if let (Some(ref fs), Some(device_idx)) = (&explorer.filesystem, explorer.device_index) {
                                            // Get file info by listing all files
                                            let file_list = fs.list_files();
                                            let file_entry = file_list.iter().find(|e| e.get_name() == filename);

                                            if let Some(file) = file_entry {
                                                let size = file.get_size_bytes() as usize;
                                                let mut buffer = alloc::vec![0u8; size];

                                                unsafe {
                                                    if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                                                        if let Some(device) = devices.get_mut(device_idx) {
                                                            if let Ok(bytes_read) = fs.read_file(device, &filename, &mut buffer) {
                                                                // Find the actual content length
                                                                let actual_len = buffer[..bytes_read].iter()
                                                                    .position(|&b| b == 0)
                                                                    .unwrap_or(bytes_read);

                                                                if let Ok(text) = core::str::from_utf8(&buffer[..actual_len]) {
                                                                    let editor_id = crate::kernel::editor::create_editor_with_content(
                                                                        &filename,
                                                                        text
                                                                    );
                                                                    let title = alloc::format!("Editor - {}", filename);
                                                                    let window = crate::kernel::window_manager::Window::new(
                                                                        0, 0, 640, 480, &title,
                                                                        crate::kernel::window_manager::WindowContent::Editor,
                                                                        editor_id
                                                                    );
                                                                    crate::kernel::window_manager::add_window(window);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                            needs_full_redraw = true;
                        }
                        _ => {}
                    }
                } else if let Some(snake_id) = crate::kernel::window_manager::get_focused_snake_id() {
                    // Snake game keyboard controls
                    match key {
                        103 => { // KEY_UP
                            if let Some(game) = crate::kernel::snake::get_snake_game(snake_id) {
                                game.set_direction(crate::kernel::snake::Direction::Up);
                            }
                            needs_full_redraw = true;
                        }
                        108 => { // KEY_DOWN
                            if let Some(game) = crate::kernel::snake::get_snake_game(snake_id) {
                                game.set_direction(crate::kernel::snake::Direction::Down);
                            }
                            needs_full_redraw = true;
                        }
                        105 => { // KEY_LEFT
                            if let Some(game) = crate::kernel::snake::get_snake_game(snake_id) {
                                game.set_direction(crate::kernel::snake::Direction::Left);
                            }
                            needs_full_redraw = true;
                        }
                        106 => { // KEY_RIGHT
                            if let Some(game) = crate::kernel::snake::get_snake_game(snake_id) {
                                game.set_direction(crate::kernel::snake::Direction::Right);
                            }
                            needs_full_redraw = true;
                        }
                        19 => { // KEY_R = 19 in evdev (R key to restart)
                            if let Some(game) = crate::kernel::snake::get_snake_game(snake_id) {
                                game.reset();
                            }
                            needs_full_redraw = true;
                        }
                        _ => {}
                    }
                } else if let Some(browser_id) = crate::kernel::window_manager::get_focused_browser_id() {
                    // Browser keyboard navigation
                    let is_shift = (modifiers & (MOD_LEFT_SHIFT | MOD_RIGHT_SHIFT)) != 0;

                    // Handle arrow keys for URL input
                    match key {
                        105 => { // KEY_LEFT
                            crate::kernel::browser::handle_arrow_key(
                                browser_id,
                                crate::kernel::text_input::ArrowKey::Left,
                                is_shift
                            );
                            needs_full_redraw = true;
                        }
                        106 => { // KEY_RIGHT
                            crate::kernel::browser::handle_arrow_key(
                                browser_id,
                                crate::kernel::text_input::ArrowKey::Right,
                                is_shift
                            );
                            needs_full_redraw = true;
                        }
                        102 => { // KEY_HOME
                            crate::kernel::browser::handle_arrow_key(
                                browser_id,
                                crate::kernel::text_input::ArrowKey::Home,
                                is_shift
                            );
                            needs_full_redraw = true;
                        }
                        107 => { // KEY_END
                            crate::kernel::browser::handle_arrow_key(
                                browser_id,
                                crate::kernel::text_input::ArrowKey::End,
                                is_shift
                            );
                            needs_full_redraw = true;
                        }
                        _ => {
                            // Regular keyboard input
                            if let Some(ascii) = evdev_to_ascii(key, modifiers) {
                                let is_ctrl = (modifiers & (MOD_LEFT_CTRL | MOD_RIGHT_CTRL)) != 0;
                                crate::kernel::browser::handle_key(browser_id, ascii as char, is_ctrl, is_shift);
                                needs_full_redraw = true;
                            }
                        }
                    }
                }
            }
            InputEvent::KeyReleased { key: _, modifiers: _ } => {
                // uart_write_string("Key released\r\n");
            }
            InputEvent::MouseMove { x_delta, y_delta } => {
                // Move the cursor on screen
                crate::kernel::framebuffer::move_cursor(x_delta, y_delta);

                // Check which menu button is hovered (if any)
                let (cx, cy) = crate::kernel::framebuffer::get_cursor_pos();
                let hovered_button = if cy >= 0 && cy < 32 {
                    // In menu bar, determine which button
                    crate::kernel::window_manager::get_hovered_menu_button(cx, cy)
                } else {
                    None
                };

                unsafe {
                    // Only trigger full redraw if hover state changed
                    if hovered_button != LAST_HOVERED_BUTTON {
                        needs_full_redraw = true;
                        LAST_HOVERED_BUTTON = hovered_button;
                    } else {
                        needs_cursor_redraw = true;
                    }
                }

                // If left mouse button is down, handle drag
                if is_mouse_button_down() {
                    let changed = crate::kernel::window_manager::handle_mouse_drag(cx, cy);
                    if changed {
                        needs_full_redraw = true;
                    } else {
                        needs_cursor_redraw = true;
                    }
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                if button == 0 { // Left mouse button
                    let (cx, cy) = crate::kernel::framebuffer::get_cursor_pos();
                    if pressed {
                        set_mouse_button_down(true);
                        crate::kernel::window_manager::handle_mouse_down(cx, cy);
                    } else {
                        set_mouse_button_down(false);
                        crate::kernel::window_manager::handle_mouse_up(cx, cy);
                    }
                    needs_full_redraw = true; // Clicks trigger a full redraw
                }
            }
            InputEvent::MouseWheel { delta } => {
                // Handle mouse wheel scrolling in focused editor
                if let Some(editor_id) = crate::kernel::window_manager::get_focused_editor_id() {
                    if let Some(editor) = crate::kernel::editor::get_editor(editor_id) {
                        // Negative delta = scroll up, positive = scroll down
                        // Multiply by 3 for smoother scrolling
                        editor.scroll(-delta as i32 * 3);
                    }
                    needs_full_redraw = true;
                }
            }
        }
    }

    // Check if status message should be auto-hidden after 3 seconds
    if check_status_timeout() {
        needs_full_redraw = true; // Redraw to remove the status message
    }

    (needs_full_redraw, needs_cursor_redraw)
}

/// Prompt state for filename input
static mut FILENAME_PROMPT: Option<String> = None;

/// Rename mode state (stores old filename when renaming)
static mut RENAME_OLD_FILENAME: Option<String> = None;

/// Delete confirmation state (stores filename to delete)
static mut DELETE_CONFIRM_FILENAME: Option<String> = None;

/// Status message to show in menu bar
static mut MENU_STATUS_MESSAGE: Option<String> = None;

/// Timestamp when status message was set (milliseconds)
static mut MENU_STATUS_TIMESTAMP: u64 = 0;

/// Track last hovered menu button index (for hover optimization)
static mut LAST_HOVERED_BUTTON: Option<usize> = None;

/// Track left mouse button state (for drag operations)
static mut MOUSE_BUTTON_DOWN: bool = false;

/// Check if left mouse button is currently down
pub fn is_mouse_button_down() -> bool {
    unsafe { MOUSE_BUTTON_DOWN }
}

/// Set left mouse button state
fn set_mouse_button_down(down: bool) {
    unsafe { MOUSE_BUTTON_DOWN = down; }
}

/// Check if we're currently prompting for a filename
pub fn is_prompting_filename() -> bool {
    unsafe { FILENAME_PROMPT.is_some() }
}

/// Set a status message to display in the menu bar
pub fn set_menu_status(msg: &str) {
    unsafe {
        MENU_STATUS_MESSAGE = Some(String::from(msg));
        MENU_STATUS_TIMESTAMP = crate::kernel::get_time_ms();
    }
}

/// Clear the menu status message
pub fn clear_menu_status() {
    unsafe {
        MENU_STATUS_MESSAGE = None;
    }
}

/// Get the current menu status message
pub fn get_menu_status() -> Option<String> {
    unsafe {
        MENU_STATUS_MESSAGE.clone()
    }
}

/// Check if status message should be auto-hidden (after 3 seconds)
pub fn check_status_timeout() -> bool {
    unsafe {
        if MENU_STATUS_MESSAGE.is_some() {
            let current_time = crate::kernel::get_time_ms();
            let elapsed = current_time - MENU_STATUS_TIMESTAMP;
            if elapsed >= 3000 { // 3 seconds
                clear_menu_status();
                return true; // Status was cleared
            }
        }
        false
    }
}

/// Start prompting for a filename
pub fn start_filename_prompt() {
    unsafe {
        FILENAME_PROMPT = Some(String::new());
    }
}

/// Add a character to the filename prompt
pub fn add_to_filename_prompt(ch: char) {
    unsafe {
        if let Some(ref mut prompt) = FILENAME_PROMPT {
            prompt.push(ch);
        }
    }
}

/// Remove the last character from the filename prompt
pub fn backspace_filename_prompt() {
    unsafe {
        if let Some(ref mut prompt) = FILENAME_PROMPT {
            prompt.pop();
        }
    }
}

/// Get the current filename prompt text
pub fn get_filename_prompt() -> Option<String> {
    unsafe { FILENAME_PROMPT.clone() }
}

/// Finish the filename prompt and save the file
pub fn finish_filename_prompt(editor: &mut crate::kernel::editor::TextEditor) {
    unsafe {
        if let Some(filename) = FILENAME_PROMPT.take() {
            if !filename.is_empty() {
                editor.set_filename(&filename);
                save_editor_file_internal(editor);
            } else {
                editor.set_status("Save cancelled - no filename provided");
            }
        }
    }
}

/// Cancel the filename prompt
pub fn cancel_filename_prompt() {
    unsafe {
        FILENAME_PROMPT = None;
    }
}

/// Finish the filename prompt and create a new file in file explorer
pub fn finish_filename_prompt_for_file_explorer() {
    unsafe {
        if let Some(filename) = FILENAME_PROMPT.take() {
            if !filename.is_empty() {
                // Get the focused file explorer
                if let Some(explorer_id) = crate::kernel::window_manager::get_focused_file_explorer_id() {
                    if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(explorer_id) {
                        if let (Some(ref mut fs), Some(device_idx)) = (&mut explorer.filesystem, explorer.device_index) {
                            if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                                if let Some(device) = devices.get_mut(device_idx) {
                                    // Create the file (1KB default size)
                                    match fs.create_file(device, &filename, 1024) {
                                        Ok(()) => {
                                            // Write some initial content
                                            let initial_content = b"";
                                            let _ = fs.write_file(device, &filename, initial_content);

                                            // Refresh the file list
                                            crate::kernel::file_explorer::refresh(explorer_id);
                                        }
                                        Err(_e) => {
                                            // File creation failed (maybe duplicate name)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Start prompting for a new filename when renaming
pub fn start_rename_prompt(old_filename: &str) {
    unsafe {
        FILENAME_PROMPT = Some(String::new());
        RENAME_OLD_FILENAME = Some(String::from(old_filename));
    }
}

/// Check if we're in rename mode
pub fn is_renaming() -> bool {
    unsafe { RENAME_OLD_FILENAME.is_some() }
}

/// Finish the rename prompt and rename the file in file explorer
pub fn finish_rename_prompt_for_file_explorer() {
    unsafe {
        if let Some(new_filename) = FILENAME_PROMPT.take() {
            if let Some(old_filename) = RENAME_OLD_FILENAME.take() {
                if !new_filename.is_empty() {
                    // Get the focused file explorer
                    if let Some(explorer_id) = crate::kernel::window_manager::get_focused_file_explorer_id() {
                        if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(explorer_id) {
                            if let (Some(ref mut fs), Some(device_idx)) = (&mut explorer.filesystem, explorer.device_index) {
                                if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                                    if let Some(device) = devices.get_mut(device_idx) {
                                        // Rename the file
                                        match fs.rename_file(device, &old_filename, &new_filename) {
                                            Ok(()) => {
                                                // Refresh the file list and re-select the renamed file
                                                crate::kernel::file_explorer::refresh(explorer_id);
                                                crate::kernel::file_explorer::select_file_by_name(explorer_id, &new_filename);
                                            }
                                            Err(_e) => {
                                                // Rename failed
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Make sure to clear both states
        RENAME_OLD_FILENAME = None;
    }
}

/// Start prompting for delete confirmation
pub fn start_delete_confirm(filename: &str) {
    unsafe {
        DELETE_CONFIRM_FILENAME = Some(String::from(filename));
    }
}

/// Check if we're in delete confirmation mode
pub fn is_confirming_delete() -> bool {
    unsafe { DELETE_CONFIRM_FILENAME.is_some() }
}

/// Get the filename being confirmed for deletion
pub fn get_delete_confirm_filename() -> Option<String> {
    unsafe { DELETE_CONFIRM_FILENAME.clone() }
}

/// Cancel delete confirmation
pub fn cancel_delete_confirm() {
    unsafe {
        DELETE_CONFIRM_FILENAME = None;
    }
}

/// Confirm deletion and delete the file
pub fn confirm_delete_file() {
    unsafe {
        if let Some(filename) = DELETE_CONFIRM_FILENAME.take() {
            // Get the focused file explorer
            if let Some(explorer_id) = crate::kernel::window_manager::get_focused_file_explorer_id() {
                if crate::kernel::file_explorer::delete_selected(explorer_id) {
                    crate::kernel::file_explorer::refresh(explorer_id);
                }
            }
        }
    }
}

/// Save the editor file to disk
fn save_editor_file(editor: &mut crate::kernel::editor::TextEditor) {
    use crate::kernel::filesystem;

    // Check if we have a filename
    if let Some(_name) = editor.get_filename() {
        // File already has a name, save directly
        save_editor_file_internal(editor);
    } else {
        // Prompt for filename in the status bar
        start_filename_prompt();
    }
}

/// Internal function to save the editor file with a known filename
fn save_editor_file_internal(editor: &mut crate::kernel::editor::TextEditor) {
    use crate::kernel::filesystem;

    // Get the filename
    let filename = if let Some(name) = editor.get_filename() {
        name.to_string()
    } else {
        editor.set_status("Error: No filename set");
        return;
    };

    // Get file content
    let content = editor.get_text();
    let content_bytes = content.as_bytes();

    // Try to access filesystem through shell first, then file explorer as fallback
    let fs_access = if let Some(shell) = crate::kernel::shell::get_shell(0) {
        shell.filesystem.as_mut().zip(shell.device_index)
    } else {
        // No shell, try file explorer
        if let Some(explorer_id) = crate::kernel::window_manager::get_focused_file_explorer_id() {
            if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(explorer_id) {
                explorer.filesystem.as_mut().zip(explorer.device_index)
            } else {
                None
            }
        } else {
            // Try any file explorer
            let explorers = crate::kernel::file_explorer::get_all_file_explorers();
            if !explorers.is_empty() {
                if let Some(explorer) = crate::kernel::file_explorer::get_file_explorer(explorers[0]) {
                    explorer.filesystem.as_mut().zip(explorer.device_index)
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    if let Some((fs, device_idx)) = fs_access {
                unsafe {
                    if let Some(ref mut devices) = crate::kernel::BLOCK_DEVICES {
                        if let Some(device) = devices.get_mut(device_idx) {
                            // Check if file exists and get its size
                            let files = fs.list_files();
                            let existing_file = files.iter().find(|f| f.get_name() == filename);

                            let required_size = ((content_bytes.len() + 511) / 512) * 512; // Round up to sector

                            // If file doesn't exist or is too small, (re)create it
                            if let Some(file) = existing_file {
                                let current_size = file.get_size_bytes() as usize;
                                if content_bytes.len() > current_size {
                                    // File exists but is too small, delete and recreate
                                    match fs.delete_file(device, &filename) {
                                        Ok(()) => {
                                            uart_write_string(&alloc::format!("Resizing '{}' from {} to {} bytes\r\n",
                                                filename, current_size, required_size));
                                        }
                                        Err(e) => {
                                            let msg = alloc::format!("Error deleting file for resize: {}", e);
                                            set_menu_status(&msg);
                                            uart_write_string(&alloc::format!("{}\r\n", msg));
                                            return;
                                        }
                                    }
                                    // Create new larger file
                                    match fs.create_file(device, &filename, required_size as u32) {
                                        Ok(()) => {
                                            uart_write_string(&alloc::format!("Created larger file '{}'\r\n", filename));
                                        }
                                        Err(e) => {
                                            let msg = alloc::format!("Error creating resized file: {}", e);
                                            set_menu_status(&msg);
                                            uart_write_string(&alloc::format!("{}\r\n", msg));
                                            return;
                                        }
                                    }
                                }
                            } else {
                                // File doesn't exist, create it
                                match fs.create_file(device, &filename, required_size as u32) {
                                    Ok(()) => {
                                        uart_write_string(&alloc::format!("Created file '{}'\r\n", filename));
                                    }
                                    Err(e) => {
                                        let msg = alloc::format!("Error creating file: {}", e);
                                        set_menu_status(&msg);
                                        uart_write_string(&alloc::format!("{}\r\n", msg));
                                        return;
                                    }
                                }
                            }

                            // Write content to file
                            match fs.write_file(device, &filename, content_bytes) {
                                Ok(()) => {
                                    editor.mark_saved();
                                    let msg = alloc::format!("Saved {} bytes to '{}'", content_bytes.len(), filename);
                                    set_menu_status(&msg);
                                    uart_write_string(&alloc::format!("{}\r\n", msg));

                                    // Update editor window title to show filename
                                    let window_title = alloc::format!("Text Editor - {}", filename);
                                    crate::kernel::window_manager::set_editor_window_title(&window_title);
                                }
                                Err(e) => {
                                    let msg = alloc::format!("Error saving: {}", e);
                                    set_menu_status(&msg);
                                    uart_write_string(&alloc::format!("{}\r\n", msg));
                                }
                            }
                        } else {
                            editor.set_status("Block device not available");
                        }
                    }
                }
    } else {
        editor.set_status("Filesystem not available - open a terminal or file explorer first");
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