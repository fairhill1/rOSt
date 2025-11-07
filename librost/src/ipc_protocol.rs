//! IPC protocol messages for kernel ↔ window manager communication
//!
//! These messages fit within the 256-byte message size limit.
//!
//! ## Design
//! - All wire-format structs are `#[repr(C)]` for guaranteed layout
//! - Enums contain these structs (no `#[repr(C)]` on enums!)
//! - Serialization uses safe transmutation of known-layout structs
//! - Message type is first byte of each struct for easy dispatch

#![allow(dead_code)]

use crate::runtime::InputEvent;

/// Maximum length for window titles in IPC messages
const MAX_TITLE_LEN: usize = 64;

/// Maximum filename length in filesystem messages
const MAX_FILENAME_LEN: usize = 128;

/// Maximum data payload per message (for read/write operations)
const MAX_DATA_LEN: usize = 200;

// ============================================================================
// Message Type Constants
// ============================================================================

pub mod msg_types {
    // Kernel ↔ WM messages (0-99)
    pub const KERNEL_INPUT_EVENT: u8 = 0;
    pub const KERNEL_CREATE_WINDOW: u8 = 1;
    pub const KERNEL_CLOSE_WINDOW: u8 = 2;
    pub const KERNEL_SET_FOCUS: u8 = 3;
    pub const KERNEL_REQUEST_REDRAW: u8 = 4;

    pub const WM_ROUTE_INPUT: u8 = 10;
    pub const WM_REQUEST_FOCUS: u8 = 11;
    pub const WM_REQUEST_CLOSE: u8 = 12;
    pub const WM_WINDOW_CREATED: u8 = 13;
    pub const WM_NO_ACTION: u8 = 14;

    // App ↔ File Server messages (100-255)
    pub const FS_OPEN: u8 = 100;
    pub const FS_READ: u8 = 101;
    pub const FS_WRITE: u8 = 102;
    pub const FS_CLOSE: u8 = 103;
    pub const FS_LIST: u8 = 104;
    pub const FS_CREATE: u8 = 105;
    pub const FS_REMOVE: u8 = 106;
    pub const FS_RENAME: u8 = 107;

    pub const FS_OPEN_SUCCESS: u8 = 110;
    pub const FS_READ_SUCCESS: u8 = 111;
    pub const FS_WRITE_SUCCESS: u8 = 112;
    pub const FS_CLOSE_SUCCESS: u8 = 113;
    pub const FS_LIST_RESPONSE: u8 = 114;
    pub const FS_CREATE_SUCCESS: u8 = 115;
    pub const FS_REMOVE_SUCCESS: u8 = 116;
    pub const FS_RENAME_SUCCESS: u8 = 117;
    pub const FS_ERROR: u8 = 255;
}

