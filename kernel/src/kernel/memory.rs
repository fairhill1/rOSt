// Memory management for the kernel

use core::arch::global_asm;

global_asm!(include_str!("trampoline.s"));

extern "C" {
    pub fn kernel_trampoline_to_high_half_with_tcr(page_table_addr: u64, new_tcr: u64);
}

#[no_mangle]
pub extern "C" fn kernel_high_half_cleanup() {
    crate::kernel::uart_write_string("[MMU] In high-half, cleaning up identity map...\r\n");
    // TODO: Implement the actual cleanup

    // TODO: TTBR0 switching breaks VirtIO descriptor access - need better solution
    // For now, keep UEFI's page tables for TTBR0 to allow kernel device access
    // This means user processes can't access low addresses yet, but system boots
    // switch_ttbr0_to_user_tables();

    // Continue with kernel initialization in the high half.
    crate::kernel::kernel_init_high_half();
}

/// UEFI Memory Descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryDescriptor {
    pub typ: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

/// Memory types from UEFI spec
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryType {
    Reserved = 0,
    LoaderCode = 1,
    LoaderData = 2,
    BootServicesCode = 3,
    BootServicesData = 4,
    RuntimeServicesCode = 5,
    RuntimeServicesData = 6,
    Conventional = 7,
    Unusable = 8,
    AcpiReclaim = 9,
    AcpiNvs = 10,
    MemoryMappedIo = 11,
    MemoryMappedIoPortSpace = 12,
    PalCode = 13,
    PersistentMemory = 14,
}

/// Physical memory allocator state
static mut PHYS_MEM_ALLOCATOR: PhysicalMemoryAllocator = PhysicalMemoryAllocator::new();

struct PhysicalMemoryAllocator {
    initialized: bool,
    next_free_page: u64,
    memory_end: u64,
}

impl PhysicalMemoryAllocator {
    const fn new() -> Self {
        Self {
            initialized: false,
            next_free_page: 0,
            memory_end: 0,
        }
    }
    
    fn init(&mut self, memory_map: &[MemoryDescriptor]) {
        // Find the highest usable memory address and first free page
        let mut highest_addr = 0u64;
        let mut first_free = u64::MAX;
        
        for desc in memory_map {
            if desc.typ == MemoryType::Conventional as u32 {
                let end = desc.physical_start + (desc.number_of_pages * 4096);
                if end > highest_addr {
                    highest_addr = end;
                }
                if desc.physical_start < first_free {
                    first_free = desc.physical_start;
                }
            }
        }
        
        // Start allocating from 16MB to leave low memory for devices
        self.next_free_page = 0x1000000.max(first_free);
        self.memory_end = highest_addr;
        self.initialized = true;
    }
    
    fn alloc_page(&mut self) -> Option<u64> {
        if !self.initialized || self.next_free_page >= self.memory_end {
            return None;
        }
        
        let page = self.next_free_page;
        self.next_free_page += 4096;
        Some(page)
    }
}

/// Initialize physical memory management
pub fn init_physical_memory(memory_map: &[MemoryDescriptor]) {
    unsafe {
        if memory_map.is_empty() {
            // If no memory map provided, use a default range for QEMU ARM64
            // QEMU ARM virt machine has RAM starting at 0x40000000
            // Use a safe range starting at 64MB (0x44000000) for kernel allocations
            PHYS_MEM_ALLOCATOR.next_free_page = 0x44000000;
            PHYS_MEM_ALLOCATOR.memory_end = 0x80000000; // 1GB range
            PHYS_MEM_ALLOCATOR.initialized = true;
        } else {
            PHYS_MEM_ALLOCATOR.init(memory_map);
        }
    }
}

/// Allocate a physical page (4KB)
pub fn alloc_physical_page() -> Option<u64> {
    unsafe {
        PHYS_MEM_ALLOCATOR.alloc_page()
    }
}

