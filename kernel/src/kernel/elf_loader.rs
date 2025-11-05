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
    // NOTE: RLSF allocator handles interrupt masking internally
    // DO NOT disable interrupts here - causes deadlock if GUI thread holds allocator mutex
    crate::kernel::uart_write_string("[ELF] Starting ELF load...\r\n");
    crate::kernel::uart_write_string("[ELF] ELF data size: ");
    crate::kernel::uart_write_string(if elf_data.len() < 100000 { "OK\r\n" } else { "LARGE\r\n" });

    // Check ELF magic bytes
    if elf_data.len() < 4 {
        crate::kernel::uart_write_string("[ELF] Error: Data too small\r\n");
        return 0;
    }

    crate::kernel::uart_write_string("[ELF] Magic bytes: ");
    for i in 0..4 {
        let byte = elf_data[i];
        let hex_chars = b"0123456789ABCDEF";
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte >> 4) as usize]);
            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[(byte & 0xF) as usize]);
            core::ptr::write_volatile(0x09000000 as *mut u8, b' ');
        }
    }
    crate::kernel::uart_write_string("\r\n");

    crate::kernel::uart_write_string("[ELF] About to call ElfFile::new()...\r\n");

    // Parse ELF file - NO allocations before this!
    let elf = match ElfFile::new(elf_data) {
        Ok(e) => {
            crate::kernel::uart_write_string("[ELF] ELF parsed successfully\r\n");
            e
        }
        Err(_e) => {
            crate::kernel::uart_write_string("[ELF] Error: Failed to parse ELF file\r\n");
            return 0;
        }
    };

    // Verify it's an AArch64 executable
    if elf.header.pt2.machine().as_machine() != xmas_elf::header::Machine::AArch64 {
        crate::kernel::uart_write_string("[ELF] Error: Not an AArch64 binary\r\n");
        return 0;
    }

    // Get entry point
    let entry_point = elf.header.pt2.entry_point();
    crate::kernel::uart_write_string("[ELF] Got entry point\r\n");

    // Load program segments into memory
    let (loaded_memory, base_vaddr) = match load_program_segments(&elf, elf_data) {
        Ok(result) => result,
        Err(_e) => {
            crate::kernel::uart_write_string("[ELF] Error loading segments\r\n");
            return 0;
        }
    };

    let loaded_base = loaded_memory.as_ptr() as u64;
    crate::kernel::uart_write_string("[ELF] Loaded program into memory\r\n");

    // Calculate entry point offset from the ELF base virtual address
    let entry_offset = entry_point - base_vaddr;
    let actual_entry = loaded_base + entry_offset;

    let entry_fn: extern "C" fn() -> ! = unsafe {
        core::mem::transmute(actual_entry as usize)
    };

    // Spawn through scheduler
    let process_id = crate::kernel::scheduler::SCHEDULER.lock().spawn_user_process(entry_fn);

    crate::kernel::uart_write_string("[ELF] Spawned process\r\n");

    // CRITICAL: We leak the memory here on purpose!
    // The process needs this memory to stay alive
    // In a real OS, this would be managed by the process manager
    Box::leak(loaded_memory);

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
    crate::kernel::uart_write_string("[ELF] Allocating program memory\r\n");

    // Allocate memory for the program
    let mut program_memory = alloc::vec![0u8; total_size].into_boxed_slice();

    crate::kernel::uart_write_string("[ELF] Copying LOAD segments\r\n");

    // Copy each LOAD segment
    for program_header in elf.program_iter() {
        if program_header.get_type() == Ok(Type::Load) {
            let vaddr = program_header.virtual_addr();
            let memsz = program_header.mem_size() as usize;
            let filesz = program_header.file_size() as usize;
            let offset = program_header.offset() as usize;

            // Calculate offset in our allocated buffer
            let buffer_offset = (vaddr - min_addr) as usize;

            // Check bounds
            if buffer_offset + memsz > total_size {
                return Err("Segment out of bounds");
            }

            // Copy file data
            if filesz > 0 {
                if offset + filesz > elf_data.len() {
                    return Err("ELF file truncated");
                }
                program_memory[buffer_offset..buffer_offset + filesz]
                    .copy_from_slice(&elf_data[offset..offset + filesz]);
            }

            // Zero the BSS (memsz > filesz)
            if memsz > filesz {
                program_memory[buffer_offset + filesz..buffer_offset + memsz].fill(0);
            }
        }
    }

    Ok((program_memory, min_addr))
}

/// Get the entry point from an ELF file without loading it
pub fn get_elf_entry_point(elf_data: &[u8]) -> Option<u64> {
    let elf = ElfFile::new(elf_data).ok()?;
    Some(elf.header.pt2.entry_point())
}
