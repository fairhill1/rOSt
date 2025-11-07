// Simple Custom Filesystem for rOSt
//
// Disk Layout:
// - Sector 0: Superblock (filesystem metadata)
// - Sector 1: File Table (up to 32 files)
// - Sectors 2-9: Reserved for future use
// - Sectors 10+: Data blocks

use crate::kernel::drivers::virtio::blk::VirtioBlkDevice;
use core::ptr;
extern crate alloc;

const FS_MAGIC: u32 = 0x524F5354; // "ROST" in ASCII
const FS_VERSION: u32 = 1;
const DATA_START_SECTOR: u64 = 11; // Moved to 11 to make room for 2-sector file table
const MAX_FILES: usize = 32;
const SECTOR_SIZE: usize = 512;
const FILE_TABLE_SECTORS: u64 = 2; // File table spans sectors 1-2 (640 bytes needs 2 sectors)

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

        // Write file table to sectors 1-2 (640 bytes total across 2 sectors)
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size; // 512 / 20 = 25

        for sector in 0..FILE_TABLE_SECTORS {
            sector_buffer = [0u8; SECTOR_SIZE];
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    ptr::copy_nonoverlapping(
                        &empty_entry as *const FileEntry as *const u8,
                        sector_buffer.as_mut_ptr().add(offset),
                        entry_size,
                    );
                }
            }
            device.write_sector(1 + sector, &sector_buffer)?;
        }
        crate::kernel::uart_write_string("File table initialized at sectors 1-2\r\n");

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
        let entries_per_sector = SECTOR_SIZE / entry_size; // 25 entries per sector

        for sector in 0..FILE_TABLE_SECTORS {
            device.read_sector(1 + sector, &mut sector_buffer)?;
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                file_table[i] = unsafe {
                    ptr::read_unaligned(sector_buffer.as_ptr().add(offset) as *const FileEntry)
                };
            }
        }

        crate::kernel::uart_write_string("File table loaded from sectors 1-2\r\n");

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

        crate::kernel::uart_write_string("Filesystem mounted!\r\n");

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
        static CREATE_BUFFER: spin::Mutex<[u8; 512]> = spin::Mutex::new([0; 512]);

        {
            let mut buffer = CREATE_BUFFER.lock();
            buffer.fill(0);
            unsafe {
                ptr::copy_nonoverlapping(
                    &self.superblock as *const Superblock as *const u8,
                    buffer.as_mut_ptr(),
                    core::mem::size_of::<Superblock>(),
                );
            }
            device.write_sector(0, &*buffer)?;
        }

        // Write updated file table to disk (sectors 1-2)
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            let mut buffer = CREATE_BUFFER.lock();
            buffer.fill(0);
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    ptr::copy_nonoverlapping(
                        &self.file_table[i] as *const FileEntry as *const u8,
                        buffer.as_mut_ptr().add(offset),
                        entry_size,
                    );
                }
            }
            device.write_sector(1 + sector, &*buffer)?;
        }

        Ok(())
    }

    /// Rename a file in the filesystem
    pub fn rename_file(
        &mut self,
        device: &mut VirtioBlkDevice,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), &'static str> {
        // Validate new name length
        if new_name.len() > 8 {
            return Err("New filename too long (max 8 chars)");
        }
        if new_name.is_empty() {
            return Err("New filename cannot be empty");
        }

        // Check if new name already exists
        for entry in self.file_table.iter() {
            if entry.is_used() && entry.get_name() == new_name {
                return Err("File with new name already exists");
            }
        }

        // Find the file to rename
        let mut entry_index = None;
        for (i, entry) in self.file_table.iter().enumerate() {
            if entry.is_used() && entry.get_name() == old_name {
                entry_index = Some(i);
                break;
            }
        }

        let entry_index = entry_index.ok_or("File not found")?;

        // Update the name in the file entry
        let mut new_name_bytes = [0u8; 8];
        let name_bytes = new_name.as_bytes();
        new_name_bytes[..name_bytes.len()].copy_from_slice(name_bytes);

        unsafe {
            ptr::copy_nonoverlapping(
                new_name_bytes.as_ptr(),
                ptr::addr_of_mut!(self.file_table[entry_index].name) as *mut u8,
                8,
            );
        }

        // Write updated file table to disk (sectors 1-2)
        static RENAME_BUFFER: spin::Mutex<[u8; 512]> = spin::Mutex::new([0; 512]);
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            let mut buffer = RENAME_BUFFER.lock();
            buffer.fill(0);
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    ptr::copy_nonoverlapping(
                        &self.file_table[i] as *const FileEntry as *const u8,
                        buffer.as_mut_ptr().add(offset),
                        entry_size,
                    );
                }
            }
            device.write_sector(1 + sector, &*buffer)?;
        }

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
        static TEMP_BUFFER: spin::Mutex<[u8; 512]> = spin::Mutex::new([0; 512]);

        {
            let mut buffer = TEMP_BUFFER.lock();
            buffer.fill(0);
            unsafe {
                ptr::copy_nonoverlapping(
                    &self.superblock as *const Superblock as *const u8,
                    buffer.as_mut_ptr(),
                    core::mem::size_of::<Superblock>(),
                );
            }
            if let Err(e) = device.write_sector(0, &*buffer) {
                return Err(e);
            }
        }

        // Write updated file table to disk (sectors 1-2)
        let entry_size = core::mem::size_of::<FileEntry>();
        let entries_per_sector = SECTOR_SIZE / entry_size;

        for sector in 0..FILE_TABLE_SECTORS {
            let mut buffer = TEMP_BUFFER.lock();
            buffer.fill(0);
            let start_entry = (sector * entries_per_sector as u64) as usize;
            let end_entry = core::cmp::min(start_entry + entries_per_sector, MAX_FILES);

            for i in start_entry..end_entry {
                let offset = (i - start_entry) * entry_size;
                unsafe {
                    ptr::copy_nonoverlapping(
                        &self.file_table[i] as *const FileEntry as *const u8,
                        buffer.as_mut_ptr().add(offset),
                        entry_size,
                    );
                }
            }
            if let Err(e) = device.write_sector(1 + sector, &*buffer) {
                return Err(e);
            }
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
