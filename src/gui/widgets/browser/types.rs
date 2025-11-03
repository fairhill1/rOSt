use crate::gui::bmp_decoder::BmpImage;
use smoltcp::iface::SocketHandle;
use alloc::string::String;
use alloc::vec::Vec;

/// Async HTTP request state machine
pub enum HttpState {
    Idle,
    ResolvingDns {
        host: String,
        path: String,
        port: u16,
        start_time: u64,
    },
    Connecting {
        socket_handle: SocketHandle,
        http_request: String,
        start_time: u64,
    },
    ReceivingResponse {
        socket_handle: SocketHandle,
        response_data: Vec<u8>,
        last_recv_time: u64,
    },
    Complete {
        html: String,
    },
    Error {
        message: String,
    },
}

/// Pending image load request
pub struct PendingImage {
    pub url: String,
    pub layout_box_index: usize,
}

/// Image format types
#[derive(Clone, Copy, Debug)]
pub enum ImageFormat {
    Bmp,
    Png,
    Jpeg,
}

/// Image loading state machine
pub enum ImageLoadState {
    Idle,
    Connecting {
        socket_handle: SocketHandle,
        http_request: String,
        start_time: u64,
        layout_box_index: usize,
        format: ImageFormat,
        url: String, // For caching
    },
    Loading {
        socket_handle: SocketHandle,
        response_data: Vec<u8>,
        last_recv_time: u64,
        layout_box_index: usize,
        format: ImageFormat,
        url: String, // For caching
    },
}

/// Pending CSS load request
pub struct PendingCss {
    pub url: String,
}

/// CSS loading state machine
pub enum CssLoadState {
    Idle,
    Connecting {
        socket_handle: SocketHandle,
        http_request: String,
        start_time: u64,
        url: String,
    },
    Loading {
        socket_handle: SocketHandle,
        response_data: Vec<u8>,
        last_recv_time: u64,
        url: String,
    },
}

/// Simple color structure
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    pub const BLACK: Color = Color::new(0, 0, 0);

    pub fn to_u32(&self) -> u32 {
        0xFF000000 | ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }
}

/// Layout box - represents a positioned element to render
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub text: String,
    pub color: Color,
    pub background_color: Option<Color>, // CSS background-color
    pub font_size: f32, // Font size in pixels (e.g., 18.0, 48.0)
    pub is_link: bool,
    pub link_url: String,
    pub bold: bool,
    pub italic: bool,
    pub element_id: String, // HTML element ID attribute
    pub is_image: bool,
    pub image_data: Option<BmpImage>,
    pub is_hr: bool, // Horizontal rule - render as solid line
    pub is_table_cell: bool, // Table cell - render with borders
    pub is_header_cell: bool, // Header cell (th) - render with bold/different bg
}
