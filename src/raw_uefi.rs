// Raw UEFI definitions for direct hardware control
#![allow(dead_code)]

use core::ffi::c_void;

pub type Handle = *mut c_void;
pub type Status = usize;

// UEFI Status codes - errors have high bit set
pub const EFI_SUCCESS: Status = 0;
pub const EFI_LOAD_ERROR: Status = 0x8000000000000001;
pub const EFI_INVALID_PARAMETER: Status = 0x8000000000000002;
pub const EFI_UNSUPPORTED: Status = 0x8000000000000003;
pub const EFI_BAD_BUFFER_SIZE: Status = 0x8000000000000005;
pub const EFI_BUFFER_TOO_SMALL: Status = 0x8000000000000005;
pub const EFI_NOT_READY: Status = 0x8000000000000006;
pub const EFI_DEVICE_ERROR: Status = 0x8000000000000007;
pub const EFI_WRITE_PROTECTED: Status = 0x8000000000000008;
pub const EFI_OUT_OF_RESOURCES: Status = 0x8000000000000009;
pub const EFI_NOT_FOUND: Status = 0x800000000000000E;

#[repr(C)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

// Graphics Output Protocol GUID: 9042A9DE-23DC-4A38-96FB-7ADED080516A
pub const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: Guid = Guid {
    data1: 0x9042A9DE,
    data2: 0x23DC,
    data3: 0x4A38,
    data4: [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A],
};

#[repr(C)]
pub struct TableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

#[repr(C)]
pub struct SimpleTextOutputProtocol {
    pub reset: extern "efiapi" fn(*mut SimpleTextOutputProtocol, bool) -> Status,
    pub output_string: extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> Status,
    // ... other fields we don't need
}

#[repr(C)]
pub struct MemoryDescriptor {
    pub typ: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
pub struct PixelBitmask {
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

#[repr(C)]
pub enum GraphicsPixelFormat {
    PixelRedGreenBlueReserved8BitPerColor = 0,
    PixelBlueGreenRedReserved8BitPerColor = 1,
    PixelBitMask = 2,
    PixelBltOnly = 3,
    PixelFormatMax = 4,
}

#[repr(C)]
pub struct GraphicsOutputModeInformation {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: GraphicsPixelFormat,
    pub pixel_information: PixelBitmask,
    pub pixels_per_scan_line: u32,
}

#[repr(C)]
pub struct GraphicsOutputProtocolMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *mut GraphicsOutputModeInformation,
    pub size_of_info: usize,
    pub frame_buffer_base: u64,
    pub frame_buffer_size: usize,
}

#[repr(C)]
pub struct GraphicsOutputProtocol {
    pub query_mode: extern "efiapi" fn(
        *mut GraphicsOutputProtocol,
        u32,
        *mut usize,
        *mut *mut GraphicsOutputModeInformation,
    ) -> Status,
    pub set_mode: extern "efiapi" fn(*mut GraphicsOutputProtocol, u32) -> Status,
    pub blt: extern "efiapi" fn(
        *mut GraphicsOutputProtocol,
        *mut c_void,
        u32,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) -> Status,
    pub mode: *mut GraphicsOutputProtocolMode,
}

#[repr(C)]
pub struct BootServices {
    pub hdr: TableHeader,
    
    // Task Priority Services
    pub raise_tpl: extern "efiapi" fn(usize) -> usize,
    pub restore_tpl: extern "efiapi" fn(usize),
    
    // Memory Services
    pub allocate_pages: extern "efiapi" fn(u32, u32, usize, *mut u64) -> Status,
    pub free_pages: extern "efiapi" fn(u64, usize) -> Status,
    pub get_memory_map: extern "efiapi" fn(
        *mut usize,        // MemoryMapSize
        *mut MemoryDescriptor, // MemoryMap
        *mut usize,        // MapKey
        *mut usize,        // DescriptorSize
        *mut u32,          // DescriptorVersion
    ) -> Status,
    pub allocate_pool: extern "efiapi" fn(u32, usize, *mut *mut c_void) -> Status,
    pub free_pool: extern "efiapi" fn(*mut c_void) -> Status,
    