/// Allocate multiple contiguous physical pages (4KB each)
pub fn allocate_pages(num_pages: usize) -> Option<u64> {
    if num_pages == 0 {
        return None;
    }
    
    unsafe {
        if !PHYS_MEM_ALLOCATOR.initialized {
            return None;
        }
        
        let total_size = num_pages * 4096;
        if PHYS_MEM_ALLOCATOR.next_free_page + total_size as u64 >= PHYS_MEM_ALLOCATOR.memory_end {
            return None;
        }
        
        let base_addr = PHYS_MEM_ALLOCATOR.next_free_page;
        PHYS_MEM_ALLOCATOR.next_free_page += total_size as u64;
        Some(base_addr)
    }
}

/// Page table structures for ARM64
#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    const fn new() -> Self {
        Self {
            entries: [PageTableEntry(0); 512],
        }
    }

    fn zero(&mut self) {
        for entry in &mut self.entries {
            entry.0 = 0;
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    const VALID: u64 = 1 << 0;
    const TABLE: u64 = 1 << 1;  // This is a table (not a block)
    const BLOCK: u64 = 0 << 1;  // This is a block (not a table)
    const AF: u64 = 1 << 10;    // Access flag
    const NG: u64 = 1 << 11;    // Not global

    // Access permissions (AP[2:1])
    const AP_KERN_RW: u64 = 0b00 << 6;  // Kernel RW, User no access
    const AP_KERN_RW_USER_RW: u64 = 0b01 << 6;  // Kernel RW, User RW
    const AP_KERN_RO: u64 = 0b10 << 6;  // Kernel RO, User no access
    const AP_KERN_RO_USER_RO: u64 = 0b11 << 6;  // Kernel RO, User RO

    // Shareability
    const SH_INNER: u64 = 3 << 8; // Inner shareable

    // Memory attributes (index into MAIR_EL1)
    const ATTR_DEVICE_nGnRnE: u64 = 0 << 2;  // Device memory
    const ATTR_NORMAL: u64 = 1 << 2;  // Normal cached memory

    // User execute never
    const UXN: u64 = 1 << 54;
    // Privileged execute never
    const PXN: u64 = 1 << 53;

    fn new_table(addr: u64) -> Self {
        Self(addr | Self::VALID | Self::TABLE)
    }

    fn new_block(addr: u64, user_access: bool, writable: bool, executable: bool) -> Self {
        let ap = if user_access {
            if writable { Self::AP_KERN_RW_USER_RW } else { Self::AP_KERN_RO_USER_RO }
        } else {
            if writable { Self::AP_KERN_RW } else { Self::AP_KERN_RO }
        };

        let mut flags = addr | Self::VALID | Self::BLOCK | Self::AF | ap | Self::SH_INNER | Self::ATTR_NORMAL;

        // Set execute-never bits if not executable
        if !executable {
            flags |= Self::UXN | Self::PXN;
        }

        Self(flags)
    }

    fn is_valid(&self) -> bool {
        (self.0 & Self::VALID) != 0
    }

    fn addr(&self) -> u64 {
        self.0 & 0x0000_FFFF_FFFF_F000 // Extract address bits [47:12]
    }
}

// TCR_EL1 (Translation Control Register) bit field constants
// Used for configuring TTBR0/TTBR1 address translation
mod tcr_el1 {
    // T1SZ field (bits 21:16) - controls TTBR1 address space size
    pub const T1SZ_MASK: u64 = 0x3F << 16;
    pub const T1SZ_16: u64 = 16 << 16;  // 48-bit VA (2^(64-16) = 256TB)

    // EPD (Enable Page walks Disable) bits
    pub const EPD0_BIT: u64 = 1 << 7;   // EPD0: Disable TTBR0 page walks if set
    pub const EPD1_BIT: u64 = 1 << 23;  // EPD1: Disable TTBR1 page walks if set
    pub const EPD0_ENABLE: u64 = 0 << 7;   // Keep TTBR0 page walks enabled
    pub const EPD1_ENABLE: u64 = 0 << 23;  // Keep TTBR1 page walks enabled

    // TG1 (Translation Granule for TTBR1) - bits 31:30
    pub const TG1_4KB: u64 = 0x2 << 30;  // 4KB granule size

    // SH1 (Shareability for TTBR1) - bits 29:28
    pub const SH1_INNER_SHAREABLE: u64 = 0x3 << 28;

    // ORGN1 (Outer cacheability for TTBR1) - bits 27:26
    pub const ORGN1_WRITE_BACK: u64 = 0x1 << 26;  // Write-Back Write-Allocate

    // IRGN1 (Inner cacheability for TTBR1) - bits 25:24
    pub const IRGN1_WRITE_BACK: u64 = 0x1 << 24;  // Write-Back Write-Allocate
}

// Global page tables (must be 4KB aligned)
static mut KERNEL_L0_TABLE: PageTable = PageTable::new();
static mut KERNEL_L1_TABLE: PageTable = PageTable::new();
static mut KERNEL_L2_TABLE_0: PageTable = PageTable::new(); // L2 for 0-1GB
static mut KERNEL_L2_TABLE_1: PageTable = PageTable::new(); // L2 for 1-2GB
static mut KERNEL_L2_TABLE_2: PageTable = PageTable::new(); // L2 for 2-3GB
static mut KERNEL_L2_TABLE_3: PageTable = PageTable::new(); // L2 for 3-4GB
static mut USER_L0_TABLE: PageTable = PageTable::new();
static mut USER_L1_TABLE: PageTable = PageTable::new();
static mut USER_L2_TABLE_0: PageTable = PageTable::new(); // User L2 for 0-1GB
static mut USER_L2_TABLE_1: PageTable = PageTable::new(); // User L2 for 1-2GB
static mut USER_L2_TABLE_2: PageTable = PageTable::new(); // User L2 for 2-3GB
static mut USER_L2_TABLE_3: PageTable = PageTable::new(); // User L2 for 3-4GB

/// Memory region for mapping
/// KERNEL_BASE must have L0 index 510 (bits [47:39] = 0x1FE) for TTBR1
/// L0 index 510 = 0x1FE = 0b1_1111_1110
/// When placed at bits [47:39]: bit 39 = 0, bits [40:47] = 0xFF
/// This gives bits [47:32] = 0xFF00, so address = 0xFFFF_FF00_0000_0000 (canonical form)
pub const KERNEL_BASE: u64 = 0xFFFF_FF00_0000_0000; // High half for kernel (L0 index 510)
const USER_BASE: u64 = 0x0000_0000_0000_0000;   // Low half for user

/// Convert kernel virtual address to physical address
/// Required for DMA operations (VirtIO, etc.) which need physical addresses
#[inline]
pub fn virt_to_phys(virt: u64) -> u64 {
    if virt >= KERNEL_BASE {
        virt - KERNEL_BASE
    } else {
        virt // Already physical or user space
    }
}

/// Initialize virtual memory with TTBR0/TTBR1 separation
pub fn init_virtual_memory() {
    use aarch64_cpu::asm::barrier;
    use aarch64_cpu::registers::*;
    use core::arch::asm;

    crate::kernel::uart_write_string("[MMU] Setting up memory protection...\r\n");

    unsafe {
        // Check if MMU is already enabled
        let sctlr_current = SCTLR_EL1.get();
        let mmu_enabled = (sctlr_current & (1 << 0)) != 0;

        if !mmu_enabled {
            crate::kernel::uart_write_string("[MMU] ERROR: MMU not enabled by UEFI!\r\n");
            crate::kernel::uart_write_string("[MMU] This is unexpected - system may not work correctly\r\n");
            return; // Don't try to set up MMU from scratch
        }

        // Read current TTBR values to understand UEFI's mapping
        let current_ttbr0 = TTBR0_EL1.get();
        let current_tcr = TCR_EL1.get();

        // Save original TTBR0 so we can restore it after user programs exit
        UEFI_TTBR0 = current_ttbr0;

        // === PREPARE MMU PAGE TABLES NOW - DURING KERNEL INIT ===
        setup_mmu_page_tables();

        // Prepare TCR_EL1 value to enable TTBR1 for high addresses
        // CRITICAL: We must KEEP T0SZ unchanged! UEFI's TTBR0 page tables were built for the current T0SZ

        // Clear only T1SZ[21:16] field, keep T0SZ unchanged
        let tcr_cleared = current_tcr & !tcr_el1::T1SZ_MASK;

        // Clear EPD0 and EPD1 bits to ensure both TTBR0 and TTBR1 page walks are enabled
        let tcr_cleared2 = tcr_cleared & !(tcr_el1::EPD0_BIT | tcr_el1::EPD1_BIT);

        let new_tcr = tcr_cleared2
            | tcr_el1::T1SZ_16                  // T1SZ = 16 (48-bit VA for TTBR1)
            | tcr_el1::EPD1_ENABLE              // Enable TTBR1 page walks (CRITICAL!)
            | tcr_el1::EPD0_ENABLE              // Keep TTBR0 page walks enabled
            | tcr_el1::TG1_4KB                  // 4KB granule for TTBR1
            | tcr_el1::SH1_INNER_SHAREABLE      // Inner Shareable for TTBR1
            | tcr_el1::ORGN1_WRITE_BACK         // Write-Back Write-Allocate for TTBR1
            | tcr_el1::IRGN1_WRITE_BACK;        // Write-Back Write-Allocate for TTBR1
            // T0SZ and other T0 attributes remain unchanged from UEFI's setting

        // === IMPLEMENT HIGHER-Half KERNEL TRAMPOLINE ===
        // IMPORTANT: Pass both the page table address AND the new TCR value
        // The assembly will switch TTBR1 FIRST, then update TCR
        unsafe {
            let kernel_l0_table_addr = (&KERNEL_L0_TABLE as *const PageTable) as u64;
            kernel_trampoline_to_high_half_with_tcr(kernel_l0_table_addr, new_tcr);
        }

        // This function never returns
        crate::kernel::uart_write_string("[MMU] ERROR: Returned from higher-half trampoline - should not happen!\r\n");
        loop { aarch64_cpu::asm::wfe(); }
    }
}

/// Prepare page tables for memory protection and return both addresses for assembly
/// This prepares everything and returns the addresses for the assembly to use
#[no_mangle]
pub extern "C" fn setup_user_page_tables(user_stack_top: u64) -> (u64, u64) {
    use aarch64_cpu::asm::barrier;
    use aarch64_cpu::registers::*;
    use core::arch::asm;

    unsafe {
        // Read current TTBR0 to understand UEFI's identity mapping
        let current_ttbr0 = TTBR0_EL1.get();

        // === STEP 1: Copy UEFI's current mappings ===
        // Read UEFI's L0 page table and copy it to preserve current behavior
        let uefi_l0 = current_ttbr0 as *const PageTableEntry;

        // Initialize our page tables by copying UEFI's mappings
        KERNEL_L0_TABLE.zero();
        KERNEL_L1_TABLE.zero();
        USER_L0_TABLE.zero();
        USER_L1_TABLE.zero();

        // Copy UEFI's L0 entries to both our TTBR0 and TTBR1 tables
        for i in 0..512usize {
            let uefi_entry = core::ptr::read_volatile(uefi_l0.add(i));
            // Copy to both user and kernel tables to preserve current behavior
            USER_L0_TABLE.entries[i] = uefi_entry;
            KERNEL_L0_TABLE.entries[i] = uefi_entry;
        }

        // === STEP 2: Prepare kernel page tables for TTBR1 ===
        // Add kernel high-half mapping to TTBR1
        // KERNEL_BASE = 0xFFFF_0000_0000_0000, L0 index = bits[47:39] = 510
        let kernel_l1_table_addr = (&KERNEL_L1_TABLE as *const PageTable) as u64;
        let kernel_l0_entry = PageTableEntry::new_table(kernel_l1_table_addr);
        KERNEL_L0_TABLE.entries[510] = kernel_l0_entry; // L0 index 510 for 0xFFFF_0000_...

        // Map kernel space in high half using 1GB block mappings at L1 level
        // L1 has 512 entries, each covering 1GB, so this covers 512GB of physical memory
        for i in 0..512usize {
            let addr = (i as u64) * 0x40000000; // 1GB blocks (0x40000000 = 1GB)
            let entry = PageTableEntry::new_block(addr, false, true, true); // Kernel RW, execute
            KERNEL_L1_TABLE.entries[i] = entry;
        }

        // === STEP 3: Prepare user space mappings for TTBR0 ===
        // Add user stack and program mappings to TTBR0
        let user_l1_table_addr = (&USER_L1_TABLE as *const PageTable) as u64;
        let user_l0_entry = PageTableEntry::new_table(user_l1_table_addr);

        // Replace first entry with our user L1 table (only affects low addresses)
        USER_L0_TABLE.entries[0] = user_l0_entry;

        // Map user stack (last 4MB before stack top)
        let stack_base = (user_stack_top & !0x1FFF_F) - 4 * 1024 * 1024; // 4MB stack
        for i in 0..2usize { // 2 * 2MB = 4MB
            let addr = stack_base + (i as u64) * 0x200000;
            let entry = PageTableEntry::new_block(addr, true, true, false); // User RW, no execute
            USER_L1_TABLE.entries[((addr >> 21) & 0x1FF) as usize] = entry;
        }

        // Map user program region (identity mapping for now)
        for i in 0..2usize { // 4MB for user program
            let addr = (i as u64) * 0x200000;
            let entry = PageTableEntry::new_block(addr, true, true, true); // User RWX
            USER_L1_TABLE.entries[i] = entry;
        }

        // === STEP 4: Prepare TCR for dual mapping ===
        let tcr_val = TCR_EL1.get();

        // Enable TTBR1 (keep T0SZ from UEFI, just enable T1SZ)
        let _new_tcr = tcr_val
            | (25u64 << 32) // T1SZ = 25 (48-bit virtual addresses for TTBR1)
            | (0x3 << 28)   // TG1 = 4KB granule for TTBR1
            | (0x3 << 24)   // SH1 = Inner Shareable for TTBR1
            | (0x1 << 23)   // ORGN1 = Normal memory for TTBR1
            | (0x1 << 22);  // IRGN1 = Normal memory for TTBR1

        // Get the table addresses for assembly
        let user_table_addr = (&USER_L0_TABLE as *const PageTable) as u64;
        let kernel_table_addr = (&KERNEL_L0_TABLE as *const PageTable) as u64;

        // Return both addresses for assembly to use: (user_table_addr, kernel_table_addr)
        (user_table_addr, kernel_table_addr)
    }
}

/// Set up the page tables during kernel initialization
pub fn setup_mmu_page_tables() {
    use aarch64_cpu::registers::*;

    unsafe {
        // Read UEFI's current L0 page table and copy it
        let uefi_l0 = TTBR0_EL1.get() as *const PageTableEntry;

        // Initialize our page tables by copying UEFI's mappings
        KERNEL_L0_TABLE.zero();
        KERNEL_L1_TABLE.zero();
        USER_L0_TABLE.zero();
        USER_L1_TABLE.zero();

        // Copy UEFI's L0 entries to both our TTBR0 and TTBR1 tables
        for i in 0..512usize {
            let uefi_entry = core::ptr::read_volatile(uefi_l0.add(i));
            // Copy to both user and kernel tables to preserve current behavior
            USER_L0_TABLE.entries[i] = uefi_entry;
            KERNEL_L0_TABLE.entries[i] = uefi_entry;
        }

        // Add kernel high-half mapping to TTBR1
        // KERNEL_BASE = 0xFFFF_FF00_0000_0000, L0 index = bits[47:39] = 510
        let kernel_l1_table_addr = (&KERNEL_L1_TABLE as *const PageTable) as u64;
        let kernel_l0_entry = PageTableEntry::new_table(kernel_l1_table_addr);
        KERNEL_L0_TABLE.entries[510] = kernel_l0_entry; // L0 index 510 for 0xFFFF_FF00_...

        // Set up L2 tables for 0-4GB using 2MB blocks
        // L2[0]: 0-1GB
        let l2_0_addr = (&KERNEL_L2_TABLE_0 as *const PageTable) as u64;
        KERNEL_L1_TABLE.entries[0] = PageTableEntry::new_table(l2_0_addr);
        for i in 0..512usize {
            let addr = (i as u64) * 0x200000;
            KERNEL_L2_TABLE_0.entries[i] = PageTableEntry::new_block(addr, false, true, true);
        }

        // L2[1]: 1-2GB (where the kernel code is!)
        let l2_1_addr = (&KERNEL_L2_TABLE_1 as *const PageTable) as u64;
        KERNEL_L1_TABLE.entries[1] = PageTableEntry::new_table(l2_1_addr);
        for i in 0..512usize {
            let addr = 0x40000000 + (i as u64) * 0x200000;
            KERNEL_L2_TABLE_1.entries[i] = PageTableEntry::new_block(addr, false, true, true);
        }

        // L2[2]: 2-3GB
        let l2_2_addr = (&KERNEL_L2_TABLE_2 as *const PageTable) as u64;
        KERNEL_L1_TABLE.entries[2] = PageTableEntry::new_table(l2_2_addr);
        for i in 0..512usize {
            let addr = 0x80000000 + (i as u64) * 0x200000;
            KERNEL_L2_TABLE_2.entries[i] = PageTableEntry::new_block(addr, false, true, true);
        }

        // L2[3]: 3-4GB
        let l2_3_addr = (&KERNEL_L2_TABLE_3 as *const PageTable) as u64;
        KERNEL_L1_TABLE.entries[3] = PageTableEntry::new_table(l2_3_addr);
        for i in 0..512usize {
            let addr = 0xC0000000 + (i as u64) * 0x200000;
            KERNEL_L2_TABLE_3.entries[i] = PageTableEntry::new_block(addr, false, true, true);
        }

        // CRITICAL: Clean data cache for ALL page tables
        // Each page table is 4KB = 64 cache lines (assuming 64-byte cache lines)

        // Clean L0 table (4KB)
        let l0_addr = &KERNEL_L0_TABLE as *const _ as usize;
        for offset in (0..4096).step_by(64) {
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) l0_addr + offset,
                options(nostack)
            );
        }

        // Clean L1 table (4KB)
        let l1_addr = &KERNEL_L1_TABLE as *const _ as usize;
        for offset in (0..4096).step_by(64) {
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) l1_addr + offset,
                options(nostack)
            );
        }

        // Clean all L2 tables (4 x 4KB)
        for l2_table in [&KERNEL_L2_TABLE_0, &KERNEL_L2_TABLE_1, &KERNEL_L2_TABLE_2, &KERNEL_L2_TABLE_3] {
            let l2_addr = l2_table as *const _ as usize;
            for offset in (0..4096).step_by(64) {
                core::arch::asm!(
                    "dc civac, {0}",
                    in(reg) l2_addr + offset,
                    options(nostack)
                );
            }
        }

        // Final barrier to ensure all cache operations complete
        core::arch::asm!(
            "dsb sy",
            "isb",
            options(nostack)
        );

        // TEMPORARY: Map entire 4GB into user space with USER permissions
        // This allows the test user program (which is compiled into the kernel) to execute
        // TODO: In production, load user binaries at low addresses and only map those pages

        // Create user-accessible L2 mappings (0-4GB with 2MB granularity)
        // L2[0]: 0-1GB
        for i in 0..512usize {
            let addr = (i as u64) * 0x200000;
            USER_L2_TABLE_0.entries[i] = PageTableEntry::new_block(addr, true, true, true); // User RWX
        }

        // L2[1]: 1-2GB (where kernel code is - needs execute permission!)
        for i in 0..512usize {
            let addr = 0x40000000 + (i as u64) * 0x200000;
            USER_L2_TABLE_1.entries[i] = PageTableEntry::new_block(addr, true, true, true); // User RWX
        }

        // L2[2]: 2-3GB
        for i in 0..512usize {
            let addr = 0x80000000 + (i as u64) * 0x200000;
            USER_L2_TABLE_2.entries[i] = PageTableEntry::new_block(addr, true, true, true); // User RWX
        }

        // L2[3]: 3-4GB
        for i in 0..512usize {
            let addr = 0xC0000000 + (i as u64) * 0x200000;
            USER_L2_TABLE_3.entries[i] = PageTableEntry::new_block(addr, true, true, true); // User RWX
        }

        // Point USER_L1_TABLE entries to the user L2 tables
        let user_l2_0_addr = (&USER_L2_TABLE_0 as *const PageTable) as u64;
        let user_l2_1_addr = (&USER_L2_TABLE_1 as *const PageTable) as u64;
        let user_l2_2_addr = (&USER_L2_TABLE_2 as *const PageTable) as u64;
        let user_l2_3_addr = (&USER_L2_TABLE_3 as *const PageTable) as u64;

        USER_L1_TABLE.entries[0] = PageTableEntry::new_table(user_l2_0_addr); // 0-1GB
        USER_L1_TABLE.entries[1] = PageTableEntry::new_table(user_l2_1_addr); // 1-2GB (kernel code!)
        USER_L1_TABLE.entries[2] = PageTableEntry::new_table(user_l2_2_addr); // 2-3GB
        USER_L1_TABLE.entries[3] = PageTableEntry::new_table(user_l2_3_addr); // 3-4GB

        // CRITICAL: Copy UEFI's mappings for 4GB-512GB range (includes PCI ECAM at 256GB!)
        // UEFI's L0 entry 0 points to an L1 table that covers 0-512GB
        // We've replaced entries 0-3 (0-4GB) with our user-accessible mappings
        // Now copy entries 4-511 (4GB-512GB) from UEFI's L1 table
        let uefi_l0_entry_0 = core::ptr::read_volatile(uefi_l0.add(0));
        if uefi_l0_entry_0.0 & PageTableEntry::VALID != 0 {
            // Extract L1 table address from UEFI's L0 entry 0
            let uefi_l1_addr = (uefi_l0_entry_0.0 & 0x0000_FFFF_FFFF_F000) as *const PageTableEntry;

            // Copy entries 4-511 from UEFI's L1 table to preserve device mappings
            for i in 4..512usize {
                let uefi_l1_entry = core::ptr::read_volatile(uefi_l1_addr.add(i));
                USER_L1_TABLE.entries[i] = uefi_l1_entry;
            }
        }

        // Clean data cache for USER_L1_TABLE (we just modified it)
        let l1_addr = &USER_L1_TABLE as *const _ as usize;
        for offset in (0..4096).step_by(64) {
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) l1_addr + offset,
                options(nostack)
            );
        }

        // Clean data cache for user L2 page tables
        for l2_table in [&USER_L2_TABLE_0, &USER_L2_TABLE_1, &USER_L2_TABLE_2, &USER_L2_TABLE_3] {
            let l2_addr = l2_table as *const _ as usize;
            for offset in (0..4096).step_by(64) {
                core::arch::asm!(
                    "dc civac, {0}",
                    in(reg) l2_addr + offset,
                    options(nostack)
                );
            }
        }

        // Barrier
        core::arch::asm!(
            "dsb sy",
            "isb",
            options(nostack)
        );

        // Connect USER_L0_TABLE[0] to USER_L1_TABLE (CRITICAL!)
        let user_l1_table_addr = (&USER_L1_TABLE as *const PageTable) as u64;
        USER_L0_TABLE.entries[0] = PageTableEntry::new_table(user_l1_table_addr);

        // Clean USER_L0_TABLE and USER_L1_TABLE caches
        let l0_addr = &USER_L0_TABLE as *const _ as usize;
        for offset in (0..4096).step_by(64) {
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) l0_addr + offset,
                options(nostack)
            );
        }
        let l1_addr = &USER_L1_TABLE as *const _ as usize;
        for offset in (0..4096).step_by(64) {
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) l1_addr + offset,
                options(nostack)
            );
        }
        core::arch::asm!("dsb sy", "isb", options(nostack));

        // CRITICAL: Invalidate TLBs for user address space (TTBR0)
        // This ensures the CPU doesn't use stale TLB entries
        core::arch::asm!(
            "tlbi vmalle1",  // Invalidate all TLB entries for EL1
            "dsb sy",
            "isb",
            options(nostack)
        );

        // Initialize TTBR0 addresses for per-thread switching
        use aarch64_cpu::registers::*;
        UEFI_TTBR0 = TTBR0_EL1.get();  // Save original UEFI TTBR0 (for kernel operations)
        let user_table_virt = (&USER_L0_TABLE as *const PageTable) as u64;
        USER_TTBR0 = virt_to_phys(user_table_virt);  // Convert user table address to physical
        USER_TABLE_ADDR = USER_TTBR0;  // Legacy name for compatibility

        let kernel_table_virt = (&KERNEL_L0_TABLE as *const PageTable) as u64;
        KERNEL_TABLE_ADDR = virt_to_phys(kernel_table_virt);  // Convert kernel table to physical (for compatibility)
    }
}

