// Raw UEFI Bootloader - direct hardware control
#![no_main]
#![no_std]

extern crate alloc;
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

mod kernel;
mod raw_uefi;

use raw_uefi::*;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

// UEFI entry point
#[no_mangle]
pub extern "efiapi" fn efi_main(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Status {
    // Initialize our raw UEFI interface
    init_system_table(system_table);
    
    // Initialize heap allocator using UEFI
    unsafe {
        let bs = get_boot_services();
        let mut heap_ptr: *mut core::ffi::c_void = core::ptr::null_mut();
        let heap_size = 1024 * 256; // 256KB heap
        
        let status = (bs.allocate_pool)(
            1, // EfiLoaderData
            heap_size,
            &mut heap_ptr,
        );
        
        if status == EFI_SUCCESS {
            ALLOCATOR.lock().init(heap_ptr as *mut u8, heap_size);
        }
    }
    
    // Print startup message
    print_string("Rust OS Raw UEFI Bootloader\r\n");
    print_string("===========================\r\n");
    print_string("Direct hardware control - no uefi-rs\r\n");
    
    // Try to get GOP framebuffer before exiting boot services
    print_string("Trying to get GOP framebuffer...\r\n");
    let gop_framebuffer: Option<kernel::framebuffer::FramebufferInfo> = match locate_graphics_output_protocol() {
        Ok(gop) if !gop.is_null() => {
            print_string("GOP found - setting graphics mode\r\n");
            unsafe {
                let gop_ref = &*gop;
                
                // Try multiple GOP modes to find one with a valid framebuffer
                let mut valid_fb_info = None;
                
                for mode_num in 0..4 {
                    print_string("Trying GOP SetMode(");
                    print_number(mode_num);
                    print_string(")...\r\n");
                    
                    let set_mode_result = (gop_ref.set_mode)(gop, mode_num);
                    print_string("SetMode result: 0x");
                    print_hex(set_mode_result as u64);
                    print_string("\r\n");
                    
                    if set_mode_result == EFI_SUCCESS {
                        let mode = &*gop_ref.mode;
                        let mode_info = &*mode.info;
                        let framebuffer_base = mode.frame_buffer_base;
                        let framebuffer_size = mode.frame_buffer_size;
                        
                        print_string("Mode ");
                        print_number(mode_num);
                        print_string(" - FB base: 0x");
                        print_hex(framebuffer_base);
                        print_string(" size: 0x");
                        print_hex(framebuffer_size as u64);
                        print_string(" res: ");
                        print_number(mode_info.horizontal_resolution);
                        print_string("x");
                        print_number(mode_info.vertical_resolution);
                        print_string("\r\n");
                        
                        // Accept any mode with a non-zero framebuffer base
                        if framebuffer_base != 0 && framebuffer_size > 0 {
                            print_string("Found valid framebuffer!\r\n");
                            valid_fb_info = Some(kernel::framebuffer::FramebufferInfo {
                                base_address: framebuffer_base,
                                size: framebuffer_size as usize,
                                width: mode_info.horizontal_resolution,
                                height: mode_info.vertical_resolution,
                                pixels_per_scanline: mode_info.pixels_per_scan_line,
                                pixel_format: kernel::framebuffer::PixelFormat::Rgb,
                            });
                            break;
                        }
                    }
                }
                
                if let Some(fb_info) = valid_fb_info {
                    Some(fb_info)
                } else {
                    print_string("No valid GOP framebuffer found, checking current mode...\r\n");
                    // Try to get current mode info
                    let mode = &*gop_ref.mode;
                    let mode_info = &*mode.info;
                    let framebuffer_base = mode.frame_buffer_base;
                    let framebuffer_size = mode.frame_buffer_size;
                    
                    print_string("GOP framebuffer base: 0x");
                    print_hex(framebuffer_base);
                    print_string("\r\n");
                    print_string("GOP framebuffer size: 0x");
                    print_hex(framebuffer_size as u64);
                    print_string("\r\n");
                    print_string("GOP resolution: ");
                    print_number(mode_info.horizontal_resolution);
                    print_string("x");
                    print_number(mode_info.vertical_resolution);
                    print_string("\r\n");
                    
                    if framebuffer_base != 0 {
                        Some(kernel::framebuffer::FramebufferInfo {
                            base_address: framebuffer_base,
                            size: framebuffer_size as usize,
                            width: mode_info.horizontal_resolution,
                            height: mode_info.vertical_resolution,
                            pixels_per_scanline: mode_info.pixels_per_scan_line,
                            pixel_format: kernel::framebuffer::PixelFormat::Rgb,
                        })
                    } else {
                        print_string("GOP framebuffer base is 0\r\n");
                        None
                    }
                }
            }
        }
        _ => {
            print_string("GOP not found\r\n");
            None
        }
    };
    /*match locate_graphics_output_protocol() {
        Ok(gop) if !gop.is_null() => {
            print_string("GOP found - getting framebuffer info\r\n");
            unsafe {
                let gop_ref = &*gop;
                let mode = &*gop_ref.mode;
                let mode_info = &*mode.info;
                let framebuffer_base = mode.frame_buffer_base;
                let framebuffer_size = mode.frame_buffer_size;
                
                if framebuffer_base != 0 {
                    print_string("GOP framebuffer found at: 0x");
                    print_hex(framebuffer_base);
                    print_string("\r\n");
                    
                    Some(kernel::framebuffer::FramebufferInfo {
                        base_address: framebuffer_base,
                        size: framebuffer_size as usize,
                        width: mode_info.horizontal_resolution,
                        height: mode_info.vertical_resolution,
                        pixels_per_scanline: mode_info.pixels_per_scanline,
                        pixel_format: kernel::framebuffer::PixelFormat::Rgb,
                    })
                } else {
                    print_string("GOP found but framebuffer base is 0\r\n");
                    None
                }
            }
        }
        _ => {
            print_string("GOP not found\r\n");
            None
        }
    };*/
    
    // Now the critical part - exit boot services
    print_string("Attempting to exit boot services...\r\n");
    
    match exit_boot_services(image_handle) {
        Ok(_) => {
            // SUCCESS! Boot services are now exited
            // We can no longer use UEFI console output
            
            // Create static boot info for kernel
            use core::mem::MaybeUninit;
            static mut BOOT_INFO_STORAGE: MaybeUninit<kernel::BootInfo> = MaybeUninit::uninit();
            static mut FB_INFO_STORAGE: MaybeUninit<kernel::framebuffer::FramebufferInfo> = MaybeUninit::uninit();
            static EMPTY_MEMORY_MAP: &[kernel::memory::MemoryDescriptor] = &[];
            
            // Use GOP framebuffer if available, otherwise dummy
            let fb_info = unsafe {
                if let Some(gop_fb) = gop_framebuffer {
                    FB_INFO_STORAGE.write(gop_fb)
                } else {
                    FB_INFO_STORAGE.write(kernel::framebuffer::FramebufferInfo {
                        base_address: 0, // Will be set by VirtIO-GPU driver
                        size: 0,
                        width: 0,
                        height: 0,
                        pixels_per_scanline: 0,
                        pixel_format: kernel::framebuffer::PixelFormat::Rgb,
                    })
                }
            };
            
            let boot_info = unsafe {
                BOOT_INFO_STORAGE.write(kernel::BootInfo {
                    memory_map: EMPTY_MEMORY_MAP,
                    framebuffer: *fb_info,
                    acpi_rsdp: None,
                })
            };
            
            unsafe {
                kernel::kernel_main(boot_info);
            }
        }
        Err(status) => {
            print_string("EXIT BOOT SERVICES FAILED: 0x");
            print_hex(status as u64);
            print_string("\r\n");
            return status;
        }
    }
}

// Helper functions for console output
fn print_string(s: &str) {
    let con_out = get_con_out();
    
    // Convert to UTF-16
    let mut utf16_buffer = [0u16; 256];
    let mut i = 0;
    for ch in s.chars() {
        if i >= utf16_buffer.len() - 1 { break; }
        utf16_buffer[i] = ch as u16;
        i += 1;
    }
    utf16_buffer[i] = 0; // null terminator
    
    (con_out.output_string)(con_out, utf16_buffer.as_ptr());
}

fn print_number(n: u32) {
    let mut buffer = [0u8; 12];
    let mut i = 0;
    let mut num = n;
    
    if num == 0 {
        print_string("0");
        return;
    }
    
    while num > 0 {
        buffer[i] = (num % 10) as u8 + b'0';
        num /= 10;
        i += 1;
    }
    
    // Reverse and print
    let mut result = [0u8; 12];
    for j in 0..i {
        result[j] = buffer[i - 1 - j];
    }
    
    let s = core::str::from_utf8(&result[0..i]).unwrap_or("?");
    print_string(s);
}

fn print_hex(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    let mut buffer = [0u8; 16];
    let mut i = 0;
    let mut num = n;
    
    if num == 0 {
        print_string("0");
        return;
    }
    
    while num > 0 {
        buffer[i] = hex_chars[(num % 16) as usize];
        num /= 16;
        i += 1;
    }
    
    // Reverse and print
    let mut result = [0u8; 16];
    for j in 0..i {
        result[j] = buffer[i - 1 - j];
    }
    
    let s = core::str::from_utf8(&result[0..i]).unwrap_or("?");
    print_string(s);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfe");
        }
    }
}