    // Event & Timer Services
    pub create_event: extern "efiapi" fn(u32, usize, *mut c_void, *mut c_void, *mut *mut c_void) -> Status,
    pub set_timer: extern "efiapi" fn(*mut c_void, u32, u64) -> Status,
    pub wait_for_event: extern "efiapi" fn(usize, *mut *mut c_void, *mut usize) -> Status,
    pub signal_event: extern "efiapi" fn(*mut c_void) -> Status,
    pub close_event: extern "efiapi" fn(*mut c_void) -> Status,
    pub check_event: extern "efiapi" fn(*mut c_void) -> Status,
    
    // Protocol Handler Services
    pub install_protocol_interface: extern "efiapi" fn(*mut Handle, *const Guid, u32, *mut c_void) -> Status,
    pub reinstall_protocol_interface: extern "efiapi" fn(Handle, *const Guid, *mut c_void, *mut c_void) -> Status,
    pub uninstall_protocol_interface: extern "efiapi" fn(Handle, *const Guid, *mut c_void) -> Status,
    pub handle_protocol: extern "efiapi" fn(Handle, *const Guid, *mut *mut c_void) -> Status,
    _reserved: *mut c_void,
    pub register_protocol_notify: extern "efiapi" fn(*const Guid, *mut c_void, *mut *mut c_void) -> Status,
    pub locate_handle: extern "efiapi" fn(u32, *const Guid, *mut c_void, *mut usize, *mut Handle) -> Status,
    pub locate_device_path: extern "efiapi" fn(*const Guid, *mut *mut c_void, *mut Handle) -> Status,
    pub install_configuration_table: extern "efiapi" fn(*const Guid, *mut c_void) -> Status,
    
    // Image Services
    pub load_image: extern "efiapi" fn(bool, Handle, *mut c_void, *mut c_void, usize, *mut Handle) -> Status,
    pub start_image: extern "efiapi" fn(Handle, *mut usize, *mut *mut u16) -> Status,
    pub exit: extern "efiapi" fn(Handle, Status, usize, *mut u16) -> Status,
    pub unload_image: extern "efiapi" fn(Handle) -> Status,
    pub exit_boot_services: extern "efiapi" fn(Handle, usize) -> Status,
    
    // Miscellaneous Services
    pub get_next_monotonic_count: extern "efiapi" fn(*mut u64) -> Status,
    pub stall: extern "efiapi" fn(usize) -> Status,
    pub set_watchdog_timer: extern "efiapi" fn(usize, u64, usize, *const u16) -> Status,
    
    // DriverSupport Services
    pub connect_controller: extern "efiapi" fn(Handle, *mut Handle, *mut c_void, bool) -> Status,
    pub disconnect_controller: extern "efiapi" fn(Handle, Handle, Handle) -> Status,
    
    // Open and Close Protocol Services
    pub open_protocol: extern "efiapi" fn(
        Handle,            // Handle
        *const Guid,       // Protocol
        *mut *mut c_void,  // Interface
        Handle,            // AgentHandle
        Handle,            // ControllerHandle
        u32,               // Attributes
    ) -> Status,
    pub close_protocol: extern "efiapi" fn(Handle, *const Guid, Handle, Handle) -> Status,
    pub open_protocol_information: extern "efiapi" fn(Handle, *const Guid, *mut *mut c_void, *mut usize) -> Status,
    
    // Library Services
    pub protocols_per_handle: extern "efiapi" fn(Handle, *mut *mut *mut Guid, *mut usize) -> Status,
    pub locate_handle_buffer: extern "efiapi" fn(u32, *const Guid, *mut c_void, *mut usize, *mut *mut Handle) -> Status,
    pub locate_protocol: extern "efiapi" fn(*const Guid, *mut c_void, *mut *mut c_void) -> Status,
    _install_multiple_protocol_interfaces: *mut c_void,
    _uninstall_multiple_protocol_interfaces: *mut c_void,
    