/// Switch TTBR0 to user page tables (called once during boot)
/// After this, we don't need to switch TTBR0 on context switches - just leave it on user tables
pub fn switch_ttbr0_to_user_tables() {
    use aarch64_cpu::registers::*;

    unsafe {
        let user_table_virt = (&USER_L0_TABLE as *const PageTable) as u64;
        let user_table_phys = virt_to_phys(user_table_virt);

        // Switch TTBR0 to user page tables (must use PHYSICAL address!)
        TTBR0_EL1.set(user_table_phys);

        // CRITICAL: Must invalidate TLB to flush stale TTBR0 entries!
        // We invalidate ALL entries (TTBR0 + TTBR1), but that's OK because:
        // - Execution continues from high-half (TTBR1) and will refill TLB as needed
        // - This ensures TTBR0 entries point to correct USER page tables, not stale UEFI tables
        core::arch::asm!(
            "dsb sy",              // Ensure TTBR0 write is visible
            "tlbi vmalle1is",      // Invalidate ALL TLB entries for EL1 (both TTBR0 and TTBR1)
            "dsb ish",             // Ensure TLB invalidation completes
            "isb",                 // Synchronize context
            options(nostack)
        );
    }
}

// Global storage for page table addresses (so assembly can access them)
#[no_mangle]
pub static mut USER_TTBR0: u64 = 0;      // Physical address of user page table
#[no_mangle]
pub static mut UEFI_TTBR0: u64 = 0;      // Original UEFI page table (for kernel)

