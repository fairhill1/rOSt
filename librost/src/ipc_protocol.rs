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

    /// Request WM to create a new window (terminal → WM)
    /// WM will allocate buffer and respond with WindowCreated
    CreateWindow {
        id: usize,              // Window ID (use PID for uniqueness)
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

    /// Request WM to redraw window (terminal → WM)
    /// Sent after terminal updates content in shared buffer
    RequestRedraw {
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

    /// WM confirms window creation and provides buffer (WM → terminal)
    /// Terminal should map this shm_id to get access to framebuffer
    WindowCreated {
        window_id: usize,
        shm_id: i32,          // WM-owned shared memory ID
        width: u32,           // Actual assigned dimensions
        height: u32,
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
            KernelToWM::RequestRedraw { id } => {
                buf[0] = 4; // Message type
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
            4 => {
                let id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                Some(KernelToWM::RequestRedraw { id })
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
            WMToKernel::WindowCreated { window_id, shm_id, width, height } => {
                buf[0] = 3; // Message type
                buf[1..9].copy_from_slice(&window_id.to_le_bytes());
                buf[9..13].copy_from_slice(&shm_id.to_le_bytes());
                buf[13..17].copy_from_slice(&width.to_le_bytes());
                buf[17..21].copy_from_slice(&height.to_le_bytes());
            }
            WMToKernel::NoAction => {
                buf[0] = 4; // Message type
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
            3 => {
                let window_id = usize::from_le_bytes([buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8]]);
                let shm_id = i32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let width = u32::from_le_bytes([buf[13], buf[14], buf[15], buf[16]]);
                let height = u32::from_le_bytes([buf[17], buf[18], buf[19], buf[20]]);
                Some(WMToKernel::WindowCreated { window_id, shm_id, width, height })
            }
            4 => Some(WMToKernel::NoAction),
            _ => None,
        }
    }
}

// ============================================================================
// File Server IPC Protocol
// ============================================================================

/// Maximum filename length in filesystem messages
const MAX_FILENAME_LEN: usize = 128;

/// Maximum data payload per message (for read/write operations)
const MAX_DATA_LEN: usize = 200; // Leave room for headers in 256-byte message

/// Message from app to file server
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum AppToFS {
    /// Open a file and return file descriptor
    Open {
        sender_pid: u32,          // PID to send response to
        request_id: u32,          // Used to match response
        filename: [u8; MAX_FILENAME_LEN],
        filename_len: usize,
        flags: u32,               // Read/Write flags
    },

    /// Read from file descriptor
    Read {
        sender_pid: u32,
        request_id: u32,
        fd: u32,
        size: usize,              // How many bytes to read
    },

    /// Write to file descriptor
    Write {
        sender_pid: u32,
        request_id: u32,
        fd: u32,
        data: [u8; MAX_DATA_LEN],
        data_len: usize,
    },

    /// Close file descriptor
    Close {
        sender_pid: u32,
        request_id: u32,
        fd: u32,
    },

    /// List all files
    List {
        sender_pid: u32,
        request_id: u32,
    },

    /// Create new file
    Create {
        sender_pid: u32,
        request_id: u32,
        filename: [u8; MAX_FILENAME_LEN],
        filename_len: usize,
        size: u32,
    },

    /// Remove file
    Remove {
        sender_pid: u32,
        request_id: u32,
        filename: [u8; MAX_FILENAME_LEN],
        filename_len: usize,
    },

    /// Rename file
    Rename {
        sender_pid: u32,
        request_id: u32,
        old_name: [u8; MAX_FILENAME_LEN],
        old_name_len: usize,
        new_name: [u8; MAX_FILENAME_LEN],
        new_name_len: usize,
    },
}

/// Message from file server to app
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum FSToApp {
    /// Response to Open - success with file descriptor
    OpenSuccess {
        request_id: u32,
        fd: u32,
    },

    /// Response to Read - success with data
    ReadSuccess {
        request_id: u32,
        data: [u8; MAX_DATA_LEN],
        data_len: usize,
    },

    /// Response to Write - success with bytes written
    WriteSuccess {
        request_id: u32,
        bytes_written: usize,
    },

    /// Response to Close - success
    CloseSuccess {
        request_id: u32,
    },

    /// Response to List - partial file list (may need multiple messages)
    ListResponse {
        request_id: u32,
        files: [u8; MAX_DATA_LEN],  // Newline-separated filenames
        files_len: usize,
        has_more: bool,               // true if more files to come
    },

    /// Response to Create - success
    CreateSuccess {
        request_id: u32,
    },

    /// Response to Remove - success
    RemoveSuccess {
        request_id: u32,
    },

    /// Response to Rename - success
    RenameSuccess {
        request_id: u32,
    },

    /// Error response for any operation
    Error {
        request_id: u32,
        error_code: i32,  // Negative error code
    },
}

impl AppToFS {
    /// Serialize message to bytes (max 256 bytes)
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        match self {
            AppToFS::Open { sender_pid, request_id, filename, filename_len, flags } => {
                buf[0] = 200; // Message type
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..17].copy_from_slice(&filename_len.to_le_bytes());
                buf[17..21].copy_from_slice(&flags.to_le_bytes());
                let len = core::cmp::min(*filename_len, MAX_FILENAME_LEN);
                buf[21..21+len].copy_from_slice(&filename[..len]);
            }
            AppToFS::Read { sender_pid, request_id, fd, size } => {
                buf[0] = 201;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..13].copy_from_slice(&fd.to_le_bytes());
                buf[13..21].copy_from_slice(&size.to_le_bytes());
            }
            AppToFS::Write { sender_pid, request_id, fd, data, data_len } => {
                buf[0] = 202;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..13].copy_from_slice(&fd.to_le_bytes());
                buf[13..21].copy_from_slice(&data_len.to_le_bytes());
                let len = core::cmp::min(*data_len, MAX_DATA_LEN);
                buf[21..21+len].copy_from_slice(&data[..len]);
            }
            AppToFS::Close { sender_pid, request_id, fd } => {
                buf[0] = 203;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..13].copy_from_slice(&fd.to_le_bytes());
            }
            AppToFS::List { sender_pid, request_id } => {
                buf[0] = 204;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
            }
            AppToFS::Create { sender_pid, request_id, filename, filename_len, size } => {
                buf[0] = 205;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..17].copy_from_slice(&filename_len.to_le_bytes());
                buf[17..21].copy_from_slice(&size.to_le_bytes());
                let len = core::cmp::min(*filename_len, MAX_FILENAME_LEN);
                buf[21..21+len].copy_from_slice(&filename[..len]);
            }
            AppToFS::Remove { sender_pid, request_id, filename, filename_len } => {
                buf[0] = 206;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..17].copy_from_slice(&filename_len.to_le_bytes());
                let len = core::cmp::min(*filename_len, MAX_FILENAME_LEN);
                buf[17..17+len].copy_from_slice(&filename[..len]);
            }
            AppToFS::Rename { sender_pid, request_id, old_name, old_name_len, new_name, new_name_len } => {
                buf[0] = 207;
                buf[1..5].copy_from_slice(&sender_pid.to_le_bytes());
                buf[5..9].copy_from_slice(&request_id.to_le_bytes());
                buf[9..17].copy_from_slice(&old_name_len.to_le_bytes());
                buf[17..25].copy_from_slice(&new_name_len.to_le_bytes());
                let old_len = core::cmp::min(*old_name_len, 50); // Limit to fit both names
                let new_len = core::cmp::min(*new_name_len, 50);
                buf[25..25+old_len].copy_from_slice(&old_name[..old_len]);
                buf[75..75+new_len].copy_from_slice(&new_name[..new_len]);
            }
        }
        buf
    }

    /// Deserialize message from bytes
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        match buf[0] {
            200 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let filename_len = usize::from_le_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);
                let flags = u32::from_le_bytes([buf[17], buf[18], buf[19], buf[20]]);
                let mut filename = [0u8; MAX_FILENAME_LEN];
                let len = core::cmp::min(filename_len, MAX_FILENAME_LEN);
                filename[..len].copy_from_slice(&buf[21..21+len]);
                Some(AppToFS::Open { sender_pid, request_id, filename, filename_len, flags })
            }
            201 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let fd = u32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let size = usize::from_le_bytes([buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19], buf[20]]);
                Some(AppToFS::Read { sender_pid, request_id, fd, size })
            }
            202 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let fd = u32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                let data_len = usize::from_le_bytes([buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19], buf[20]]);
                let mut data = [0u8; MAX_DATA_LEN];
                let len = core::cmp::min(data_len, MAX_DATA_LEN);
                data[..len].copy_from_slice(&buf[21..21+len]);
                Some(AppToFS::Write { sender_pid, request_id, fd, data, data_len })
            }
            203 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let fd = u32::from_le_bytes([buf[9], buf[10], buf[11], buf[12]]);
                Some(AppToFS::Close { sender_pid, request_id, fd })
            }
            204 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                Some(AppToFS::List { sender_pid, request_id })
            }
            205 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let filename_len = usize::from_le_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);
                let size = u32::from_le_bytes([buf[17], buf[18], buf[19], buf[20]]);
                let mut filename = [0u8; MAX_FILENAME_LEN];
                let len = core::cmp::min(filename_len, MAX_FILENAME_LEN);
                filename[..len].copy_from_slice(&buf[21..21+len]);
                Some(AppToFS::Create { sender_pid, request_id, filename, filename_len, size })
            }
            206 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let filename_len = usize::from_le_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);
                let mut filename = [0u8; MAX_FILENAME_LEN];
                let len = core::cmp::min(filename_len, MAX_FILENAME_LEN);
                filename[..len].copy_from_slice(&buf[17..17+len]);
                Some(AppToFS::Remove { sender_pid, request_id, filename, filename_len })
            }
            207 => {
                let sender_pid = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let request_id = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let old_name_len = usize::from_le_bytes([buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15], buf[16]]);
                let new_name_len = usize::from_le_bytes([buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23], buf[24]]);
                let mut old_name = [0u8; MAX_FILENAME_LEN];
                let mut new_name = [0u8; MAX_FILENAME_LEN];
                let old_len = core::cmp::min(old_name_len, 50);
                let new_len = core::cmp::min(new_name_len, 50);
                old_name[..old_len].copy_from_slice(&buf[25..25+old_len]);
                new_name[..new_len].copy_from_slice(&buf[75..75+new_len]);
                Some(AppToFS::Rename { sender_pid, request_id, old_name, old_name_len, new_name, new_name_len })
            }
            _ => None,
        }
    }
}

