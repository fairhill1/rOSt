//! ELF loader for userspace programs
//!
//! Loads ELF binaries from memory/filesystem and spawns them as isolated EL0 processes.

use xmas_elf::ElfFile;
use xmas_elf::program::Type;
use alloc::vec::Vec;
use alloc::boxed::Box;

/// Load an ELF binary from memory and spawn as userspace process
///
/// This function:
/// 1. Parses the ELF file structure
/// 2. Allocates memory for program segments
/// 3. Loads segments into memory
/// 4. Spawns process with entry point
///
/// Returns process ID on success, 0 on failure
pub fn load_elf_and_spawn(elf_data: &[u8]) -> usize {
    crate::kernel::uart_write_string("[ELF] load_elf_and_spawn entered\r\n");

    // NOTE: RLSF allocator handles interrupt masking internally
    // DO NOT disable interrupts here - causes deadlock if GUI thread holds allocator mutex

    crate::kernel::uart_write_string("[ELF] About to validate size\r\n");

    // Validate size
    if elf_data.len() < 4 {
        crate::kernel::uart_write_string("[ELF] Error: Data too small\r\n");
        return 0;
    }

    crate::kernel::uart_write_string("[ELF] Size validated, about to parse ELF\r\n");

    // Parse ELF file
    crate::kernel::uart_write_string("[ELF] Calling ElfFile::new()...\r\n");
    let elf = match ElfFile::new(elf_data) {
        Ok(e) => {
            crate::kernel::uart_write_string("[ELF] ElfFile::new() returned Ok\r\n");
            e
        },
        Err(_e) => {
            crate::kernel::uart_write_string("[ELF] Error: Failed to parse ELF file\r\n");
            return 0;
        }
    };
    crate::kernel::uart_write_string("[ELF] ELF parsed successfully\r\n");

    // Verify it's an AArch64 executable
    if elf.header.pt2.machine().as_machine() != xmas_elf::header::Machine::AArch64 {
        crate::kernel::uart_write_string("[ELF] Error: Not an AArch64 binary\r\n");
        return 0;
    }

    // Get entry point
    let entry_point = elf.header.pt2.entry_point();

    // Load program segments into memory
    crate::kernel::uart_write_string("[ELF] Calling load_program_segments...\r\n");
    let (loaded_memory, base_vaddr) = match load_program_segments(&elf, elf_data) {
        Ok(result) => {
            crate::kernel::uart_write_string("[ELF] load_program_segments returned Ok\r\n");
            result
        },
        Err(_e) => {
            crate::kernel::uart_write_string("[ELF] Error loading segments\r\n");
            return 0;
        }
    };
    crate::kernel::uart_write_string("[ELF] Destructured result tuple\r\n");

    let loaded_base = loaded_memory.as_ptr() as u64;
    crate::kernel::uart_write_string("[ELF] Got loaded_base pointer\r\n");

    // Calculate entry point offset from the ELF base virtual address
    crate::kernel::uart_write_string("[ELF] About to calculate entry_offset\r\n");
    let entry_offset = entry_point - base_vaddr;
    crate::kernel::uart_write_string("[ELF] Calculated entry_offset\r\n");
    let actual_entry = loaded_base + entry_offset;
    crate::kernel::uart_write_string("[ELF] Calculated actual_entry\r\n");

    crate::kernel::uart_write_string("[ELF] About to transmute entry point\r\n");
    let entry_fn: extern "C" fn() -> ! = unsafe {
        core::mem::transmute(actual_entry as usize)
    };
    crate::kernel::uart_write_string("[ELF] Entry function created\r\n");

    // Spawn through scheduler
    crate::kernel::uart_write_string("[ELF] About to lock scheduler\r\n");
    let process_id = crate::kernel::scheduler::SCHEDULER.lock().spawn_user_process(entry_fn);
    crate::kernel::uart_write_string("[ELF] Process spawned\r\n");

    // CRITICAL: We leak the memory here on purpose!
    // The process needs this memory to stay alive
    // In a real OS, this would be managed by the process manager
    crate::kernel::uart_write_string("[ELF] About to leak memory\r\n");
    Box::leak(loaded_memory);
    crate::kernel::uart_write_string("[ELF] Memory leaked\r\n");

    crate::kernel::uart_write_string("[ELF] Returning process_id\r\n");
    process_id
}

