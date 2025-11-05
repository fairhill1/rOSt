//! IPC protocol messages for kernel ↔ window manager communication
//!
//! These messages fit within the 256-byte message size limit.

#![allow(dead_code)]

use crate::runtime::InputEvent;

/// Maximum length for window titles in IPC messages
const MAX_TITLE_LEN: usize = 64;

/// Message from kernel to window manager
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum KernelToWM {
    /// Forward input event to WM for routing
    InputEvent {
        sender_pid: u32,  // PID to send responses to
        mouse_x: i32,
        mouse_y: i32,
        event: InputEvent,
    },

    /// Notify WM about new window
    CreateWindow {
        id: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        title: [u8; MAX_TITLE_LEN],
        title_len: usize,
    },

    /// Notify WM about closed window
    CloseWindow {
        id: usize,
    },

    /// Update window focus
    SetFocus {
        id: usize,
    },
}

/// Message from window manager to kernel
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum WMToKernel {
    /// Route input event to specific window
    RouteInput {
        window_id: usize,
        event: InputEvent,
    },

    /// Window manager requests focus change
    RequestFocus {
        window_id: usize,
    },

    /// Window manager requests window close
    RequestClose {
        window_id: usize,
    },

    /// No action needed
    NoAction,
}

impl KernelToWM {
    /// Serialize message to bytes (max 256 bytes)
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        match self {
            KernelToWM::InputEvent { sender_pid, mouse_x, mouse_y, event } => {
                buf[0] = 0; // Message type
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&mouse_x.to_le_bytes());
                buf[9..13].copy_from_slice(&mouse_y.to_le_bytes());
                buf[13..17].copy_from_slice(&event.event_type.to_le_bytes());
                buf[17] = event.key;
                buf[18] = event.modifiers;
                buf[19] = event.button;
                buf[20] = event.pressed;
                buf[21] = event.x_delta as u8;
                buf[22] = event.y_delta as u8;
                buf[23] = event.wheel_delta as u8;
            }
            KernelToWM::CreateWindow { id, x, y, width, height, title, title_len } => {
                buf[0] = 1; // Message type
                buf[1..9].copy_from_slice(&id.to_le_bytes());
                buf[9..13].copy_from_slice(&x.to_le_bytes());
                buf[13..17].copy_from_slice(&y.to_le_bytes());
                buf[17..21].copy_from_slice(&width.to_le_bytes());
                buf[21..25].copy_from_slice(&height.to_le_bytes());
                buf[25..33].copy_from_slice(&title_len.to_le_bytes());
                let len = core::cmp::min(*title_len, MAX_TITLE_LEN);
                buf[33..33+len].copy_from_slice(&title[..len]);
            }
            KernelToWM::CloseWindow { id } => {
                buf[0] = 2; // Message type
                buf[1..9].copy_from_slice(&id.to_le_bytes());
            }
            KernelToWM::SetFocus { id } => {
                buf[0] = 3; // Message type
                buf[1..9].copy_from_slice(&id.to_le_bytes());
            }
        }
        buf
    }

    /// Deserialize message from bytes
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        match buf[0] {
            0 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let mouse_x = i32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let mouse_y = i32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let event_type = u32::from_le_bytes([buf[13], buf[14], buf[15], buf[16]]);
                let event = InputEvent {
                    event_type,
                    key: buf[17],
                    modifiers: buf[18],
                    button: buf[19],
                    pressed: buf[20],
                    x_delta: buf[21] as i8,
                    y_delta: buf[22] as i8,
                    wheel_delta: buf[23] as i8,
                };
                Some(KernelToWM::InputEvent { sender_pid, mouse_x, mouse_y, event })
            }
            1 => {
                let id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                let x = i32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let y = i32::from_le_bytes([buf[13], buf[14], buf[15], buf[16]]);
                let width = u32::from_le_bytes([buf[17], buf[18], buf[19], buf[20]]);
                let height = u32::from_le_bytes([buf[21], buf[22], buf[23], buf[24]]);
                let title_len = usize::from_le_bytes([buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31], buf[32]]);
                let mut title = [0u8; MAX_TITLE_LEN];
                let len = core::cmp::min(title_len, MAX_TITLE_LEN);
                title[..len].copy_from_slice(&buf[33..33+len]);
                Some(KernelToWM::CreateWindow { id, x, y, width, height, title, title_len })
            }
            2 => {
                let id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                Some(KernelToWM::CloseWindow { id })
            }
            3 => {
                let id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                Some(KernelToWM::SetFocus { id })
            }
            _ => None,
        }
    }
}

impl WMToKernel {
    /// Serialize message to bytes (max 256 bytes)
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        match self {
            WMToKernel::RouteInput { window_id, event } => {
                buf[0] = 0; // Message type
                buf[1..9].copy_from_slice(&window_id.to_le_bytes());
                buf[9..13].copy_from_slice(&event.event_type.to_le_bytes());
                buf[13] = event.key;
                buf[14] = event.modifiers;
                buf[15] = event.button;
                buf[16] = event.pressed;
                buf[17] = event.x_delta as u8;
                buf[18] = event.y_delta as u8;
                buf[19] = event.wheel_delta as u8;
            }
            WMToKernel::RequestFocus { window_id } => {
                buf[0] = 1; // Message type
                buf[1..9].copy_from_slice(&window_id.to_le_bytes());
            }
            WMToKernel::RequestClose { window_id } => {
                buf[0] = 2; // Message type
                buf[1..9].copy_from_slice(&window_id.to_le_bytes());
            }
            WMToKernel::NoAction => {
                buf[0] = 3; // Message type
            }
        }
        buf
    }

    /// Deserialize message from bytes
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        match buf[0] {
            0 => {
                let window_id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                let event_type = u32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let event = InputEvent {
                    event_type,
                    key: buf[13],
                    modifiers: buf[14],
                    button: buf[15],
                    pressed: buf[16],
                    x_delta: buf[17] as i8,
                    y_delta: buf[18] as i8,
                    wheel_delta: buf[19] as i8,
                };
                Some(WMToKernel::RouteInput { window_id, event })
            }
            1 => {
                let window_id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                Some(WMToKernel::RequestFocus { window_id })
            }
            2 => {
                let window_id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                Some(WMToKernel::RequestClose { window_id })
            }
            3 => Some(WMToKernel::NoAction),
            _ => None,
        }
    }
}
