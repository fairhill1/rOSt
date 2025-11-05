//! ELF loader for userspace programs
//!
//! Loads ELF binaries from memory/filesystem and spawns them as isolated EL0 processes.

use goblin::elf::Elf;
use goblin::elf::program_header::PT_LOAD;
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

    // Parse ELF file with goblin
    crate::kernel::uart_write_string("[ELF] Calling Elf::parse()...\r\n");
    let elf = match Elf::parse(elf_data) {
        Ok(e) => {
            crate::kernel::uart_write_string("[ELF] Elf::parse() returned Ok\r\n");
            e
        },
        Err(_e) => {
            crate::kernel::uart_write_string("[ELF] Error: Failed to parse ELF file\r\n");
            return 0;
        }
    };
    crate::kernel::uart_write_string("[ELF] ELF parsed successfully\r\n");

    // Verify it's an AArch64 executable
    if elf.header.e_machine != goblin::elf::header::EM_AARCH64 {
        crate::kernel::uart_write_string("[ELF] Error: Not an AArch64 binary\r\n");
        return 0;
    }

    // Get entry point
    let entry_point = elf.entry;

    // Load program segments into memory
    crate::kernel::uart_write_string("[ELF] Calling load_program_segments...\r\n");
    let (mut loaded_memory, base_vaddr) = match load_program_segments(&elf, elf_data) {
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

    // Apply relocations (critical for .rodata string literals to work)
    crate::kernel::uart_write_string("[ELF] Applying relocations...\r\n");
    apply_relocations(&elf, &mut loaded_memory, loaded_base, base_vaddr);
    crate::kernel::uart_write_string("[ELF] Relocations applied\r\n");

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
fn load_program_segments(elf: &Elf, elf_data: &[u8]) -> Result<(Box<[u8]>, u64), &'static str> {
    // Find the total size needed (highest vaddr + memsz)
    let mut max_addr = 0u64;
    let mut min_addr = u64::MAX;

    for ph in &elf.program_headers {
        if ph.p_type == PT_LOAD {
            let vaddr = ph.p_vaddr;
            let memsz = ph.p_memsz;

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
    crate::kernel::uart_write_string("[ELF] Starting program header iteration...\r\n");
    for ph in &elf.program_headers {
        crate::kernel::uart_write_string("[ELF] Checking segment type...\r\n");
        crate::kernel::uart_write_string("[ELF] Got segment type\r\n");
        if ph.p_type == PT_LOAD {
            crate::kernel::uart_write_string("[ELF] Found LOAD segment, processing...\r\n");
            let vaddr = ph.p_vaddr;
            crate::kernel::uart_write_string("[ELF] Got vaddr\r\n");
            let memsz = ph.p_memsz as usize;
            crate::kernel::uart_write_string("[ELF] Got memsz\r\n");
            let filesz = ph.p_filesz as usize;
            crate::kernel::uart_write_string("[ELF] Got filesz\r\n");
            let offset = ph.p_offset as usize;
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
    let elf = Elf::parse(elf_data).ok()?;
    Some(elf.entry)
}

/// Apply relocations to loaded ELF segments
///
/// This processes .rela.dyn section and patches code/data references
/// to work at the actual loaded base address.
fn apply_relocations(elf: &Elf, loaded_memory: &mut [u8], loaded_base: u64, elf_base_vaddr: u64) {
    use goblin::elf::reloc::*;

    // Calculate the load offset (difference between where ELF expected to be and where it actually is)
    let load_offset = loaded_base.wrapping_sub(elf_base_vaddr);

    crate::kernel::uart_write_string("[RELOC] Processing relocations\r\n");
    crate::kernel::uart_write_string("[RELOC] Load offset calculated\r\n");
    crate::kernel::uart_write_string("[RELOC] ELF base vaddr checked\r\n");
    crate::kernel::uart_write_string("[RELOC] Loaded base confirmed\r\n");

    // Count dynamic relocations
    crate::kernel::uart_write_string("[RELOC] Dynamic relocations (dynrelas): ");
    print_number(elf.dynrelas.len());
    crate::kernel::uart_write_string("\r\n");

    // Count section relocations
    crate::kernel::uart_write_string("[RELOC] Section relocations (shdr_relocs): ");
    print_number(elf.shdr_relocs.len());
    crate::kernel::uart_write_string("\r\n");

    let mut total_relocs_applied = 0;

    // Process dynamic relocations (.rela.dyn) - for dynamically linked binaries
    for reloc in &elf.dynrelas {
        if apply_single_relocation(reloc.r_type, reloc.r_offset, reloc.r_addend.unwrap_or(0),
                                   loaded_memory, load_offset, elf_base_vaddr) {
            total_relocs_applied += 1;
        }
    }

    // Process section relocations (.rela.text, .rela.rodata, etc.) - for statically linked binaries
    crate::kernel::uart_write_string("[RELOC] Processing section relocations...\r\n");
    for (_section_idx, reloc_section) in &elf.shdr_relocs {
        crate::kernel::uart_write_string("[RELOC] Found relocation section with ");
        print_number(reloc_section.len());
        crate::kernel::uart_write_string(" entries\r\n");

        for reloc in reloc_section.iter() {
            let r_addend = reloc.r_addend.unwrap_or(0);
            if apply_single_relocation(reloc.r_type, reloc.r_offset, r_addend,
                                       loaded_memory, load_offset, elf_base_vaddr) {
                total_relocs_applied += 1;
            }
        }
    }

    crate::kernel::uart_write_string("[RELOC] Total relocations applied: ");
    print_number(total_relocs_applied);
    crate::kernel::uart_write_string("\r\n");

    crate::kernel::uart_write_string("[RELOC] Relocation processing complete\r\n");
}

/// Apply a single relocation entry
/// Returns true if successfully applied, false if skipped/failed
fn apply_single_relocation(
    r_type: u32,
    r_offset: u64,
    r_addend: i64,
    loaded_memory: &mut [u8],
    load_offset: u64,
    elf_base_vaddr: u64,
) -> bool {
    use goblin::elf::reloc::*;

    // Calculate position in loaded memory
    let offset_in_buffer = (r_offset - elf_base_vaddr) as usize;

    if offset_in_buffer + 8 > loaded_memory.len() {
        crate::kernel::uart_write_string("[RELOC] Warning: relocation out of bounds\r\n");
        return false;
    }

    // Apply relocation based on type (AArch64 specific)
    match r_type {
        R_AARCH64_RELATIVE => {
            // B + A (base address + addend)
            let value = load_offset.wrapping_add(r_addend as u64);

            // Write 64-bit value at relocation offset
            let bytes = value.to_le_bytes();
            loaded_memory[offset_in_buffer..offset_in_buffer + 8].copy_from_slice(&bytes);
            true
        }
        R_AARCH64_ABS64 => {
            // S + A (symbol value + addend)
            // Absolute 64-bit address that needs adjustment for load offset
            let value = load_offset.wrapping_add(r_addend as u64);

            let bytes = value.to_le_bytes();
            loaded_memory[offset_in_buffer..offset_in_buffer + 8].copy_from_slice(&bytes);
            true
        }
        R_AARCH64_GLOB_DAT | R_AARCH64_JUMP_SLOT => {
            // These need symbol table lookup - skip for now
            false
        }
        275 | 277 | 283 | 285 | 286 => {
            // PC-relative instruction relocations (ADRP, ADD, LDR/STR)
            // For statically-linked binaries, these are already correctly resolved by the linker
            // The code is position-independent - PC-relative offsets don't need adjustment
            // Skip these to avoid corrupting already-correct instructions
            true  // Return true to count as "processed" but don't modify anything
        }
        _ => {
            // Skip unknown types silently
            false
        }
    }
}

/// Print a number to UART without heap allocations
fn print_number(mut n: usize) {
    if n == 0 {
        crate::kernel::uart_write_string("0");
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    // Print in reverse (we built it backwards)
    while i > 0 {
        i -= 1;
        let ch = [buf[i]];
        if let Ok(s) = core::str::from_utf8(&ch) {
            crate::kernel::uart_write_string(s);
        }
    }
}
