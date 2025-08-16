use core::arch::asm;

// Simple 1GB block mapping - much simpler than page tables
#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; 512],
}

static mut L1_TABLE: PageTable = PageTable { entries: [0; 512] };

// Page table entry flags
const PTE_VALID: u64 = 1 << 0;
const PTE_BLOCK: u64 = 0 << 1;      // Block descriptor (not table)
const PTE_AF: u64 = 1 << 10;        // Access flag
const PTE_AP_RW_EL1: u64 = 0 << 6;  // RW for EL1 only
const PTE_SH_INNER: u64 = 3 << 8;   // Inner shareable
const PTE_ATTR_NORMAL: u64 = 0 << 2; // Index 0 in MAIR (normal memory)
const PTE_ATTR_DEVICE: u64 = 1 << 2; // Index 1 in MAIR (device memory)

pub fn init() {
    let uart = crate::uart::Uart::new(0x0900_0000);
    uart.puts("Setting up simple MMU...\n");
    
    unsafe {
        setup_simple_mapping();
        enable_mmu();
    }
    
    uart.puts("Simple MMU initialized\n");
}

unsafe fn setup_simple_mapping() {
    // Clear L1 table
    for i in 0..512 {
        L1_TABLE.entries[i] = 0;
    }
    
    // Create 1GB identity mappings
    // Entry 0: 0x00000000 - 0x3FFFFFFF (1GB) - Normal memory (RAM + low MMIO)
    L1_TABLE.entries[0] = 0x00000000 | 
                          PTE_VALID | 
                          PTE_BLOCK | 
                          PTE_AF | 
                          PTE_AP_RW_EL1 | 
                          PTE_ATTR_NORMAL |
                          PTE_SH_INNER;
    
    // Entry 1: 0x40000000 - 0x7FFFFFFF (1GB) - Normal memory 
    L1_TABLE.entries[1] = 0x40000000 | 
                          PTE_VALID | 
                          PTE_BLOCK | 
                          PTE_AF | 
                          PTE_AP_RW_EL1 | 
                          PTE_ATTR_NORMAL |
                          PTE_SH_INNER;
}

unsafe fn enable_mmu() {
    let uart = crate::uart::Uart::new(0x0900_0000);
    
    // Set up MAIR (Memory Attribute Indirection Register)
    // Index 0: Normal memory, write-back
    // Index 1: Device memory
    let mair = 0xFF | (0x04 << 8);
    asm!("msr mair_el1, {}", in(reg) mair);
    
    // Set up TCR (Translation Control Register)
    // T0SZ = 25 (39-bit VA), TG0 = 4KB, SH0 = Inner Shareable
    // ORGN0/IRGN0 = Write-back cacheable
    let tcr = 25 | (0 << 14) | (3 << 12) | (1 << 10) | (1 << 8);
    asm!("msr tcr_el1, {}", in(reg) tcr);
    
    // Set TTBR0 to point to our L1 table
    let ttbr0 = &L1_TABLE as *const _ as u64;
    asm!("msr ttbr0_el1, {}", in(reg) ttbr0);
    
    // Invalidate TLB
    asm!("tlbi alle1");
    asm!("dsb sy");
    asm!("isb");
    
    uart.puts("About to enable MMU...\n");
    
    // Enable MMU
    let mut sctlr: u64;
    asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    sctlr |= 1; // Enable MMU
    asm!("msr sctlr_el1, {}", in(reg) sctlr);
    asm!("isb");
    
    uart.puts("MMU enabled successfully!\n");
}