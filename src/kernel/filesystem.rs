// Simple Custom Filesystem for rOSt
//
// Disk Layout:
// - Sector 0: Superblock (filesystem metadata)
// - Sector 1: File Table (up to 32 files)
// - Sectors 2-9: Reserved for future use
// - Sectors 10+: Data blocks

use crate::kernel::virtio_blk::VirtioBlkDevice;
use core::ptr;
extern crate alloc;

const FS_MAGIC: u32 = 0x524F5354; // "ROST" in ASCII
const FS_VERSION: u32 = 1;
const DATA_START_SECTOR: u64 = 10;
const MAX_FILES: usize = 32;
const SECTOR_SIZE: usize = 512;

/// Filesystem superblock (stored in sector 0)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Superblock {
    magic: u32,              // Magic number for validation
    version: u32,            // Filesystem version
    total_sectors: u64,      // Total sectors on disk
    data_start_sector: u64,  // First data sector (10)
    file_count: u32,         // Number of files currently stored
    reserved: [u8; 480],     // Padding to fill 512 bytes
}

/// File table entry (16 bytes each)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FileEntry {
    name: [u8; 8],       // 8-character filename (null-terminated)
    start_sector: u16,   // First data sector for this file
    size_sectors: u16,   // Size in sectors
    size_bytes: u32,     // Actual size in bytes
    flags: u8,           // 0x01 = used, 0x00 = free
    reserved: [u8; 3],   // Future use
}

impl FileEntry {
    const FLAG_USED: u8 = 0x01;
    const FLAG_FREE: u8 = 0x00;

    pub fn is_used(&self) -> bool {
        unsafe { ptr::read_volatile(ptr::addr_of!(self.flags)) == Self::FLAG_USED }
    }

    pub fn is_free(&self) -> bool {
        !self.is_used()
    }

    pub fn get_name(&self) -> &str {
        unsafe {
            let name_bytes = &*(ptr::addr_of!(self.name) as *const [u8; 8]);
            // Find null terminator
            let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
            core::str::from_utf8(&name_bytes[..len]).unwrap_or("")
        }
    }

    pub fn get_size_bytes(&self) -> u32 {
        unsafe { ptr::read_volatile(ptr::addr_of!(self.size_bytes)) }
    }

    pub fn get_start_sector(&self) -> u16 {
        unsafe { ptr::read_volatile(ptr::addr_of!(self.start_sector)) }
    }

    pub fn get_size_sectors(&self) -> u16 {
        unsafe { ptr::read_volatile(ptr::addr_of!(self.size_sectors)) }
    }
}

/// Simple filesystem implementation
pub struct SimpleFilesystem {
    superblock: Superblock,
    file_table: [FileEntry; MAX_FILES],
    next_free_sector: u64,  // Track next available data sector
}

impl SimpleFilesystem {
    /// Format a disk with the simple filesystem
    pub fn format(device: &mut VirtioBlkDevice, total_sectors: u64) -> Result<(), &'static str> {
        crate::kernel::uart_write_string("Formatting disk with SimpleFS...\r\n");

        // Create superblock
        let mut superblock = Superblock {
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
            ptr::copy_nonoverlapping(
                &superblock as *const Superblock as *const u8,
                sector_buffer.as_mut_ptr(),
                core::mem::size_of::<Superblock>(),
            );
        }
        device.write_sector(0, &sector_buffer)?;
        crate::kernel::uart_write_string("Superblock written to sector 0\r\n");

        // Create empty file table
        let empty_entry = FileEntry {
            name: [0; 8],
            start_sector: 0,
            size_sectors: 0,
            size_bytes: 0,
            flags: FileEntry::FLAG_FREE,
            reserved: [0; 3],
        };

