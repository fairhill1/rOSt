#![no_std]
#![no_main]

extern crate alloc;
use alloc::vec::Vec;
use librost::*;
use librost::ipc_protocol::*;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

// ============================================================================
// Bump Allocator for Userspace
// ============================================================================

const HEAP_SIZE: usize = 128 * 1024; // 128KB heap

struct BumpAllocator {
    heap: UnsafeCell<[u8; HEAP_SIZE]>,
    next: UnsafeCell<usize>,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap: UnsafeCell::new([0; HEAP_SIZE]),
            next: UnsafeCell::new(0),
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        let next = *self.next.get();
        let aligned = (next + align - 1) & !(align - 1);
        let new_next = aligned + size;

        if new_next > HEAP_SIZE {
            return core::ptr::null_mut();
        }

        *self.next.get() = new_next;
        self.heap.get().cast::<u8>().add(aligned)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

// ============================================================================
// Filesystem Constants
// ============================================================================

const SECTOR_SIZE: usize = 512;
const FS_MAGIC: u32 = 0x524F5354; // "ROST" in ASCII
const FS_VERSION: u32 = 1;
const DATA_START_SECTOR: u64 = 11;
const MAX_FILES: usize = 32;
const FILE_TABLE_SECTORS: u64 = 2;

// ============================================================================
// Block Device Abstraction
// ============================================================================

/// Abstract block device interface - can be implemented for kernel or userspace
trait BlockDevice {
    fn read_block(&mut self, sector: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str>;
    fn write_block(&mut self, sector: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), &'static str>;
}

/// Userspace block device that uses syscalls
struct UserSpaceBlockDevice {
    device_id: u32,  // VirtIO block device ID (0 = first device)
}

impl UserSpaceBlockDevice {
    fn new(device_id: u32) -> Self {
        Self { device_id }
    }
}

impl BlockDevice for UserSpaceBlockDevice {
    fn read_block(&mut self, sector: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        let result = librost::read_block(self.device_id, sector as u32, buffer);
        if result == 0 {
            Ok(())
        } else {
            Err("Block read failed")
        }
    }

    fn write_block(&mut self, sector: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        let result = librost::write_block(self.device_id, sector as u32, buffer);
        if result == 0 {
            Ok(())
        } else {
            Err("Block write failed")
        }
    }
}

// ============================================================================
// SimpleFilesystem (ported from kernel)
// ============================================================================

/// Filesystem superblock (stored in sector 0)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Superblock {
    magic: u32,
    version: u32,
    total_sectors: u64,
    data_start_sector: u64,
    file_count: u32,
    reserved: [u8; 480],
}

/// File table entry (20 bytes each - corrected from 16 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct FileEntry {
    name: [u8; 8],
    start_sector: u16,
    size_sectors: u16,
    size_bytes: u32,
    flags: u8,
    reserved: [u8; 3],
}

impl FileEntry {
    const FLAG_USED: u8 = 0x01;
    const FLAG_FREE: u8 = 0x00;

    fn is_used(&self) -> bool {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(self.flags)) == Self::FLAG_USED }
    }

    fn is_free(&self) -> bool {
        !self.is_used()
    }

    fn get_name(&self) -> &str {
        unsafe {
            let name_bytes = &*(core::ptr::addr_of!(self.name) as *const [u8; 8]);
            let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            core::str::from_utf8(&name_bytes[..len]).unwrap_or("")
        }
    }

    fn get_size_bytes(&self) -> u32 {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(self.size_bytes)) }
    }

    fn get_start_sector(&self) -> u16 {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(self.start_sector)) }
    }

    fn get_size_sectors(&self) -> u16 {
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!(self.size_sectors)) }
    }
}

/// Simple filesystem implementation
struct SimpleFilesystem {
    superblock: Superblock,
    file_table: [FileEntry; MAX_FILES],
    next_free_sector: u64,
}

