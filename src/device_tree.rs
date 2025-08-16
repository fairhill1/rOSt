// Device Tree Blob (DTB) parser for hardware discovery
use core::slice;
use core::str;

// Helper function to read big-endian u32
unsafe fn read_be(ptr: *const u32) -> u32 {
    let bytes = core::ptr::read(ptr as *const [u8; 4]);
    u32::from_be_bytes(bytes)
}

// Helper function to read big-endian u64
unsafe fn read_be_u64(ptr: *const u64) -> u64 {
    let bytes = core::ptr::read(ptr as *const [u8; 8]);
    u64::from_be_bytes(bytes)
}

#[derive(Debug)]
pub struct DeviceTreeInfo {
    pub framebuffer_addr: Option<u64>,
    pub framebuffer_size: Option<(u32, u32)>,
    pub framebuffer_stride: Option<u32>,
}

#[repr(C)]
struct FdtHeader {
    magic: u32,
    totalsize: u32,
    off_dt_struct: u32,
    off_dt_strings: u32,
    off_mem_rsvmap: u32,
    version: u32,
    last_comp_version: u32,
    boot_cpuid_phys: u32,
    size_dt_strings: u32,
    size_dt_struct: u32,
}

const FDT_MAGIC: u32 = 0xd00dfeed;
const FDT_BEGIN_NODE: u32 = 0x00000001;
const FDT_END_NODE: u32 = 0x00000002;
const FDT_PROP: u32 = 0x00000003;
const FDT_NOP: u32 = 0x00000004;
const FDT_END: u32 = 0x00000009;

pub fn parse_device_tree(dtb_addr: u64) -> Result<DeviceTreeInfo, &'static str> {
    let mut info = DeviceTreeInfo {
        framebuffer_addr: None,
        framebuffer_size: None,
        framebuffer_stride: None,
    };
    
    unsafe {
        let header = dtb_addr as *const FdtHeader;
        let magic = read_be(&(*header).magic);
        
        if magic != FDT_MAGIC {
            return Err("Invalid device tree magic");
        }
        
        let totalsize = read_be(&(*header).totalsize) as usize;
        let off_dt_struct = read_be(&(*header).off_dt_struct) as usize;
        let off_dt_strings = read_be(&(*header).off_dt_strings) as usize;
        
        let dtb_data = slice::from_raw_parts(dtb_addr as *const u8, totalsize);
        let struct_data = &dtb_data[off_dt_struct..];
        let strings_data = &dtb_data[off_dt_strings..];
        
        parse_struct_block(struct_data, strings_data, &mut info)?;
    }
    
    Ok(info)
}

unsafe fn parse_struct_block(struct_data: &[u8], strings_data: &[u8], info: &mut DeviceTreeInfo) -> Result<(), &'static str> {
    let mut offset = 0;
    let mut depth = 0;
    let mut current_node = "";
    
    while offset < struct_data.len() {
        let token_ptr = struct_data.as_ptr().add(offset) as *const u32;
        let token = read_be(token_ptr);
        offset += 4;
        
        match token {
            FDT_BEGIN_NODE => {
                // Node name follows
                let name_start = offset;
                while offset < struct_data.len() && struct_data[offset] != 0 {
                    offset += 1;
                }
                
                if offset < struct_data.len() {
                    let name_bytes = &struct_data[name_start..offset];
                    if let Ok(name) = str::from_utf8(name_bytes) {
                        current_node = name;
                        
                        // Look for framebuffer-related nodes
                        if name.contains("framebuffer") || name.contains("display") || name.contains("ramfb") {
                            // This might be our framebuffer node
                        }
                    }
                }
                
                // Align to 4-byte boundary
                offset = (offset + 4) & !3;
                depth += 1;
            }
            
            FDT_PROP => {
                if offset + 8 > struct_data.len() {
                    break;
                }
                
                let len_ptr = struct_data.as_ptr().add(offset) as *const u32;
                let nameoff_ptr = struct_data.as_ptr().add(offset + 4) as *const u32;
                let len = read_be(len_ptr) as usize;
                let nameoff = read_be(nameoff_ptr) as usize;
                offset += 8;
                
                if offset + len > struct_data.len() {
                    break;
                }
                
                // Get property name
                if nameoff < strings_data.len() {
                    let mut name_end = nameoff;
                    while name_end < strings_data.len() && strings_data[name_end] != 0 {
                        name_end += 1;
                    }
                    
                    if let Ok(prop_name) = str::from_utf8(&strings_data[nameoff..name_end]) {
                        // Parse framebuffer properties
                        if current_node.contains("framebuffer") || current_node.contains("ramfb") {
                            match prop_name {
                                "reg" => {
                                    // Address and size
                                    if len >= 16 {
                                        let addr_ptr = struct_data.as_ptr().add(offset) as *const u64;
                                        info.framebuffer_addr = Some(read_be_u64(addr_ptr));
                                    }
                                }
                                "width" => {
                                    if len >= 4 {
                                        let width_ptr = struct_data.as_ptr().add(offset) as *const u32;
                                        let width = read_be(width_ptr);
                                        if let Some((_, height)) = info.framebuffer_size {
                                            info.framebuffer_size = Some((width, height));
                                        } else {
                                            info.framebuffer_size = Some((width, 0));
                                        }
                                    }
                                }
                                "height" => {
                                    if len >= 4 {
                                        let height_ptr = struct_data.as_ptr().add(offset) as *const u32;
                                        let height = read_be(height_ptr);
                                        if let Some((width, _)) = info.framebuffer_size {
                                            info.framebuffer_size = Some((width, height));
                                        } else {
                                            info.framebuffer_size = Some((0, height));
                                        }
                                    }
                                }
                                "stride" => {
                                    if len >= 4 {
                                        let stride_ptr = struct_data.as_ptr().add(offset) as *const u32;
                                        info.framebuffer_stride = Some(read_be(stride_ptr));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                
                offset += (len + 3) & !3; // Align to 4 bytes
            }
            
            FDT_END_NODE => {
                depth -= 1;
                if depth == 0 {
                    current_node = "";
                }
            }
            
            FDT_NOP => {
                // Skip
            }
            
            FDT_END => {
                break;
            }
            
            _ => {
                return Err("Unknown DTB token");
            }
        }
    }
    
    Ok(())
}

// Get device tree address from bootloader
// In QEMU aarch64 virt machine, DTB is typically loaded at 0x40000000
pub fn get_dtb_address() -> u64 {
    // For QEMU virt machine, check common DTB locations
    let possible_addresses = [
        0x40000000, // QEMU default
        0x44000000, // Alternative location
        0x48000000, // Another common location
    ];
    
    for &addr in &possible_addresses {
        unsafe {
            let magic_ptr = addr as *const u32;
            if read_be(magic_ptr) == FDT_MAGIC {
                return addr;
            }
        }
    }
    
    0 // Not found
}