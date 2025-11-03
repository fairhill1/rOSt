/// BMP image decoder
/// Supports 24-bit uncompressed BMP images

use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct BmpImage {
    pub width: u32,
    pub height: u32,
    /// Pixel data in 0xAABBGGRR format (top to bottom, left to right)
    pub pixels: Vec<u32>,
}

/// Parse a BMP file from bytes
pub fn decode_bmp(data: &[u8]) -> Option<BmpImage> {
    if data.len() < 54 {
        return None;
    }

    // Check BMP signature "BM"
    if data[0] != 0x42 || data[1] != 0x4D {
        return None;
    }

    // Read pixel data offset
    let data_offset = read_u32_le(&data[10..14]) as usize;

    // Read image dimensions
    let width = read_u32_le(&data[18..22]);
    let height_raw = read_i32_le(&data[22..26]);

    // Height can be negative (top-down bitmap)
    let height = height_raw.abs() as u32;
    let top_down = height_raw < 0;

    // Read bits per pixel
    let bits_per_pixel = read_u16_le(&data[28..30]);

    // Read compression method
    let compression = read_u32_le(&data[30..34]);

    // Only support uncompressed 24-bit BMPs for now
    if bits_per_pixel != 24 || compression != 0 {
        return None;
    }

    // Calculate row size (must be multiple of 4 bytes)
    let bytes_per_row = ((width * 3 + 3) / 4) * 4;

    // Allocate pixel buffer
    let mut pixels = Vec::with_capacity((width * height) as usize);

    // Read pixel data (BMP is bottom-up by default, BGR format)
    // pixels[0] should be TOP of display, pixels[last] should be BOTTOM of display
    // Bottom-up BMP: file row 0 = bottom of image, file row (height-1) = top of image
    // So for bottom-up: pixels[0] should read from file row (height-1)
    for row in 0..height {
        // Calculate which row to read from file
        let read_row = if top_down {
            row  // Top-down BMP: file row 0 = top, read in order
        } else {
            height - 1 - row  // Bottom-up BMP: flip vertically
        };

        let row_offset = data_offset + (read_row as usize * bytes_per_row as usize);

        if row_offset + (width * 3) as usize > data.len() {
            return None;
        }

        for col in 0..width {
            let pixel_offset = row_offset + (col * 3) as usize;

            // BMP stores pixels as BGR
            let b = data[pixel_offset];
            let g = data[pixel_offset + 1];
            let r = data[pixel_offset + 2];

            // Convert to framebuffer format (0xAABBGGRR)
            let pixel = 0xFF000000 | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
            pixels.push(pixel);
        }
    }

    Some(BmpImage {
        width,
        height,
        pixels,
    })
}

/// Read a 32-bit little-endian unsigned integer
fn read_u32_le(bytes: &[u8]) -> u32 {
    (bytes[0] as u32)
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24)
}

/// Read a 32-bit little-endian signed integer
fn read_i32_le(bytes: &[u8]) -> i32 {
    read_u32_le(bytes) as i32
}

/// Read a 16-bit little-endian unsigned integer
fn read_u16_le(bytes: &[u8]) -> u16 {
    (bytes[0] as u16) | ((bytes[1] as u16) << 8)
}