impl SimpleFilesystem {
    /// Mount an existing filesystem
    fn mount<D: BlockDevice>(device: &mut D) -> Result<Self, &'static str> {
        // Read superblock from sector 0
        let mut sector_buffer = [0u8; SECTOR_SIZE];
        device.read_block(0, &mut sector_buffer)?;

        let superblock: Superblock = unsafe {
            core::ptr::read_volatile(sector_buffer.as_ptr() as *const Superblock)
        };

        // Validate magic number
        if superblock.magic != FS_MAGIC {
            return Err("Invalid filesystem magic number");
        }

        // Read file table from sectors 1-2
        let mut file_table = [FileEntry {
            name: [0; 8],
            start_sector: 0,
            size_sectors: 0,
            size_bytes: 0,
            flags: FileEntry::FLAG_FREE,
            reserved: [0; 3],
        }; MAX_FILES];

        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            device.read_block(1 + sector, &mut sector_buffer)?;

            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                file_table[i] = unsafe {
                    core::ptr::read_volatile(sector_buffer.as_ptr().add(offset) as *const FileEntry)
                };
            }
        }

        // Calculate next free sector
        let mut next_free_sector = DATA_START_SECTOR;
        for entry in &file_table {
            if entry.is_used() {
                let entry_end = entry.get_start_sector() as u64 + entry.get_size_sectors() as u64;
                if entry_end > next_free_sector {
                    next_free_sector = entry_end;
                }
            }
        }

        Ok(Self {
            superblock,
            file_table,
            next_free_sector,
        })
    }

    /// Format a new filesystem on the device
    fn format<D: BlockDevice>(device: &mut D, total_sectors: u64) -> Result<Self, &'static str> {
        print_debug("Formatting new filesystem...\r\n");

        // Create empty file table
        let file_table = [FileEntry {
            name: [0; 8],
            start_sector: 0,
            size_sectors: 0,
            size_bytes: 0,
            flags: FileEntry::FLAG_FREE,
            reserved: [0; 3],
        }; MAX_FILES];

        // Create superblock
        let superblock = Superblock {
            magic: FS_MAGIC,
            version: FS_VERSION,
            total_sectors,
            data_start_sector: DATA_START_SECTOR,
            file_count: 0,
            reserved: [0; 480],
        };

        // Write superblock to sector 0
        let mut sector_buffer = [0u8; SECTOR_SIZE];
        unsafe {
            core::ptr::write_volatile(sector_buffer.as_mut_ptr() as *mut Superblock, superblock);
        }
        device.write_block(0, &sector_buffer)?;

        // Write empty file table to sectors 1-2
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            sector_buffer = [0u8; SECTOR_SIZE];
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    core::ptr::write_volatile(
                        sector_buffer.as_mut_ptr().add(offset) as *mut FileEntry,
                        file_table[i]
                    );
                }
            }

            device.write_block(1 + sector, &sector_buffer)?;
        }

        print_debug("Filesystem formatted successfully!\r\n");

        Ok(Self {
            superblock,
            file_table,
            next_free_sector: DATA_START_SECTOR,
        })
    }

    /// List all files in the filesystem
    fn list_files(&self) -> Vec<&str> {
        let mut files = Vec::new();
        for entry in &self.file_table {
            if entry.is_used() {
                files.push(entry.get_name());
            }
        }
        files
    }

    /// Find file entry by name
    fn find_file(&self, name: &str) -> Option<&FileEntry> {
        self.file_table.iter().find(|entry| {
            entry.is_used() && entry.get_name() == name
        })
    }

    /// Read file contents
    fn read_file<D: BlockDevice>(&self, device: &mut D, name: &str, buffer: &mut [u8]) -> Result<usize, &'static str> {
        let entry = self.find_file(name).ok_or("File not found")?;

        let size = entry.get_size_bytes() as usize;
        if size > buffer.len() {
            return Err("Buffer too small");
        }

        let start_sector = entry.get_start_sector() as u64;
        let size_sectors = entry.get_size_sectors() as usize;

        let mut sector_buffer = [0u8; SECTOR_SIZE];
        for i in 0..size_sectors {
            device.read_block(DATA_START_SECTOR + start_sector as u64 + i as u64, &mut sector_buffer)?;

            let offset = i * SECTOR_SIZE;
            let to_copy = core::cmp::min(SECTOR_SIZE, size - offset);
            buffer[offset..offset + to_copy].copy_from_slice(&sector_buffer[..to_copy]);
        }

        Ok(size)
    }

    /// Create a new file
    fn create_file<D: BlockDevice>(&mut self, device: &mut D, name: &str, size: u32) -> Result<(), &'static str> {
        // Check if file already exists
        if self.find_file(name).is_some() {
            return Err("File already exists");
        }

        // Find free file table entry
        let mut free_entry_idx = None;
        for (i, entry) in self.file_table.iter().enumerate() {
            if entry.is_free() {
                free_entry_idx = Some(i);
                break;
            }
        }

        let entry_idx = free_entry_idx.ok_or("File table full")?;

        // Calculate sectors needed
        let size_sectors = ((size as usize + SECTOR_SIZE - 1) / SECTOR_SIZE) as u16;
        let start_sector = (self.next_free_sector - DATA_START_SECTOR) as u16;

        // Create file entry
        let mut name_bytes = [0u8; 8];
        let name_len = core::cmp::min(name.len(), 8);
        name_bytes[..name_len].copy_from_slice(&name.as_bytes()[..name_len]);

        let new_entry = FileEntry {
            name: name_bytes,
            start_sector,
            size_sectors,
            size_bytes: size,
            flags: FileEntry::FLAG_USED,
            reserved: [0; 3],
        };

        // Write file entry to table
        self.file_table[entry_idx] = new_entry;

        // Update superblock file count
        self.superblock.file_count += 1;

        // Write superblock to disk
        let mut sector_buffer = [0u8; SECTOR_SIZE];
        unsafe {
            core::ptr::write_volatile(sector_buffer.as_mut_ptr() as *mut Superblock, self.superblock);
        }
        device.write_block(0, &sector_buffer)?;

        // Write file table to disk
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            sector_buffer = [0u8; SECTOR_SIZE];
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    core::ptr::write_volatile(
                        sector_buffer.as_mut_ptr().add(offset) as *mut FileEntry,
                        self.file_table[i]
                    );
                }
            }

            device.write_block(1 + sector, &sector_buffer)?;
        }

        // Zero out file data sectors
        sector_buffer = [0u8; SECTOR_SIZE];
        for i in 0..size_sectors {
            device.write_block(self.next_free_sector + i as u64, &sector_buffer)?;
        }

        // Update next free sector
        self.next_free_sector += size_sectors as u64;

        Ok(())
    }

    /// Write data to an existing file
    fn write_file<D: BlockDevice>(&mut self, device: &mut D, name: &str, data: &[u8]) -> Result<usize, &'static str> {
        // Find file entry
        let entry_idx = self.file_table.iter().position(|e| e.is_used() && e.get_name() == name)
            .ok_or("File not found")?;

        let entry = &self.file_table[entry_idx];
        let max_size = (entry.get_size_sectors() as usize) * SECTOR_SIZE;

        if data.len() > max_size {
            return Err("Data too large for file");
        }

        let start_sector = entry.get_start_sector() as u64;
        let size_sectors = ((data.len() + SECTOR_SIZE - 1) / SECTOR_SIZE) as u16;

        // Write data sectors
        let mut sector_buffer = [0u8; SECTOR_SIZE];
        for i in 0..size_sectors as usize {
            sector_buffer = [0u8; SECTOR_SIZE];
            let offset = i * SECTOR_SIZE;
            let to_copy = core::cmp::min(SECTOR_SIZE, data.len() - offset);
            sector_buffer[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);
            device.write_block(DATA_START_SECTOR + start_sector + i as u64, &sector_buffer)?;
        }

        // Update file entry with actual size
        self.file_table[entry_idx].size_bytes = data.len() as u32;

        // Write updated file table to disk
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            sector_buffer = [0u8; SECTOR_SIZE];
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    core::ptr::write_volatile(
                        sector_buffer.as_mut_ptr().add(offset) as *mut FileEntry,
                        self.file_table[i]
                    );
                }
            }

            device.write_block(1 + sector, &sector_buffer)?;
        }

        Ok(data.len())
    }
}

