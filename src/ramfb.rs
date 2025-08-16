// QEMU ramfb device configuration using fw_cfg interface
use core::ptr::{write_volatile, read_volatile};

#[repr(C, packed)]
struct RamfbConfig {
    addr: u64,      // Framebuffer address  
    fourcc: u32,    // Pixel format
    flags: u32,     // Flags (unused)
    width: u32,     // Width in pixels
    height: u32,    // Height in pixels  
    stride: u32,    // Bytes per line
}

// QEMU fw_cfg interface addresses
const FW_CFG_SELECTOR: u64 = 0x0900_0008;
const FW_CFG_DATA: u64 = 0x0900_0000;

// ramfb fw_cfg key (this is the correct QEMU value)
const FW_CFG_FILE_FIRST: u16 = 0x0020;
const RAMFB_CFG_KEY: u16 = FW_CFG_FILE_FIRST + 1; // Typically around 0x21 or higher

const RAMFB_BUFFER_ADDR: u64 = 0x4000_0000; // 1GB mark
const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // 'XR24' in little endian

pub fn init_ramfb() -> Result<(), &'static str> {
    let width = 800;
    let height = 600;
    let stride = width * 4;
    
    unsafe {
        // Create ramfb configuration
        let config = RamfbConfig {
            addr: RAMFB_BUFFER_ADDR,
            fourcc: DRM_FORMAT_XRGB8888,
            flags: 0,
            width,
            height,
            stride,
        };
        
        // Try to configure via fw_cfg
        let selector_ptr = FW_CFG_SELECTOR as *mut u16;
        let data_ptr = FW_CFG_DATA as *mut u8;
        
        // Write ramfb configuration through fw_cfg
        write_volatile(selector_ptr, RAMFB_CFG_KEY);
        
        // Write the configuration structure byte by byte
        let config_bytes = core::slice::from_raw_parts(
            &config as *const RamfbConfig as *const u8,
            core::mem::size_of::<RamfbConfig>()
        );
        
        for (i, &byte) in config_bytes.iter().enumerate() {
            write_volatile(data_ptr.add(i), byte);
        }
        
        // Initialize framebuffer with test pattern
        let fb_ptr = RAMFB_BUFFER_ADDR as *mut u32;
        
        // Clear to dark blue
        for i in 0..(width * height) {
            write_volatile(fb_ptr.add(i as usize), 0xFF001040);
        }
        
        // Add bright red rectangle
        for y in 100..200 {
            for x in 100..300 {
                let offset = y * width + x;
                write_volatile(fb_ptr.add(offset as usize), 0xFFFF0000);
            }
        }
        
        // Add green rectangle
        for y in 250..350 {
            for x in 300..500 {
                let offset = y * width + x;
                write_volatile(fb_ptr.add(offset as usize), 0xFF00FF00);
            }
        }
        
        // Add white text background
        for y in 50..90 {
            for x in 50..400 {
                let offset = y * width + x;
                write_volatile(fb_ptr.add(offset as usize), 0xFFFFFFFF);
            }
        }
    }
    
    Ok(())
}

pub fn get_framebuffer_addr() -> u64 {
    RAMFB_BUFFER_ADDR
}

pub fn get_framebuffer_size() -> (u32, u32) {
    (800, 600)
}