/// PNG image decoder for rOSt using minipng
/// Supports standard PNG images

use alloc::vec::Vec;
use crate::gui::bmp_decoder::BmpImage; // Reuse the same image structure

/// Parse a PNG file from bytes
pub fn decode_png(data: &[u8]) -> Option<BmpImage> {
    // First, decode the PNG header to get required buffer size
    let header = match minipng::decode_png_header(data) {
        Ok(h) => h,
        Err(_) => return None,
    };

    let width = header.width();
    let height = header.height();

    // Allocate buffer for RGBA8 output
    let required_bytes = header.required_bytes_rgba8bpc();
    let mut buffer = Vec::with_capacity(required_bytes);
    buffer.resize(required_bytes, 0);

    // Decode the PNG into the buffer
    let mut image = match minipng::decode_png(data, &mut buffer) {
        Ok(img) => img,
        Err(_) => return None,
    };

    // Convert to RGBA8 (8 bits per channel)
    image.convert_to_rgba8bpc();

    // Get the pixel data
    let raw_pixels = image.pixels();

    // Convert RGBA bytes to u32 pixels in 0xAABBGGRR format (same as BMP)
    // minipng outputs RGBA in sequence: R, G, B, A, R, G, B, A, ...
    let pixel_count = (width * height) as usize;
    let mut pixels = Vec::with_capacity(pixel_count);

    for i in 0..pixel_count {
        let base = i * 4;
        if base + 3 < raw_pixels.len() {
            let r = raw_pixels[base];
            let g = raw_pixels[base + 1];
            let b = raw_pixels[base + 2];
            let a = raw_pixels[base + 3];

            // Convert to 0xAABBGGRR format (same as BMP decoder output)
            let pixel = ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
            pixels.push(pixel);
        } else {
            return None;
        }
    }

    Some(BmpImage {
        width,
        height,
        pixels,
    })
}