// ============================================================================
// File Server Main Loop
// ============================================================================

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_debug("=== rOSt File Server (EL0) ===");
    print_debug("Initializing...");

    print_debug("Creating block device...\r\n");
    let mut device = UserSpaceBlockDevice::new(0); // First VirtIO block device

    print_debug("Attempting to mount filesystem...\r\n");
    let mut fs = match SimpleFilesystem::mount(&mut device) {
        Ok(fs) => {
            print_debug("Filesystem mounted successfully\r\n");
            fs
        }
        Err(e) => {
            print_debug("Failed to mount filesystem: ");
            print_debug(e);
            print_debug("\r\n");

            // Try to format the disk instead
            print_debug("Attempting to format disk...\r\n");
            match SimpleFilesystem::format(&mut device, 20480) { // 10MB = 20480 sectors
                Ok(fs) => {
                    print_debug("Disk formatted successfully\r\n");
                    fs
                }
                Err(format_err) => {
                    print_debug("Failed to format disk: ");
                    print_debug(format_err);
                    print_debug("\r\nFile server will exit\r\n");
                    exit(1);
                }
            }
        }
    };

    print_debug("File server ready, waiting for requests...");

    let my_pid = getpid();
    print_debug("File server PID: ");
    if my_pid < 10 {
        let pid_str = [b'0' + my_pid as u8];
        print_debug(core::str::from_utf8(&pid_str).unwrap());
    }

    // Main IPC message loop
    let mut msg_buf = [0u8; 256];
    let mut request_count = 0u32;
    loop {
        // Wait for messages (1 second timeout) and get sender PID
        let mut sender_pid: u32 = 0;
        let result = recv_message_from(&mut msg_buf, 1000, &mut sender_pid as *mut u32);

        if result > 0 {
            request_count += 1;
            print_debug("File server: received request #");
            if request_count < 10 {
                let count_str = [b'0' + request_count as u8];
                print_debug(core::str::from_utf8(&count_str).unwrap());
            }

            // Parse and handle request
            if let Some(request) = AppToFS::from_bytes(&msg_buf) {
                handle_request(&mut fs, &mut device, request, sender_pid as usize);
            } else {
                print_debug("File server: failed to parse request");
            }
        }

        // Yield CPU to other processes
        yield_now();
    }
}