impl FSToApp {
    /// Serialize message to bytes (max 256 bytes)
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        match self {
            FSToApp::OpenSuccess { request_id, fd } => {
                buf[0] = 100;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
                buf[5..9].copy_from_slice(&fd.to_le_bytes());
            }
            FSToApp::ReadSuccess { request_id, data, data_len } => {
                buf[0] = 101;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
                buf[5..13].copy_from_slice(&data_len.to_le_bytes());
                let len = core::cmp::min(*data_len, MAX_DATA_LEN);
                buf[13..13+len].copy_from_slice(&data[..len]);
            }
            FSToApp::WriteSuccess { request_id, bytes_written } => {
                buf[0] = 102;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
                buf[5..13].copy_from_slice(&bytes_written.to_le_bytes());
            }
            FSToApp::CloseSuccess { request_id } => {
                buf[0] = 103;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
            }
            FSToApp::ListResponse { request_id, files, files_len, has_more } => {
                buf[0] = 104;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
                buf[5..13].copy_from_slice(&files_len.to_le_bytes());
                buf[13] = if *has_more { 1 } else { 0 };
                let len = core::cmp::min(*files_len, MAX_DATA_LEN);
                buf[14..14+len].copy_from_slice(&files[..len]);
            }
            FSToApp::CreateSuccess { request_id } => {
                buf[0] = 105;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
            }
            FSToApp::RemoveSuccess { request_id } => {
                buf[0] = 106;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
            }
            FSToApp::RenameSuccess { request_id } => {
                buf[0] = 107;
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
            }
            FSToApp::Error { request_id, error_code } => {
                buf[0] = 255; // Error type
                buf[1..5].copy_from_slice(&request_id.to_le_bytes());
                buf[5..9].copy_from_slice(&error_code.to_le_bytes());
            }
        }
        buf
    }

    /// Deserialize message from bytes
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        match buf[0] {
            100 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let fd = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                Some(FSToApp::OpenSuccess { request_id, fd })
            }
            101 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let data_len = usize::from_le_bytes([buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12]]);
                let mut data = [0u8; MAX_DATA_LEN];
                let len = core::cmp::min(data_len, MAX_DATA_LEN);
                data[..len].copy_from_slice(&buf[13..13+len]);
                Some(FSToApp::ReadSuccess { request_id, data, data_len })
            }
            102 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let bytes_written = usize::from_le_bytes([buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12]]);
                Some(FSToApp::WriteSuccess { request_id, bytes_written })
            }
            103 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                Some(FSToApp::CloseSuccess { request_id })
            }
            104 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let files_len = usize::from_le_bytes([buf[5], buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12]]);
                let has_more = buf[13] != 0;
                let mut files = [0u8; MAX_DATA_LEN];
                let len = core::cmp::min(files_len, MAX_DATA_LEN);
                files[..len].copy_from_slice(&buf[14..14+len]);
                Some(FSToApp::ListResponse { request_id, files, files_len, has_more })
            }
            105 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                Some(FSToApp::CreateSuccess { request_id })
            }
            106 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                Some(FSToApp::RemoveSuccess { request_id })
            }
            107 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                Some(FSToApp::RenameSuccess { request_id })
            }
            255 => {
                let request_id = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let error_code = i32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                Some(FSToApp::Error { request_id, error_code })
            }
            _ => None,
        }
    }
}
