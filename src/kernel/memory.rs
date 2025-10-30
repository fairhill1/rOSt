// Memory management for the kernel

use core::mem::size_of;

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

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    const VALID: u64 = 1 << 0;
    const TABLE: u64 = 1 << 1;  // This is a table (not a block)
    const AF: u64 = 1 << 10;    // Access flag
    const NG: u64 = 1 << 11;    // Not global
    const AP_RW: u64 = 0 << 7;  // Read-write
    const AP_RO: u64 = 1 << 7;  // Read-only
    const SH_INNER: u64 = 3 << 8; // Inner shareable
    const ATTR_NORMAL: u64 = 0 << 2; // Normal memory
    
    fn new_table(addr: u64) -> Self {
        Self(addr | Self::VALID | Self::TABLE)
    }
    
    fn new_page(addr: u64, writable: bool) -> Self {
        let ap = if writable { Self::AP_RW } else { Self::AP_RO };
        Self(addr | Self::VALID | Self::AF | ap | Self::SH_INNER | Self::ATTR_NORMAL)
    }
}

/// Initialize virtual memory (identity mapping for now)
pub fn init_virtual_memory() {
    // For now, we'll use identity mapping (virtual = physical)
    // This is simpler and sufficient for early kernel development
    
    unsafe {
        // Get translation table base register
        let mut ttbr0: u64;
        core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ttbr0);
        
        // We're already using UEFI's page tables
        // Later we'll create our own
        
        // For now, just ensure memory barriers
        core::arch::asm!("dsb sy");
        core::arch::asm!("isb");
    }
}