/// Handle a single filesystem request
fn handle_request<D: BlockDevice>(fs: &mut SimpleFilesystem, device: &mut D, request: AppToFS, sender_pid: usize) {
    match request {
        AppToFS::List(msg) => {
            let request_id = msg.request_id;
            print_debug("File server: handling List request");

            // List all files
            let files = fs.list_files();

            print_debug("File server: found files count: ");
            if files.len() < 10 {
                let count_str = [b'0' + files.len() as u8];
                print_debug(core::str::from_utf8(&count_str).unwrap());
            }

            // Build newline-separated list
            let mut files_data = [0u8; 200];
            let mut pos = 0;
            for (i, filename) in files.iter().enumerate() {
                print_debug("File server: processing filename: '");
                print_debug(filename);
                print_debug("' len=");
                if filename.len() < 10 {
                    let len_str = [b'0' + filename.len() as u8];
                    print_debug(core::str::from_utf8(&len_str).unwrap());
                }
                print_debug("\r\n");

                let bytes = filename.as_bytes();
                if pos + bytes.len() + 1 > files_data.len() {
                    break; // Out of space
                }
                files_data[pos..pos + bytes.len()].copy_from_slice(bytes);
                pos += bytes.len();
                if i < files.len() - 1 {
                    files_data[pos] = b'\n';
                    pos += 1;
                }
            }

            // Debug: print pos value
            print_debug("File server: files_len (pos) = ");
            if pos < 10 {
                let pos_str = [b'0' + pos as u8];
                print_debug(core::str::from_utf8(&pos_str).unwrap());
            } else {
                print_debug(">=10");
            }
            print_debug("\r\n");

            let response = FSToApp::ListResponse(FSListResponseMsg {
                msg_type: msg_types::FS_LIST_RESPONSE,
                has_more: 0,  // false
                _pad1: [0; 2],
                request_id,
                files_len: pos,
                files: files_data,
            });

            print_debug("File server: sending response to PID ");
            if sender_pid < 10 {
                let pid_str = [b'0' + sender_pid as u8];
                print_debug(core::str::from_utf8(&pid_str).unwrap());
            }

            let response_bytes = response.to_bytes();
            let result = send_message(sender_pid as u32, &response_bytes);

            if result < 0 {
                print_debug("File server: ERROR sending message!");
            } else {
                print_debug("File server: response sent successfully");
            }
        }
        AppToFS::Open(msg) => {
            // TODO: Implement open
            let response = FSToApp::Error(FSErrorMsg {
                msg_type: msg_types::FS_ERROR,
                _pad1: [0; 3],
                request_id: msg.request_id,
                error_code: -99, // Not implemented
            });
            send_message(sender_pid as u32, &response.to_bytes());
        }
        AppToFS::Read(msg) => {
            // TODO: Implement read
            let response = FSToApp::Error(FSErrorMsg {
                msg_type: msg_types::FS_ERROR,
                _pad1: [0; 3],
                request_id: msg.request_id,
                error_code: -99,
            });
            send_message(sender_pid as u32, &response.to_bytes());
        }
        AppToFS::Create(msg) => {
            let request_id = msg.request_id;
            let filename = msg.filename;
            let filename_len = msg.filename_len;
            let size = msg.size;
            print_debug("File server: handling Create request\r\n");

            // Convert filename bytes to str
            let name = core::str::from_utf8(&filename[..filename_len])
                .unwrap_or("");

            // Create the file
            match fs.create_file(device, name, size) {
                Ok(()) => {
                    print_debug("File server: file created successfully\r\n");
                    let response = FSToApp::CreateSuccess(FSCreateSuccessMsg {
                        msg_type: msg_types::FS_CREATE_SUCCESS,
                        _pad1: [0; 3],
                        request_id,
                    });
                    send_message(sender_pid as u32, &response.to_bytes());
                }
                Err(e) => {
                    print_debug("File server: create failed: ");
                    print_debug(e);
                    print_debug("\r\n");
                    let response = FSToApp::Error(FSErrorMsg {
                        msg_type: msg_types::FS_ERROR,
                        _pad1: [0; 3],
                        request_id,
                        error_code: -1,
                    });
                    send_message(sender_pid as u32, &response.to_bytes());
                }
            }
        }
        AppToFS::Write(msg) => {
            print_debug("File server: handling Write request\r\n");

            // For now, assume fd=0 means we're writing to the last pending file
            // This is a simplification - proper implementation would track open file descriptors
            // Extract filename from pending write operation (terminal stores it)
            // For this simple implementation, we need the terminal to send filename in a different way
            // Let's just send an error for now until we implement proper file descriptors

            let response = FSToApp::Error(FSErrorMsg {
                msg_type: msg_types::FS_ERROR,
                _pad1: [0; 3],
                request_id: msg.request_id,
                error_code: -98, // Need Open first
            });
            send_message(sender_pid as u32, &response.to_bytes());
        }
        _ => {
            // Other operations not yet implemented
            print_debug("Unhandled file server request\r\n");
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    print_debug("PANIC in file_server!");
    exit(1);
}