/// Load program segments from ELF into memory
///
/// Allocates a contiguous buffer and copies all LOAD segments.
/// Returns (allocated memory buffer, base virtual address from ELF).
fn load_program_segments(elf: &ElfFile, elf_data: &[u8]) -> Result<(Box<[u8]>, u64), &'static str> {
    // Find the total size needed (highest vaddr + memsz)
    let mut max_addr = 0u64;
    let mut min_addr = u64::MAX;

    for program_header in elf.program_iter() {
        if program_header.get_type() == Ok(Type::Load) {
            let vaddr = program_header.virtual_addr();
            let memsz = program_header.mem_size();

            min_addr = core::cmp::min(min_addr, vaddr);
            max_addr = core::cmp::max(max_addr, vaddr + memsz);
        }
    }

    if min_addr == u64::MAX {
        return Err("No LOAD segments found");
    }

    let total_size = (max_addr - min_addr) as usize;
    crate::kernel::uart_write_string("[ELF] Allocating program memory, size = ");
    if total_size < 1000000 {  // Print size if reasonable
        // Simple size print (just to confirm it's sane)
    }
    crate::kernel::uart_write_string("\r\n");

    // Allocate memory for the program
    crate::kernel::uart_write_string("[ELF] About to allocate vec...\r\n");
    let mut program_memory = alloc::vec![0u8; total_size].into_boxed_slice();
    crate::kernel::uart_write_string("[ELF] Vec allocated successfully\r\n");

    crate::kernel::uart_write_string("[ELF] Copying LOAD segments\r\n");

    // Copy each LOAD segment
    crate::kernel::uart_write_string("[ELF] Starting program_iter...\r\n");
    for program_header in elf.program_iter() {
        crate::kernel::uart_write_string("[ELF] Checking segment type...\r\n");
        let seg_type = program_header.get_type();
        crate::kernel::uart_write_string("[ELF] Got segment type\r\n");
        if seg_type == Ok(Type::Load) {
            crate::kernel::uart_write_string("[ELF] Found LOAD segment, processing...\r\n");
            let vaddr = program_header.virtual_addr();
            crate::kernel::uart_write_string("[ELF] Got vaddr\r\n");
            let memsz = program_header.mem_size() as usize;
            crate::kernel::uart_write_string("[ELF] Got memsz\r\n");
            let filesz = program_header.file_size() as usize;
            crate::kernel::uart_write_string("[ELF] Got filesz\r\n");
            let offset = program_header.offset() as usize;
            crate::kernel::uart_write_string("[ELF] Got offset\r\n");

            // Calculate offset in our allocated buffer
            let buffer_offset = (vaddr - min_addr) as usize;
            crate::kernel::uart_write_string("[ELF] Calculated buffer_offset\r\n");

            // Check bounds
            if buffer_offset + memsz > total_size {
                return Err("Segment out of bounds");
            }
            crate::kernel::uart_write_string("[ELF] Bounds check passed\r\n");

            // Copy file data
            if filesz > 0 {
                crate::kernel::uart_write_string("[ELF] About to copy file data\r\n");
                if offset + filesz > elf_data.len() {
                    return Err("ELF file truncated");
                }
                program_memory[buffer_offset..buffer_offset + filesz]
                    .copy_from_slice(&elf_data[offset..offset + filesz]);
                crate::kernel::uart_write_string("[ELF] File data copied\r\n");
            }

            // Zero the BSS (memsz > filesz)
            if memsz > filesz {
                crate::kernel::uart_write_string("[ELF] About to zero BSS\r\n");
                program_memory[buffer_offset + filesz..buffer_offset + memsz].fill(0);
                crate::kernel::uart_write_string("[ELF] BSS zeroed\r\n");
            }
            crate::kernel::uart_write_string("[ELF] Segment complete\r\n");
        } else {
            crate::kernel::uart_write_string("[ELF] Not a LOAD segment, skipping\r\n");
        }
    }
    crate::kernel::uart_write_string("[ELF] All segments processed\r\n");

    crate::kernel::uart_write_string("[ELF] About to return from load_program_segments\r\n");
    let result = (program_memory, min_addr);
    crate::kernel::uart_write_string("[ELF] Result tuple created\r\n");
    Ok(result)
}

/// Get the entry point from an ELF file without loading it
pub fn get_elf_entry_point(elf_data: &[u8]) -> Option<u64> {
    let elf = ElfFile::new(elf_data).ok()?;
    Some(elf.header.pt2.entry_point())
}