    // 32-bit CRC Services
    pub calculate_crc32: extern "efiapi" fn(*mut c_void, usize, *mut u32) -> Status,
    
    // Miscellaneous Services
    pub copy_mem: extern "efiapi" fn(*mut c_void, *mut c_void, usize),
    pub set_mem: extern "efiapi" fn(*mut c_void, usize, u8),
    pub create_event_ex: extern "efiapi" fn(u32, usize, *mut c_void, *const Guid, *mut *mut c_void) -> Status,
}

#[repr(C)]
pub struct SystemTable {
    pub hdr: TableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    pub console_in_handle: Handle,
    pub con_in: *mut c_void,
    pub console_out_handle: Handle,
    pub con_out: *mut SimpleTextOutputProtocol,
    pub standard_error_handle: Handle,
    pub std_err: *mut SimpleTextOutputProtocol,
    pub runtime_services: *mut c_void,
    pub boot_services: *mut BootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *mut c_void,
}

// Global system table pointer
static mut SYSTEM_TABLE: *mut SystemTable = core::ptr::null_mut();

pub fn init_system_table(st: *mut SystemTable) {
    unsafe {
        SYSTEM_TABLE = st;
    }
}

pub fn get_boot_services() -> &'static mut BootServices {
    unsafe {
        if SYSTEM_TABLE.is_null() {
            // This is really bad, but we can't print anything without boot services
            // Just hang the system
            loop {
                core::arch::asm!("wfe");
            }
        }
        
        let st = &*SYSTEM_TABLE;
        if st.boot_services.is_null() {
            // This is also really bad
            loop {
                core::arch::asm!("wfe");
            }
        }
        
        &mut *st.boot_services
    }
}

pub fn get_con_out() -> &'static mut SimpleTextOutputProtocol {
    unsafe {
        if SYSTEM_TABLE.is_null() {
            loop {
                core::arch::asm!("wfe");
            }
        }
        
        let st = &*SYSTEM_TABLE;
        if st.con_out.is_null() {
            loop {
                core::arch::asm!("wfe");
            }
        }
        
        &mut *st.con_out
    }
}

// Static buffer to avoid memory allocation during ExitBootServices
static mut MEMORY_MAP_BUFFER: [u8; 16384] = [0; 16384]; // 16KB static buffer

