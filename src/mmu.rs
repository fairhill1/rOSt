use core::arch::asm;

// Memory constants
const PAGE_SIZE: usize = 4096;
const TABLE_ENTRIES: usize = 512;

// Page table entry flags
const PTE_VALID: u64 = 1 << 0;
const PTE_TABLE: u64 = 1 << 1;
const PTE_AF: u64 = 1 << 10;        // Access flag
const PTE_nG: u64 = 1 << 11;        // Not global
const PTE_AP_RW_EL1: u64 = 0 << 6;  // RW for EL1
const PTE_AP_RW_ALL: u64 = 1 << 6;  // RW for all
const PTE_AP_RO_EL1: u64 = 2 << 6;  // RO for EL1
const PTE_AP_RO_ALL: u64 = 3 << 6;  // RO for all
const PTE_SH_INNER: u64 = 3 << 8;   // Inner shareable
const PTE_NORMAL_MEM: u64 = 0 << 2; // Normal memory (index 0 in MAIR)
const PTE_DEVICE_MEM: u64 = 1 << 2; // Device memory (index 1 in MAIR)

// Translation Control Register flags
const TCR_T0SZ: u64 = 25;           // 39-bit virtual addresses
const TCR_TG0_4K: u64 = 0 << 14;    // 4KB granule
const TCR_SH0_INNER: u64 = 3 << 12; // Inner shareable
const TCR_ORGN0_WB: u64 = 1 << 10;  // Write-back cacheable
const TCR_IRGN0_WB: u64 = 1 << 8;   // Write-back cacheable

// Memory Attribute Indirection Register
const MAIR_NORMAL: u64 = 0xFF;      // Normal memory, write-back, read/write allocate
const MAIR_DEVICE: u64 = 0x04;      // Device memory, nGnRE (allow gather, no reorder)

// We'll use static page tables for simplicity
#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; TABLE_ENTRIES],
}

static mut L1_TABLE: PageTable = PageTable { entries: [0; TABLE_ENTRIES] };
static mut L2_TABLE_0: PageTable = PageTable { entries: [0; TABLE_ENTRIES] };
static mut L2_TABLE_1: PageTable = PageTable { entries: [0; TABLE_ENTRIES] };
static mut L2_TABLE_2: PageTable = PageTable { entries: [0; TABLE_ENTRIES] };
static mut L2_TABLE_3: PageTable = PageTable { entries: [0; TABLE_ENTRIES] };

pub fn init() {
    unsafe {
        setup_page_tables();
        enable_mmu();
    }
    
    crate::uart::Uart::new(0x0900_0000).puts("MMU initialized with paging enabled\n");
}

unsafe fn setup_page_tables() {
    // Clear all tables
    for i in 0..TABLE_ENTRIES {
        L1_TABLE.entries[i] = 0;
    }
    
    // Identity map first 1GB for kernel and MMIO
    // L1[0] -> L2_TABLE_0 (0x00000000 - 0x3FFFFFFF)
    L1_TABLE.entries[0] = (&L2_TABLE_0 as *const _ as u64) | PTE_TABLE | PTE_VALID;
    
    // Each L2 entry covers 2MB - map only what we need
    for i in 0..256 { // Only map first 512MB instead of full 1GB
        let addr = (i * 0x200000) as u64;
        let mut flags = PTE_VALID | PTE_AF;
        
        // Memory type based on address range
        if addr < 0x80000000 {
            // RAM and low MMIO (0-2GB range)
            if addr < 0x08000000 {
                // RAM (0-128MB) - normal cached memory
                flags |= PTE_NORMAL_MEM | PTE_AP_RW_EL1 | PTE_SH_INNER;
            } else {
                // MMIO devices (GIC, UART, etc.) - device memory
                flags |= PTE_DEVICE_MEM | PTE_AP_RW_EL1;
            }
            
            // 2MB block mapping
            L2_TABLE_0.entries[i] = addr | flags | (1 << 1); // Block descriptor
        }
    }
    
    // Map higher half for future user space (0xFFFF_0000_0000_0000)
    // This is prepared but not used yet
}

unsafe fn enable_mmu() {
    let uart = crate::uart::Uart::new(0x0900_0000);
    
    // Set up MAIR (Memory Attribute Indirection Register)
    let mair = MAIR_NORMAL | (MAIR_DEVICE << 8);
    asm!("msr mair_el1, {}", in(reg) mair);
    
    uart.puts("MAIR set: ");
    uart.put_hex(mair);
    uart.puts("\n");
    
    // Set up TCR (Translation Control Register)
    let tcr = TCR_T0SZ | TCR_TG0_4K | TCR_SH0_INNER | TCR_ORGN0_WB | TCR_IRGN0_WB;
    asm!("msr tcr_el1, {}", in(reg) tcr);
    
    uart.puts("TCR set: ");
    uart.put_hex(tcr);
    uart.puts("\n");
    
    // Set page table base
    let ttbr0 = &L1_TABLE as *const _ as u64;
    asm!("msr ttbr0_el1, {}", in(reg) ttbr0);
    
    uart.puts("TTBR0 set: ");
    uart.put_hex(ttbr0);
    uart.puts("\n");
    
    // Invalidate all TLB entries
    asm!("tlbi alle1");
    asm!("dsb sy");
    asm!("isb");
    
    // Enable MMU but without caches first to debug
    let mut sctlr: u64;
    asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    
    uart.puts("Current SCTLR: ");
    uart.put_hex(sctlr);
    uart.puts("\n");
    
    // Enable only MMU first (no caches)
    sctlr |= 1 << 0;  // M bit - Enable MMU
    
    uart.puts("Enabling MMU...\n");
    asm!("msr sctlr_el1, {}", in(reg) sctlr);
    asm!("isb");
    
    uart.puts("MMU enabled successfully!\n");
}

pub fn translate_address(vaddr: u64) -> Option<u64> {
    unsafe {
        let mut par: u64;
        asm!(
            "at s1e1r, {}",
            "mrs {}, par_el1",
            in(reg) vaddr,
            out(reg) par
        );
        
        if par & 1 == 0 {
            // Translation successful
            Some((par & 0xFFFFFFFFF000) | (vaddr & 0xFFF))
        } else {
            None
        }
    }
}

pub fn map_page(vaddr: u64, paddr: u64, flags: u64) {
    // Simplified page mapping for demonstration
    // In a real OS, this would be much more complex
    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    
    unsafe {
        // For now, just update L2 table for simplicity
        if l1_idx == 0 && l2_idx < 512 {
            let block_addr = paddr & !0x1FFFFF; // Align to 2MB
            L2_TABLE_0.entries[l2_idx] = block_addr | flags | PTE_VALID | PTE_AF | (1 << 1);
            
            // Invalidate TLB
            asm!("tlbi vae1, {}", in(reg) vaddr >> 12);
            asm!("dsb sy");
            asm!("isb");
        }
    }
}