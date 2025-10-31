// USB HID (Human Interface Device) Driver - Keyboard and Mouse Support

extern crate alloc;
use alloc::vec::Vec;
use alloc::collections::VecDeque;
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

// Keyboard scan codes to ASCII mapping
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
pub fn test_input_events() {
    // Process all queued input events and update cursor
    while let Some(event) = get_input_event() {
        match event {
            InputEvent::KeyPressed { key, modifiers } => {
                uart_write_string("Key pressed: ");
                if let Some(ascii) = scancode_to_ascii(key, modifiers) {
                    unsafe {
                        core::ptr::write_volatile(0x09000000 as *mut u8, ascii);
                    }
                } else {
                    uart_write_string("(non-printable)");
                }
                uart_write_string("\r\n");
            }
            InputEvent::KeyReleased { key: _, modifiers: _ } => {
                // uart_write_string("Key released\r\n");
            }
            InputEvent::MouseMove { x_delta, y_delta } => {
                // Move the cursor on screen
                crate::kernel::framebuffer::move_cursor(x_delta, y_delta);

                uart_write_string("Mouse moved: ");
                // Simple hex output for deltas
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, (x_delta as u8));
                    core::ptr::write_volatile(0x09000000 as *mut u8, b',');
                    core::ptr::write_volatile(0x09000000 as *mut u8, (y_delta as u8));
                }
                uart_write_string("\r\n");
            }
            InputEvent::MouseButton { button, pressed } => {
                uart_write_string("Mouse button ");
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, button + b'0');
                }
                if pressed {
                    uart_write_string(" pressed\r\n");
                } else {
                    uart_write_string(" released\r\n");
                }
            }
            InputEvent::MouseWheel { delta } => {
                uart_write_string("Mouse wheel: ");
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, (delta as u8));
                }
                uart_write_string("\r\n");
            }
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