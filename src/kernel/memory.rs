// Memory management for the kernel

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

// Global page tables (must be 4KB aligned)
static mut KERNEL_L0_TABLE: PageTable = PageTable::new();
static mut KERNEL_L1_TABLE: PageTable = PageTable::new();
static mut USER_L0_TABLE: PageTable = PageTable::new();
static mut USER_L1_TABLE: PageTable = PageTable::new();

/// Memory region for mapping
const KERNEL_BASE: u64 = 0xFFFF_0000_0000_0000; // High half for kernel
const USER_BASE: u64 = 0x0000_0000_0000_0000;   // Low half for user

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

        crate::kernel::uart_write_string("[MMU] MMU already enabled by UEFI - reading current state...\r\n");

        // Read current TTBR values to understand UEFI's mapping
        let current_ttbr0 = TTBR0_EL1.get();
        let current_ttbr1 = TTBR1_EL1.get();
        let current_tcr = TCR_EL1.get();

        crate::kernel::uart_write_string("[MMU] Current TTBR0: 0x");
        print_hex(current_ttbr0);
        crate::kernel::uart_write_string("\r\n");

        crate::kernel::uart_write_string("[MMU] Current TTBR1: 0x");
        print_hex(current_ttbr1);
        crate::kernel::uart_write_string("\r\n");

        crate::kernel::uart_write_string("[MMU] Current TCR: 0x");
        print_hex(current_tcr);
        crate::kernel::uart_write_string("\r\n");

        // === PREPARE MMU PAGE TABLES NOW - DURING KERNEL INIT ===
        setup_mmu_page_tables();

        crate::kernel::uart_write_string("[MMU] Memory protection is PREPARED\r\n");
        crate::kernel::uart_write_string("[MMU] Switch will happen immediately when entering EL0\r\n");
    }
}

