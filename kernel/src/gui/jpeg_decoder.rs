/// JPEG image decoder for rOSt using zune-jpeg
/// Supports standard JPEG images

use alloc::vec::Vec;
use crate::gui::bmp_decoder::BmpImage; // Reuse the same image structure
use zune_jpeg::JpegDecoder;
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

/// Parse a JPEG file from bytes
pub fn decode_jpeg(data: &[u8]) -> Option<BmpImage> {
    // Create a cursor from the byte slice
    let cursor = ZCursor::new(data);

    // Configure decoder options for RGBA output
    let mut options = DecoderOptions::default();
    options = options.jpeg_set_out_colorspace(ColorSpace::RGBA);

    // Create decoder with the cursor and options
    let mut decoder = JpegDecoder::new_with_options(cursor, options);

    // Decode headers first to get dimensions
    if let Err(_) = decoder.decode_headers() {
        crate::kernel::uart_write_string("decode_jpeg: Failed to decode headers\r\n");
        return None;
    }

    // Get image dimensions
    let (width, height) = match decoder.dimensions() {
        Some(dims) => dims,
        None => {
            crate::kernel::uart_write_string("decode_jpeg: Failed to get dimensions\r\n");
            return None;
        }
    };

    crate::kernel::uart_write_string(&alloc::format!(
        "decode_jpeg: Dimensions: {}x{}\r\n", width, height
    ));

    // Decode the full image
    let raw_pixels = match decoder.decode() {
        Ok(pixels) => pixels,
        Err(_) => {
            crate::kernel::uart_write_string("decode_jpeg: Failed to decode image\r\n");
            return None;
        }
    };

    // Convert RGBA bytes to u32 pixels in 0xAABBGGRR format (same as BMP/PNG)
    // zune-jpeg outputs RGBA in sequence: R, G, B, A, R, G, B, A, ...
    let pixel_count = width * height;
    let mut pixels = Vec::with_capacity(pixel_count);

    for i in 0..pixel_count {
        let base = i * 4;
        if base + 3 < raw_pixels.len() {
            let r = raw_pixels[base];
            let g = raw_pixels[base + 1];
            let b = raw_pixels[base + 2];
            let a = raw_pixels[base + 3];

            // Convert to 0xAABBGGRR format (same as BMP/PNG decoder output)
            let pixel = ((a as u32) << 24) | ((b as u32) << 16) | ((g as u32) << 8) | (r as u32);
            pixels.push(pixel);
        } else {
            crate::kernel::uart_write_string(&alloc::format!(
                "decode_jpeg: Pixel data truncated at index {}\r\n", i
            ));
            return None;
        }
    }

    crate::kernel::uart_write_string(&alloc::format!(
        "decode_jpeg: Successfully decoded {}x{} image ({} pixels)\r\n",
        width, height, pixels.len()
    ));

    Some(BmpImage {
        width: width as u32,
        height: height as u32,
        pixels,
    })
}
