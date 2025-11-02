// Image Viewer - Display BMP and PNG images in a window

use crate::gui::framebuffer;
use crate::gui::bmp_decoder::{BmpImage, decode_bmp};
use crate::gui::png_decoder::decode_png;
extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;

pub struct ImageViewer {
    image: Option<BmpImage>,
    filename: String,
    error_message: Option<String>,
    scroll_x: i32,
    scroll_y: i32,
}

impl ImageViewer {
    pub fn new() -> Self {
        ImageViewer {
            image: None,
            filename: String::new(),
            error_message: None,
            scroll_x: 0,
            scroll_y: 0,
        }
    }

    /// Load an image from raw file data
    pub fn load_image(&mut self, filename: &str, data: &[u8]) {
        self.filename = String::from(filename);

        // Detect image format by magic bytes
        let is_png = data.len() >= 8 &&
                     data[0] == 0x89 && data[1] == 0x50 &&
                     data[2] == 0x4E && data[3] == 0x47;
        let is_bmp = data.len() >= 2 &&
                     data[0] == 0x42 && data[1] == 0x4D;

        crate::kernel::uart_write_string(&alloc::format!(
            "[ImageViewer] Loading {} (PNG={}, BMP={})\r\n",
            filename, is_png, is_bmp
        ));

        let result = if is_png {
            decode_png(data)
        } else if is_bmp {
            decode_bmp(data)
        } else {
            crate::kernel::uart_write_string(
                "[ImageViewer] Unknown image format (not PNG or BMP)\r\n"
            );
            None
        };

        match result {
            Some(img) => {
                crate::kernel::uart_write_string(&alloc::format!(
                    "[ImageViewer] Loaded {}x{} image\r\n", img.width, img.height
                ));
                self.image = Some(img);
                self.error_message = None;
            }
            None => {
                let format_name = if is_png { "PNG" } else if is_bmp { "BMP" } else { "unknown" };
                self.error_message = Some(alloc::format!("Failed to decode {} image", format_name));
                self.image = None;
            }
        }
    }

    /// Render the image viewer
    pub fn render_at(&self, offset_x: i32, offset_y: i32, width: u32, height: u32) {
        // Display error if image failed to load
        if let Some(ref error) = self.error_message {
            let error_y = offset_y + (height / 2) as i32;
            framebuffer::draw_string(
                (offset_x + 20) as u32,
                error_y as u32,
                error,
                0xFFFF0000, // Red
            );
            return;
        }

        // Display image if loaded
        if let Some(ref img) = self.image {
            // Get framebuffer for direct pixel writes (like browser does)
            let fb = framebuffer::get_back_buffer();
            let (fb_width_u32, fb_height_u32) = framebuffer::get_screen_dimensions();
            let fb_width = fb_width_u32 as usize;
            let fb_height = fb_height_u32 as usize;

            // Calculate centering offsets
            let img_width = img.width as i32;
            let img_height = img.height as i32;
            let window_width = width as i32;
            let window_height = height as i32;

            // Center the image if it's smaller than the window
            let center_x = if img_width < window_width {
                (window_width - img_width) / 2
            } else {
                0
            };
            let center_y = if img_height < window_height {
                (window_height - img_height) / 2
            } else {
                0
            };

            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    // Calculate source pixel coordinates (accounting for centering and scroll)
                    let src_x = x - center_x + self.scroll_x;
                    let src_y = y - center_y + self.scroll_y;

                    // Check if source pixel is within image bounds
                    if src_x >= 0 && src_x < img_width && src_y >= 0 && src_y < img_height {
                        let pixel_idx = (src_y * img_width + src_x) as usize;
                        if pixel_idx < img.pixels.len() {
                            let dest_x = (offset_x + x) as usize;
                            let dest_y = (offset_y + y) as usize;

                            // Write directly to framebuffer
                            // Swap R and B channels: convert 0xAABBGGRR to 0xAARRGGBB
                            if dest_x < fb_width && dest_y < fb_height {
                                let pixel = img.pixels[pixel_idx];
                                let r = pixel & 0xFF;
                                let g = (pixel >> 8) & 0xFF;
                                let b = (pixel >> 16) & 0xFF;
                                let a = pixel & 0xFF000000;
                                let swapped = a | (r << 16) | (g << 8) | b;
                                fb[dest_y * fb_width + dest_x] = swapped;
                            }
                        }
                    }
                }
            }

            // Display image info at the bottom
            let info_text = alloc::format!(
                "{} - {}x{} pixels",
                self.filename, img.width, img.height
            );
            let info_y = offset_y + height as i32 - 24;

            // Draw semi-transparent background for info text
            for dy in 0..20 {
                for dx in 0..width {
                    let px = offset_x + dx as i32;
                    let py = info_y + dy;
                    if py >= offset_y && py < offset_y + height as i32 {
                        framebuffer::draw_pixel(px as u32, py as u32, 0xCC000000);
                    }
                }
            }

            framebuffer::draw_string(
                (offset_x + 8) as u32,
                (info_y + 4) as u32,
                &info_text,
                0xFFFFFFFF,
            );
        } else {
            // No image loaded
            let msg = "No image loaded";
            framebuffer::draw_string(
                (offset_x + 20) as u32,
                (offset_y + 20) as u32,
                msg,
                0xFFAAAAAA,
            );
        }
    }

    /// Scroll the image (for images larger than window)
    pub fn scroll(&mut self, dx: i32, dy: i32) {
        if let Some(ref img) = self.image {
            self.scroll_x = (self.scroll_x + dx).max(0).min(img.width as i32);
            self.scroll_y = (self.scroll_y + dy).max(0).min(img.height as i32);
        }
    }
}

/// Global image viewer instances
static mut IMAGE_VIEWERS: Vec<ImageViewer> = Vec::new();

pub fn init() {
    // Nothing to do - viewers are created on demand
}

/// Create a new image viewer instance and return its ID
pub fn create_image_viewer() -> usize {
    unsafe {
        IMAGE_VIEWERS.push(ImageViewer::new());
        IMAGE_VIEWERS.len() - 1
    }
}

/// Create an image viewer with loaded image data
pub fn create_image_viewer_with_data(filename: &str, data: &[u8]) -> usize {
    unsafe {
        let mut viewer = ImageViewer::new();
        viewer.load_image(filename, data);
        IMAGE_VIEWERS.push(viewer);
        IMAGE_VIEWERS.len() - 1
    }
}

/// Remove an image viewer instance by ID
pub fn remove_image_viewer(id: usize) {
    unsafe {
        if id < IMAGE_VIEWERS.len() {
            IMAGE_VIEWERS.remove(id);
        }
    }
}

/// Get an image viewer instance by ID
pub fn get_image_viewer(id: usize) -> Option<&'static mut ImageViewer> {
    unsafe {
        IMAGE_VIEWERS.get_mut(id)
    }
}

/// Render an image viewer instance
pub fn render_at(id: usize, offset_x: i32, offset_y: i32, width: u32, height: u32) {
    if let Some(viewer) = get_image_viewer(id) {
        viewer.render_at(offset_x, offset_y, width, height);
    }
}

/// Scroll an image viewer
pub fn scroll(id: usize, dx: i32, dy: i32) {
    if let Some(viewer) = get_image_viewer(id) {
        viewer.scroll(dx, dy);
    }
}