// ============================================================================
// Kernel ↔ WM Wire Format Structs
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InputEventMsg {
    pub msg_type: u8,      // msg_types::KERNEL_INPUT_EVENT
    pub _pad1: [u8; 3],    // Align to 4 bytes
    pub sender_pid: u32,
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub event: InputEvent,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CreateWindowMsg {
    pub msg_type: u8,      // msg_types::KERNEL_CREATE_WINDOW
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub id: usize,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title_len: usize,
    pub title: [u8; MAX_TITLE_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CloseWindowMsg {
    pub msg_type: u8,      // msg_types::KERNEL_CLOSE_WINDOW
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub id: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SetFocusMsg {
    pub msg_type: u8,      // msg_types::KERNEL_SET_FOCUS
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub id: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RequestRedrawMsg {
    pub msg_type: u8,      // msg_types::KERNEL_REQUEST_REDRAW
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub id: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RouteInputMsg {
    pub msg_type: u8,      // msg_types::WM_ROUTE_INPUT
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub window_id: usize,
    pub event: InputEvent,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RequestFocusMsg {
    pub msg_type: u8,      // msg_types::WM_REQUEST_FOCUS
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub window_id: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RequestCloseMsg {
    pub msg_type: u8,      // msg_types::WM_REQUEST_CLOSE
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub window_id: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WindowCreatedMsg {
    pub msg_type: u8,      // msg_types::WM_WINDOW_CREATED
    pub _pad1: [u8; 7],    // Align to 8 bytes
    pub window_id: usize,
    pub shm_id: i32,
    pub _pad2: [u8; 4],    // Align to 8 bytes
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NoActionMsg {
    pub msg_type: u8,      // msg_types::WM_NO_ACTION
}

// ============================================================================
// Kernel ↔ WM Enums (contain the structs above)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum KernelToWM {
    InputEvent(InputEventMsg),
    CreateWindow(CreateWindowMsg),
    CloseWindow(CloseWindowMsg),
    SetFocus(SetFocusMsg),
    RequestRedraw(RequestRedrawMsg),
}

#[derive(Debug, Clone, Copy)]
pub enum WMToKernel {
    RouteInput(RouteInputMsg),
    RequestFocus(RequestFocusMsg),
    RequestClose(RequestCloseMsg),
    WindowCreated(WindowCreatedMsg),
    NoAction(NoActionMsg),
}

impl KernelToWM {
    /// Serialize message to bytes using safe transmutation
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        unsafe {
            match self {
                KernelToWM::InputEvent(msg) => {
                    let ptr = msg as *const InputEventMsg as *const u8;
                    let len = core::mem::size_of::<InputEventMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                KernelToWM::CreateWindow(msg) => {
                    let ptr = msg as *const CreateWindowMsg as *const u8;
                    let len = core::mem::size_of::<CreateWindowMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                KernelToWM::CloseWindow(msg) => {
                    let ptr = msg as *const CloseWindowMsg as *const u8;
                    let len = core::mem::size_of::<CloseWindowMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                KernelToWM::SetFocus(msg) => {
                    let ptr = msg as *const SetFocusMsg as *const u8;
                    let len = core::mem::size_of::<SetFocusMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                KernelToWM::RequestRedraw(msg) => {
                    let ptr = msg as *const RequestRedrawMsg as *const u8;
                    let len = core::mem::size_of::<RequestRedrawMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
            }
        }
        buf
    }

    /// Deserialize message from bytes using safe transmutation
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        let msg_type = buf[0];
        unsafe {
            match msg_type {
                msg_types::KERNEL_INPUT_EVENT => {
                    let msg = &*(buf.as_ptr() as *const InputEventMsg);
                    Some(KernelToWM::InputEvent(*msg))
                }
                msg_types::KERNEL_CREATE_WINDOW => {
                    let msg = &*(buf.as_ptr() as *const CreateWindowMsg);
                    Some(KernelToWM::CreateWindow(*msg))
                }
                msg_types::KERNEL_CLOSE_WINDOW => {
                    let msg = &*(buf.as_ptr() as *const CloseWindowMsg);
                    Some(KernelToWM::CloseWindow(*msg))
                }
                msg_types::KERNEL_SET_FOCUS => {
                    let msg = &*(buf.as_ptr() as *const SetFocusMsg);
                    Some(KernelToWM::SetFocus(*msg))
                }
                msg_types::KERNEL_REQUEST_REDRAW => {
                    let msg = &*(buf.as_ptr() as *const RequestRedrawMsg);
                    Some(KernelToWM::RequestRedraw(*msg))
                }
                _ => None,
            }
        }
    }
}

impl WMToKernel {
    /// Serialize message to bytes using safe transmutation
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        unsafe {
            match self {
                WMToKernel::RouteInput(msg) => {
                    let ptr = msg as *const RouteInputMsg as *const u8;
                    let len = core::mem::size_of::<RouteInputMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                WMToKernel::RequestFocus(msg) => {
                    let ptr = msg as *const RequestFocusMsg as *const u8;
                    let len = core::mem::size_of::<RequestFocusMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                WMToKernel::RequestClose(msg) => {
                    let ptr = msg as *const RequestCloseMsg as *const u8;
                    let len = core::mem::size_of::<RequestCloseMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                WMToKernel::WindowCreated(msg) => {
                    let ptr = msg as *const WindowCreatedMsg as *const u8;
                    let len = core::mem::size_of::<WindowCreatedMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                WMToKernel::NoAction(msg) => {
                    let ptr = msg as *const NoActionMsg as *const u8;
                    let len = core::mem::size_of::<NoActionMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
            }
        }
        buf
    }

    /// Deserialize message from bytes using safe transmutation
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        let msg_type = buf[0];
        unsafe {
            match msg_type {
                msg_types::WM_ROUTE_INPUT => {
                    let msg = &*(buf.as_ptr() as *const RouteInputMsg);
                    Some(WMToKernel::RouteInput(*msg))
                }
                msg_types::WM_REQUEST_FOCUS => {
                    let msg = &*(buf.as_ptr() as *const RequestFocusMsg);
                    Some(WMToKernel::RequestFocus(*msg))
                }
                msg_types::WM_REQUEST_CLOSE => {
                    let msg = &*(buf.as_ptr() as *const RequestCloseMsg);
                    Some(WMToKernel::RequestClose(*msg))
                }
                msg_types::WM_WINDOW_CREATED => {
                    let msg = &*(buf.as_ptr() as *const WindowCreatedMsg);
                    Some(WMToKernel::WindowCreated(*msg))
                }
                msg_types::WM_NO_ACTION => {
                    let msg = &*(buf.as_ptr() as *const NoActionMsg);
                    Some(WMToKernel::NoAction(*msg))
                }
                _ => None,
            }
        }
    }
}

// ============================================================================
// File Server Wire Format Structs
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSOpenMsg {
    pub msg_type: u8,      // msg_types::FS_OPEN
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub filename_len: usize,
    pub flags: u32,
    pub _pad2: [u8; 4],
    pub filename: [u8; MAX_FILENAME_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSReadMsg {
    pub msg_type: u8,      // msg_types::FS_READ
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub fd: u32,
    pub _pad2: [u8; 4],
    pub size: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSWriteMsg {
    pub msg_type: u8,      // msg_types::FS_WRITE
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub fd: u32,
    pub _pad2: [u8; 4],
    pub data_len: usize,
    pub data: [u8; MAX_DATA_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSCloseMsg {
    pub msg_type: u8,      // msg_types::FS_CLOSE
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub fd: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSListMsg {
    pub msg_type: u8,      // msg_types::FS_LIST
    pub _pad1: [u8; 3],
    pub request_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSCreateMsg {
    pub msg_type: u8,      // msg_types::FS_CREATE
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub filename_len: usize,
    pub size: u32,
    pub _pad2: [u8; 4],
    pub filename: [u8; MAX_FILENAME_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSRemoveMsg {
    pub msg_type: u8,      // msg_types::FS_REMOVE
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub filename_len: usize,
    pub filename: [u8; MAX_FILENAME_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSRenameMsg {
    pub msg_type: u8,      // msg_types::FS_RENAME
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub old_name_len: usize,
    pub new_name_len: usize,
    pub old_name: [u8; MAX_FILENAME_LEN],
    pub new_name: [u8; MAX_FILENAME_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSOpenSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_OPEN_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub fd: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSReadSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_READ_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub data_len: usize,
    pub data: [u8; MAX_DATA_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSWriteSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_WRITE_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub bytes_written: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSCloseSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_CLOSE_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSListResponseMsg {
    pub msg_type: u8,      // msg_types::FS_LIST_RESPONSE
    pub has_more: u8,
    pub _pad1: [u8; 2],
    pub request_id: u32,
    pub files_len: usize,
    pub files: [u8; MAX_DATA_LEN],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSCreateSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_CREATE_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSRemoveSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_REMOVE_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSRenameSuccessMsg {
    pub msg_type: u8,      // msg_types::FS_RENAME_SUCCESS
    pub _pad1: [u8; 3],
    pub request_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FSErrorMsg {
    pub msg_type: u8,      // msg_types::FS_ERROR
    pub _pad1: [u8; 3],
    pub request_id: u32,
    pub error_code: i32,
}

// ============================================================================
// File Server Enums (contain the structs above)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub enum AppToFS {
    Open(FSOpenMsg),
    Read(FSReadMsg),
    Write(FSWriteMsg),
    Close(FSCloseMsg),
    List(FSListMsg),
    Create(FSCreateMsg),
    Remove(FSRemoveMsg),
    Rename(FSRenameMsg),
}

#[derive(Debug, Clone, Copy)]
pub enum FSToApp {
    OpenSuccess(FSOpenSuccessMsg),
    ReadSuccess(FSReadSuccessMsg),
    WriteSuccess(FSWriteSuccessMsg),
    CloseSuccess(FSCloseSuccessMsg),
    ListResponse(FSListResponseMsg),
    CreateSuccess(FSCreateSuccessMsg),
    RemoveSuccess(FSRemoveSuccessMsg),
    RenameSuccess(FSRenameSuccessMsg),
    Error(FSErrorMsg),
}

impl AppToFS {
    /// Serialize message to bytes using safe transmutation
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        unsafe {
            match self {
                AppToFS::Open(msg) => {
                    let ptr = msg as *const FSOpenMsg as *const u8;
                    let len = core::mem::size_of::<FSOpenMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Read(msg) => {
                    let ptr = msg as *const FSReadMsg as *const u8;
                    let len = core::mem::size_of::<FSReadMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Write(msg) => {
                    let ptr = msg as *const FSWriteMsg as *const u8;
                    let len = core::mem::size_of::<FSWriteMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Close(msg) => {
                    let ptr = msg as *const FSCloseMsg as *const u8;
                    let len = core::mem::size_of::<FSCloseMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::List(msg) => {
                    let ptr = msg as *const FSListMsg as *const u8;
                    let len = core::mem::size_of::<FSListMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Create(msg) => {
                    let ptr = msg as *const FSCreateMsg as *const u8;
                    let len = core::mem::size_of::<FSCreateMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Remove(msg) => {
                    let ptr = msg as *const FSRemoveMsg as *const u8;
                    let len = core::mem::size_of::<FSRemoveMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                AppToFS::Rename(msg) => {
                    let ptr = msg as *const FSRenameMsg as *const u8;
                    let len = core::mem::size_of::<FSRenameMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
            }
        }
        buf
    }

    /// Deserialize message from bytes using safe transmutation
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        let msg_type = buf[0];
        unsafe {
            match msg_type {
                msg_types::FS_OPEN => {
                    let msg = &*(buf.as_ptr() as *const FSOpenMsg);
                    Some(AppToFS::Open(*msg))
                }
                msg_types::FS_READ => {
                    let msg = &*(buf.as_ptr() as *const FSReadMsg);
                    Some(AppToFS::Read(*msg))
                }
                msg_types::FS_WRITE => {
                    let msg = &*(buf.as_ptr() as *const FSWriteMsg);
                    Some(AppToFS::Write(*msg))
                }
                msg_types::FS_CLOSE => {
                    let msg = &*(buf.as_ptr() as *const FSCloseMsg);
                    Some(AppToFS::Close(*msg))
                }
                msg_types::FS_LIST => {
                    let msg = &*(buf.as_ptr() as *const FSListMsg);
                    Some(AppToFS::List(*msg))
                }
                msg_types::FS_CREATE => {
                    let msg = &*(buf.as_ptr() as *const FSCreateMsg);
                    Some(AppToFS::Create(*msg))
                }
                msg_types::FS_REMOVE => {
                    let msg = &*(buf.as_ptr() as *const FSRemoveMsg);
                    Some(AppToFS::Remove(*msg))
                }
                msg_types::FS_RENAME => {
                    let msg = &*(buf.as_ptr() as *const FSRenameMsg);
                    Some(AppToFS::Rename(*msg))
                }
                _ => None,
            }
        }
    }
}

impl FSToApp {
    /// Serialize message to bytes using safe transmutation
    pub fn to_bytes(&self) -> [u8; 256] {
        let mut buf = [0u8; 256];
        unsafe {
            match self {
                FSToApp::OpenSuccess(msg) => {
                    let ptr = msg as *const FSOpenSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSOpenSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::ReadSuccess(msg) => {
                    let ptr = msg as *const FSReadSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSReadSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::WriteSuccess(msg) => {
                    let ptr = msg as *const FSWriteSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSWriteSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::CloseSuccess(msg) => {
                    let ptr = msg as *const FSCloseSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSCloseSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::ListResponse(msg) => {
                    let ptr = msg as *const FSListResponseMsg as *const u8;
                    let len = core::mem::size_of::<FSListResponseMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::CreateSuccess(msg) => {
                    let ptr = msg as *const FSCreateSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSCreateSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::RemoveSuccess(msg) => {
                    let ptr = msg as *const FSRemoveSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSRemoveSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::RenameSuccess(msg) => {
                    let ptr = msg as *const FSRenameSuccessMsg as *const u8;
                    let len = core::mem::size_of::<FSRenameSuccessMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
                FSToApp::Error(msg) => {
                    let ptr = msg as *const FSErrorMsg as *const u8;
                    let len = core::mem::size_of::<FSErrorMsg>();
                    let slice = core::slice::from_raw_parts(ptr, len);
                    buf[..len].copy_from_slice(slice);
                }
            }
        }
        buf
    }

    /// Deserialize message from bytes using safe transmutation
    pub fn from_bytes(buf: &[u8; 256]) -> Option<Self> {
        let msg_type = buf[0];
        unsafe {
            match msg_type {
                msg_types::FS_OPEN_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSOpenSuccessMsg);
                    Some(FSToApp::OpenSuccess(*msg))
                }
                msg_types::FS_READ_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSReadSuccessMsg);
                    Some(FSToApp::ReadSuccess(*msg))
                }
                msg_types::FS_WRITE_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSWriteSuccessMsg);
                    Some(FSToApp::WriteSuccess(*msg))
                }
                msg_types::FS_CLOSE_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSCloseSuccessMsg);
                    Some(FSToApp::CloseSuccess(*msg))
                }
                msg_types::FS_LIST_RESPONSE => {
                    let msg = &*(buf.as_ptr() as *const FSListResponseMsg);
                    Some(FSToApp::ListResponse(*msg))
                }
                msg_types::FS_CREATE_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSCreateSuccessMsg);
                    Some(FSToApp::CreateSuccess(*msg))
                }
                msg_types::FS_REMOVE_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSRemoveSuccessMsg);
                    Some(FSToApp::RemoveSuccess(*msg))
                }
                msg_types::FS_RENAME_SUCCESS => {
                    let msg = &*(buf.as_ptr() as *const FSRenameSuccessMsg);
                    Some(FSToApp::RenameSuccess(*msg))
                }
                msg_types::FS_ERROR => {
                    let msg = &*(buf.as_ptr() as *const FSErrorMsg);
                    Some(FSToApp::Error(*msg))
                }
                _ => None,
            }
        }
    }
}