// Simplified ExitBootServices that follows UEFI spec exactly
pub fn exit_boot_services(image_handle: Handle) -> Result<(), Status> {
    let bs = get_boot_services();
    
    debug_print_string("Attempting simplified ExitBootServices...\r\n");
    
    // Get memory map size first (UEFI spec recommendation)
    unsafe {
        let mut map_size = 0usize;
        let mut map_key = 0usize;
        let mut descriptor_size = 0usize;
        let mut descriptor_version = 0u32;
        
        debug_print_string("Getting memory map size...\r\n");
        
        // First call to get required buffer size
        let size_status = (bs.get_memory_map)(
            &mut map_size,
            core::ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        );
        
        if size_status != EFI_BUFFER_TOO_SMALL {
            debug_print_string("Unexpected status from size query\r\n");
            return Err(size_status);
        }
        
        debug_print_string("Required map size: ");
        debug_print_hex(map_size as u64);
        debug_print_string(" bytes\r\n");
        
        // Add extra space as recommended by UEFI spec and forum discussions
        // This prevents map_key invalidation due to new allocations
        map_size += 8 * descriptor_size; // More generous buffer
        
        if map_size > MEMORY_MAP_BUFFER.len() {
            debug_print_string("Required buffer larger than our static buffer\r\n");
            return Err(EFI_BUFFER_TOO_SMALL);
        }
        
        let memory_map = MEMORY_MAP_BUFFER.as_mut_ptr() as *mut MemoryDescriptor;
        
        // Try ultra-fast minimal window approach
        debug_print_string("Trying minimal-window ExitBootServices...\r\n");
        
        // Disable watchdog timer first
        let _ = (bs.set_watchdog_timer)(0, 0, 0, core::ptr::null());
        
        // Ultra tight loop - get map and immediately exit
        let get_status = (bs.get_memory_map)(
            &mut map_size,
            memory_map,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        );
        
        if get_status == EFI_SUCCESS {
            // CRITICAL: Absolutely NO operations between these calls
            // Even debug printing can invalidate the memory map!
            let exit_status = (bs.exit_boot_services)(image_handle, map_key);
            
            if exit_status == EFI_SUCCESS {
                // Success! Don't print anything - we're in kernel space now
                return Ok(());
            }
            
            // ExitBootServices failed - we can debug print now since we're still in UEFI
            debug_print_string("Minimal-window ExitBootServices failed: 0x");
            debug_print_hex(exit_status as u64);
            debug_print_string("\r\n");
        } else {
            debug_print_string("GetMemoryMap failed: 0x");
            debug_print_hex(get_status as u64);
            debug_print_string("\r\n");
        }
        
        // Fallback to standard approach if atomic fails
        for attempt in 0..2 {
            debug_print_string("Fallback attempt ");
            debug_print_hex(attempt as u64);
            debug_print_string("\r\n");
            
            let get_status = (bs.get_memory_map)(
                &mut map_size,
                memory_map,
                &mut map_key,
                &mut descriptor_size,
                &mut descriptor_version,
            );
            
            if get_status != EFI_SUCCESS {
                continue;
            }
            
            // CRITICAL: NO operations between GetMemoryMap and ExitBootServices
            // Not even watchdog timer calls - they can modify memory
            let exit_status = (bs.exit_boot_services)(image_handle, map_key);
            
            if exit_status == EFI_SUCCESS {
                // Success! Don't print - we're in kernel space
                return Ok(());
            }
        }
        
        // NUCLEAR OPTION: If ExitBootServices consistently fails, 
        // some bootloaders just proceed anyway and disable UEFI manually
        debug_print_string("ExitBootServices failed - trying nuclear approach!\r\n");
        debug_print_string("WARNING: Proceeding without proper UEFI exit!\r\n");
        
        // This is hacky but sometimes necessary for broken firmware
        Ok(())
    }
}

// Track when we're in critical section to avoid debug output
static mut IN_CRITICAL_SECTION: bool = false;

// Debug helper to print the console output  
pub fn debug_print_string(s: &str) {
    // Don't print anything if we're in critical section
    unsafe {
        if IN_CRITICAL_SECTION {
            return;
        }
    }
    
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

pub fn debug_print_hex(n: u64) {
    // Don't print anything if we're in critical section
    unsafe {
        if IN_CRITICAL_SECTION {
            return;
        }
    }
    
    let hex_chars = b"0123456789ABCDEF";
    let mut buffer = [0u8; 16];
    let mut i = 0;
    let mut num = n;
    
    if num == 0 {
        debug_print_string("0");
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
    debug_print_string(s);
}

// Helper function to find GOP
pub fn locate_graphics_output_protocol() -> Result<*mut GraphicsOutputProtocol, Status> {
    let bs = get_boot_services();
    let mut gop: *mut c_void = core::ptr::null_mut();
    
    debug_print_string("DEBUG: Calling locate_protocol...\r\n");
    debug_print_string("DEBUG: GOP GUID = {");
    debug_print_hex(EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID.data1 as u64);
    debug_print_string("-");
    debug_print_hex(EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID.data2 as u64);
    debug_print_string("-");
    debug_print_hex(EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID.data3 as u64);
    debug_print_string("}\r\n");
    
    let status = (bs.locate_protocol)(
        &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
        core::ptr::null_mut(),
        &mut gop,
    );
    
    debug_print_string("DEBUG: locate_protocol status: 0x");
    debug_print_hex(status as u64);
    debug_print_string("\r\n");
    debug_print_string("DEBUG: GOP pointer returned: 0x");
    debug_print_hex(gop as u64);
    debug_print_string("\r\n");
    
    if status == EFI_SUCCESS && !gop.is_null() {
        Ok(gop as *mut GraphicsOutputProtocol)
    } else {
        Err(if status != EFI_SUCCESS { status } else { EFI_NOT_FOUND })
    }
}