// Legacy names for compatibility with get_page_table_addresses()
#[no_mangle]
static mut USER_TABLE_ADDR: u64 = 0;     // Same as USER_TTBR0 (physical address)
#[no_mangle]
static mut KERNEL_TABLE_ADDR: u64 = 0;   // Kernel table (not used with per-thread TTBR0 switching)

/// Get the prepared page table addresses for assembly code
#[no_mangle]
pub extern "C" fn get_page_table_addresses() -> (u64, u64) {
    unsafe { (USER_TABLE_ADDR, KERNEL_TABLE_ADDR) }
}

/// Restore original kernel MMU context after user program exits
pub fn restore_kernel_mmu_context() {
    use aarch64_cpu::registers::*;
    use aarch64_cpu::asm::barrier;

    unsafe {
        // Restore original TTBR0 (UEFI's page table)
        let original_ttbr0 = UEFI_TTBR0;

        // Switch back to original page tables
        TTBR0_EL1.set(original_ttbr0);

        // Memory barrier to ensure the change takes effect
        barrier::dsb(barrier::SY);
        barrier::isb(barrier::SY);
    }
}

fn print_hex(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    for i in (0..16).rev() {
        let digit = (n >> (i * 4)) & 0xF;
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, hex_chars[digit as usize]);
        }
    }
}