        // Write file table to sector 1
        sector_buffer = [0u8; SECTOR_SIZE];
        for i in 0..MAX_FILES {
            let offset = i * core::mem::size_of::<FileEntry>();
            unsafe {
                ptr::copy_nonoverlapping(
                    &empty_entry as *const FileEntry as *const u8,
                    sector_buffer.as_mut_ptr().add(offset),
                    core::mem::size_of::<FileEntry>(),
                );
            }
        }
        device.write_sector(1, &sector_buffer)?;
        crate::kernel::uart_write_string("File table initialized at sector 1\r\n");

        crate::kernel::uart_write_string("Format complete!\r\n");
        Ok(())
    }

    /// Mount an existing filesystem
    pub fn mount(device: &mut VirtioBlkDevice) -> Result<Self, &'static str> {
        crate::kernel::uart_write_string("Mounting SimpleFS...\r\n");

        // Read superblock from sector 0
        let mut sector_buffer = [0u8; SECTOR_SIZE];
        device.read_sector(0, &mut sector_buffer)?;

        let superblock: Superblock = unsafe {
            ptr::read_unaligned(sector_buffer.as_ptr() as *const Superblock)
        };

        // Validate magic number
        let magic = unsafe { ptr::read_volatile(ptr::addr_of!(superblock.magic)) };
        if magic != FS_MAGIC {
            crate::kernel::uart_write_string(&alloc::format!(
                "ERROR: Invalid magic number: 0x{:x}, expected 0x{:x}\r\n",
                magic, FS_MAGIC
            ));
            return Err("Invalid filesystem magic number");
        }

        let version = unsafe { ptr::read_volatile(ptr::addr_of!(superblock.version)) };
        if version != FS_VERSION {
            crate::kernel::uart_write_string(&alloc::format!(
                "ERROR: Unsupported version: {}, expected {}\r\n",
                version, FS_VERSION
            ));
            return Err("Unsupported filesystem version");
        }

        crate::kernel::uart_write_string("Superblock validated\r\n");

        // Read file table from sector 1
        device.read_sector(1, &mut sector_buffer)?;

        let mut file_table = [FileEntry {
            name: [0; 8],
            start_sector: 0,
            size_sectors: 0,
            size_bytes: 0,
            flags: FileEntry::FLAG_FREE,
            reserved: [0; 3],
        }; MAX_FILES];

        for i in 0..MAX_FILES {
            let offset = i * core::mem::size_of::<FileEntry>();
            file_table[i] = unsafe {
                ptr::read_unaligned(sector_buffer.as_ptr().add(offset) as *const FileEntry)
            };
        }

        crate::kernel::uart_write_string("File table loaded\r\n");

        // Calculate next free sector
        let mut next_free_sector = DATA_START_SECTOR;
        for entry in &file_table {
            if entry.is_used() {
                let end_sector = entry.get_start_sector() as u64 + entry.get_size_sectors() as u64;
                if end_sector > next_free_sector {
                    next_free_sector = end_sector;
                }
            }
        }

        let file_count = unsafe { ptr::read_volatile(ptr::addr_of!(superblock.file_count)) };
        crate::kernel::uart_write_string(&alloc::format!(
            "Filesystem mounted! {} files, next free sector: {}\r\n",
            file_count, next_free_sector
        ));

        Ok(SimpleFilesystem {
            superblock,
            file_table,
            next_free_sector,
        })
    }

    /// Get the number of files in the filesystem
    pub fn file_count(&self) -> u32 {
        unsafe { ptr::read_volatile(ptr::addr_of!(self.superblock.file_count)) }
    }

    /// List all files in the filesystem
    pub fn list_files(&self) -> alloc::vec::Vec<&FileEntry> {
        self.file_table
            .iter()
            .filter(|entry| entry.is_used())
            .collect()
    }

    /// Create a new file in the filesystem
    pub fn create_file(
        &mut self,
        device: &mut VirtioBlkDevice,
        name: &str,
        size_bytes: u32,
    ) -> Result<(), &'static str> {
        // Validate filename
        if name.len() == 0 || name.len() > 8 {
            return Err("Filename must be 1-8 characters");
        }

        // Check if file already exists
        for entry in &self.file_table {
            if entry.is_used() && entry.get_name() == name {
                return Err("File already exists");
            }
        }

        // Find free entry
        let mut entry_index = None;
        for (i, entry) in self.file_table.iter().enumerate() {
            if entry.is_free() {
                entry_index = Some(i);
                break;
            }
        }

        let entry_index = entry_index.ok_or("File table full (max 32 files)")?;

        // Calculate sectors needed
        let size_sectors = ((size_bytes + SECTOR_SIZE as u32 - 1) / SECTOR_SIZE as u32) as u16;

        // Check if we have enough space
        let total_sectors = unsafe { ptr::read_volatile(ptr::addr_of!(self.superblock.total_sectors)) };
        if self.next_free_sector + size_sectors as u64 > total_sectors {
            return Err("Not enough disk space");
        }

        // Create file entry
        let mut name_bytes = [0u8; 8];
        for (i, byte) in name.bytes().take(8).enumerate() {
            name_bytes[i] = byte;
        }

        let new_entry = FileEntry {
            name: name_bytes,
            start_sector: self.next_free_sector as u16,
            size_sectors,
            size_bytes,
            flags: FileEntry::FLAG_USED,
            reserved: [0; 3],
        };

        // Update file table in memory
        self.file_table[entry_index] = new_entry;

        // Update superblock
        let mut file_count = unsafe { ptr::read_volatile(ptr::addr_of!(self.superblock.file_count)) };
        file_count += 1;
        unsafe {
            ptr::write_volatile(ptr::addr_of_mut!(self.superblock.file_count) as *mut u32, file_count);
        }

        // Update next free sector
        self.next_free_sector += size_sectors as u64;

        // Write updated superblock to disk
        // Use static buffer to avoid stack overflow
        static mut CREATE_BUFFER: [u8; 512] = [0; 512];

        unsafe {
            CREATE_BUFFER.fill(0);
            ptr::copy_nonoverlapping(
                &self.superblock as *const Superblock as *const u8,
                CREATE_BUFFER.as_mut_ptr(),
                core::mem::size_of::<Superblock>(),
            );
        }
        device.write_sector(0, unsafe { &CREATE_BUFFER })?;

        // Write updated file table to disk
        unsafe {
            CREATE_BUFFER.fill(0);
            for (i, entry) in self.file_table.iter().enumerate() {
                let offset = i * core::mem::size_of::<FileEntry>();
                ptr::copy_nonoverlapping(
                    entry as *const FileEntry as *const u8,
                    CREATE_BUFFER.as_mut_ptr().add(offset),
                    core::mem::size_of::<FileEntry>(),
                );
            }
        }
        device.write_sector(1, unsafe { &CREATE_BUFFER })?;

        Ok(())
    }

    /// Delete a file from the filesystem
    pub fn delete_file(
        &mut self,
        device: &mut VirtioBlkDevice,
        name: &str,
    ) -> Result<(), &'static str> {
        // Find the file
        let mut entry_index = None;
        for (i, entry) in self.file_table.iter().enumerate() {
            if entry.is_used() && entry.get_name() == name {
                entry_index = Some(i);
                break;
            }
        }

        let entry_index = entry_index.ok_or("File not found")?;

        // Mark entry as free
        unsafe {
            ptr::write_volatile(
                ptr::addr_of_mut!(self.file_table[entry_index].flags) as *mut u8,
                FileEntry::FLAG_FREE
            );
        }

        // Update superblock
        let mut file_count = unsafe { ptr::read_volatile(ptr::addr_of!(self.superblock.file_count)) };
        file_count -= 1;
        unsafe {
            ptr::write_volatile(ptr::addr_of_mut!(self.superblock.file_count) as *mut u32, file_count);
        }

        // Write updated superblock to disk
        // Use static buffer to avoid stack issues
        static mut TEMP_BUFFER: [u8; 512] = [0; 512];

        unsafe {
            TEMP_BUFFER.fill(0);
            ptr::copy_nonoverlapping(
                &self.superblock as *const Superblock as *const u8,
                TEMP_BUFFER.as_mut_ptr(),
                core::mem::size_of::<Superblock>(),
            );
        }
        if let Err(e) = device.write_sector(0, unsafe { &TEMP_BUFFER }) {
            return Err(e);
        }

        // Write updated file table to disk
        unsafe {
            TEMP_BUFFER.fill(0);
            for (i, entry) in self.file_table.iter().enumerate() {
                let offset = i * core::mem::size_of::<FileEntry>();
                ptr::copy_nonoverlapping(
                    entry as *const FileEntry as *const u8,
                    TEMP_BUFFER.as_mut_ptr().add(offset),
                    core::mem::size_of::<FileEntry>(),
                );
            }
        }
        if let Err(e) = device.write_sector(1, unsafe { &TEMP_BUFFER }) {
            return Err(e);
        }

        Ok(())
    }

    /// Write data to a file
    pub fn write_file(
        &mut self,
        device: &mut VirtioBlkDevice,
        name: &str,
        data: &[u8],
    ) -> Result<(), &'static str> {
        // Find the file
        let mut file_entry = None;
        for entry in &self.file_table {
            if entry.is_used() && entry.get_name() == name {
                file_entry = Some(entry.clone());
                break;
            }
        }

        let file_entry = file_entry.ok_or("File not found")?;

        let size_bytes = file_entry.get_size_bytes();
        if data.len() > size_bytes as usize {
            return Err("Data too large for file");
        }

        let start_sector = file_entry.get_start_sector();
        let size_sectors = file_entry.get_size_sectors();

        // Write data sector by sector
        let mut bytes_written = 0;
        for sector_idx in 0..size_sectors {
            let mut sector_buffer = [0u8; SECTOR_SIZE];

            // Copy data for this sector
            let bytes_to_write = core::cmp::min(SECTOR_SIZE, data.len() - bytes_written);
            sector_buffer[..bytes_to_write].copy_from_slice(&data[bytes_written..bytes_written + bytes_to_write]);

            // Write sector
            device.write_sector((start_sector + sector_idx) as u64, &sector_buffer)?;

            bytes_written += bytes_to_write;
            if bytes_written >= data.len() {
                break;
            }
        }

        Ok(())
    }

    /// Read data from a file
    pub fn read_file(
        &self,
        device: &mut VirtioBlkDevice,
        name: &str,
        buffer: &mut [u8],
    ) -> Result<usize, &'static str> {
        // Find the file
        let mut file_entry = None;
        for entry in &self.file_table {
            if entry.is_used() && entry.get_name() == name {
                file_entry = Some(entry.clone());
                break;
            }
        }

        let file_entry = file_entry.ok_or("File not found")?;

        let size_bytes = file_entry.get_size_bytes() as usize;
        if buffer.len() < size_bytes {
            return Err("Buffer too small for file");
        }

        let start_sector = file_entry.get_start_sector();
        let size_sectors = file_entry.get_size_sectors();

        // Read data sector by sector
        let mut bytes_read = 0;
        for sector_idx in 0..size_sectors {
            let mut sector_buffer = [0u8; SECTOR_SIZE];

            // Read sector
            device.read_sector((start_sector + sector_idx) as u64, &mut sector_buffer)?;

            // Copy to output buffer
            let bytes_to_copy = core::cmp::min(SECTOR_SIZE, size_bytes - bytes_read);
            buffer[bytes_read..bytes_read + bytes_to_copy].copy_from_slice(&sector_buffer[..bytes_to_copy]);

            bytes_read += bytes_to_copy;
            if bytes_read >= size_bytes {
                break;
            }
        }

        Ok(bytes_read)
    }
}