/// Prepare page tables for memory protection and return both addresses for assembly
/// This prepares everything and returns the addresses for the assembly to use
#[no_mangle]
pub extern "C" fn setup_user_page_tables(user_stack_top: u64) -> (u64, u64) {
    use aarch64_cpu::asm::barrier;
    use aarch64_cpu::registers::*;
    use core::arch::asm;

    crate::kernel::uart_write_string("[MMU] Preparing REAL memory protection...\r\n");

    unsafe {
        // Read current TTBR0 to understand UEFI's identity mapping
        let current_ttbr0 = TTBR0_EL1.get();
        let current_ttbr1 = TTBR1_EL1.get();

        crate::kernel::uart_write_string("[MMU] Current UEFI TTBR0: 0x");
        print_hex(current_ttbr0);
        crate::kernel::uart_write_string("\r\n");

        crate::kernel::uart_write_string("[MMU] Current UEFI TTBR1: 0x");
        print_hex(current_ttbr1);
        crate::kernel::uart_write_string("\r\n");

        // === STEP 1: Copy UEFI's current mappings ===
        // Read UEFI's L0 page table and copy it to preserve current behavior
        let uefi_l0 = current_ttbr0 as *const PageTableEntry;

        crate::kernel::uart_write_string("[MMU] Copying UEFI page tables for real switch...\r\n");

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
        crate::kernel::uart_write_string("[MMU] Preparing kernel high-half mapping...\r\n");

        // Add kernel high-half mapping to TTBR1
        let kernel_l1_table_addr = (&KERNEL_L1_TABLE as *const PageTable) as u64;
        let kernel_l0_entry = PageTableEntry::new_table(kernel_l1_table_addr);
        KERNEL_L0_TABLE.entries[511] = kernel_l0_entry; // L0 index 511 gives high half

        // Map kernel space in high half (copy first 1GB of physical memory)
        for i in 0..512usize {
            let addr = (i as u64) * 0x200000; // 2MB blocks
            let entry = PageTableEntry::new_block(addr, false, true, true); // Kernel RW, execute
            KERNEL_L1_TABLE.entries[i] = entry;
        }

        // === STEP 3: Prepare user space mappings for TTBR0 ===
        crate::kernel::uart_write_string("[MMU] Preparing user space mappings...\r\n");

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
        crate::kernel::uart_write_string("[MMU] Preparing TCR for dual mapping...\r\n");

        let tcr_val = TCR_EL1.get();
        crate::kernel::uart_write_string("[MMU] Current TCR: 0x");
        print_hex(tcr_val);
        crate::kernel::uart_write_string("\r\n");

        // Enable TTBR1 (keep T0SZ from UEFI, just enable T1SZ)
        let new_tcr = tcr_val
            | (25u64 << 32) // T1SZ = 25 (48-bit virtual addresses for TTBR1)
            | (0x3 << 28)   // TG1 = 4KB granule for TTBR1
            | (0x3 << 24)   // SH1 = Inner Shareable for TTBR1
            | (0x1 << 23)   // ORGN1 = Normal memory for TTBR1
            | (0x1 << 22);  // IRGN1 = Normal memory for TTBR1

        crate::kernel::uart_write_string("[MMU] New TCR: 0x");
        print_hex(new_tcr);
        crate::kernel::uart_write_string("\r\n");

        // Get the table addresses for assembly
        let user_table_addr = (&USER_L0_TABLE as *const PageTable) as u64;
        let kernel_table_addr = (&KERNEL_L0_TABLE as *const PageTable) as u64;

        crate::kernel::uart_write_string("[MMU] Page tables prepared for REAL switch\r\n");
        crate::kernel::uart_write_string("[MMU] Prepared TTBR0: 0x");
        print_hex(user_table_addr);
        crate::kernel::uart_write_string("\r\n");

        crate::kernel::uart_write_string("[MMU] Prepared TTBR1: 0x");
        print_hex(kernel_table_addr);
        crate::kernel::uart_write_string("\r\n");

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

        crate::kernel::uart_write_string("[MMU] Copying UEFI mappings...\r\n");

        // Copy UEFI's L0 entries to both our TTBR0 and TTBR1 tables
        for i in 0..512usize {
            let uefi_entry = core::ptr::read_volatile(uefi_l0.add(i));
            // Copy to both user and kernel tables to preserve current behavior
            USER_L0_TABLE.entries[i] = uefi_entry;
            KERNEL_L0_TABLE.entries[i] = uefi_entry;
        }

        crate::kernel::uart_write_string("[MMU] Setting up kernel high-half mapping...\r\n");

        // Add kernel high-half mapping to TTBR1
        let kernel_l1_table_addr = (&KERNEL_L1_TABLE as *const PageTable) as u64;
        let kernel_l0_entry = PageTableEntry::new_table(kernel_l1_table_addr);
        KERNEL_L0_TABLE.entries[511] = kernel_l0_entry; // L0 index 511 gives high half

        // Map kernel space in high half (first 1GB of physical memory)
        for i in 0..512usize {
            let addr = (i as u64) * 0x200000; // 2MB blocks
            let entry = PageTableEntry::new_block(addr, false, true, true); // Kernel RW, execute
            KERNEL_L1_TABLE.entries[i] = entry;
        }

        crate::kernel::uart_write_string("[MMU] Setting up user space mappings...\r\n");

        // For now, don't create user mappings - keep UEFI mappings intact
        // We can add user space mappings later without breaking existing functionality
        // USER_L0_TABLE.entries[0] = user_l0_entry; // COMMENTED OUT - don't override UEFI mappings

        // Map some user space (first 4MB for now)
        for i in 0..2usize { // 2 * 2MB = 4MB
            let addr = (i as u64) * 0x200000;
            let entry = PageTableEntry::new_block(addr, true, true, true); // User RWX
            USER_L1_TABLE.entries[i] = entry;
        }

        // Store page table addresses for the assembly code
        USER_TABLE_ADDR = (&USER_L0_TABLE as *const PageTable) as u64;
        KERNEL_TABLE_ADDR = (&KERNEL_L0_TABLE as *const PageTable) as u64;

        crate::kernel::uart_write_string("[MMU] Page tables prepared!\r\n");
        crate::kernel::uart_write_string("[MMU] Ready for TTBR0/TTBR1 switch\r\n");
    }
}

// Global storage for page table addresses (so assembly can access them)
pub static mut USER_TABLE_ADDR: u64 = 0;
pub static mut KERNEL_TABLE_ADDR: u64 = 0;

/// Get the prepared page table addresses for assembly code
#[no_mangle]
pub extern "C" fn get_page_table_addresses() -> (u64, u64) {
    unsafe { (USER_TABLE_ADDR, KERNEL_TABLE_ADDR) }
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