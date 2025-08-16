use core::ptr::{read_volatile, write_volatile};

// QEMU virt machine GIC addresses
const GICD_BASE: usize = 0x0800_0000;  // Distributor
const GICC_BASE: usize = 0x0801_0000;  // CPU interface
const GICR_BASE: usize = 0x080A_0000;  // Redistributor (GICv3)

// Distributor registers
const GICD_CTLR: usize = GICD_BASE + 0x0000;
const GICD_TYPER: usize = GICD_BASE + 0x0004;
const GICD_ISENABLER: usize = GICD_BASE + 0x0100;
const GICD_ICENABLER: usize = GICD_BASE + 0x0180;
const GICD_IPRIORITYR: usize = GICD_BASE + 0x0400;
const GICD_ITARGETSR: usize = GICD_BASE + 0x0800;
const GICD_ICFGR: usize = GICD_BASE + 0x0C00;

// CPU interface registers
const GICC_CTLR: usize = GICC_BASE + 0x0000;
const GICC_PMR: usize = GICC_BASE + 0x0004;
const GICC_IAR: usize = GICC_BASE + 0x000C;
const GICC_EOIR: usize = GICC_BASE + 0x0010;

pub struct Gic {
    max_interrupts: u32,
}

impl Gic {
    pub fn new() -> Self {
        let typer = unsafe { read_volatile(GICD_TYPER as *const u32) };
        let max_interrupts = ((typer & 0x1F) + 1) * 32;
        
        Gic { max_interrupts }
    }
    
    pub fn init(&self) {
        unsafe {
            // Disable distributor
            write_volatile(GICD_CTLR as *mut u32, 0);
            
            // Disable all interrupts first
            for i in 0..(self.max_interrupts / 32) {
                write_volatile((GICD_ICENABLER + (i * 4) as usize) as *mut u32, 0xFFFFFFFF);
            }
            
            // Set all interrupts to highest priority (0x00)
            for i in 0..(self.max_interrupts / 4) {
                write_volatile((GICD_IPRIORITYR + (i * 4) as usize) as *mut u32, 0x00000000);
            }
            
            // Set all interrupts to target CPU 0
            for i in 8..(self.max_interrupts / 4) {  // Start from 8 (skip SGIs/PPIs)
                write_volatile((GICD_ITARGETSR + (i * 4) as usize) as *mut u32, 0x01010101);
            }
            
            // Configure PPIs (16-31) as level-triggered
            write_volatile((GICD_ICFGR + 4) as *mut u32, 0x00000000);
            
            // Enable distributor with ARE, Group1
            write_volatile(GICD_CTLR as *mut u32, 0x03);
            
            // CPU interface init
            // Set priority mask to allow all priorities
            write_volatile(GICC_PMR as *mut u32, 0xFF);
            
            // Enable CPU interface for Group 1
            write_volatile(GICC_CTLR as *mut u32, 0x01);
        }
        
        crate::uart::Uart::new(0x0900_0000).puts("GIC initialized\n");
    }
    
    pub fn enable_interrupt(&self, irq: u32) {
        if irq >= self.max_interrupts {
            return;
        }
        
        let reg = irq / 32;
        let bit = irq % 32;
        
        unsafe {
            let addr = (GICD_ISENABLER + (reg * 4) as usize) as *mut u32;
            write_volatile(addr, 1 << bit);
        }
    }
    
    pub fn disable_interrupt(&self, irq: u32) {
        if irq >= self.max_interrupts {
            return;
        }
        
        let reg = irq / 32;
        let bit = irq % 32;
        
        unsafe {
            let addr = (GICD_ICENABLER + (reg * 4) as usize) as *mut u32;
            write_volatile(addr, 1 << bit);
        }
    }
    
    pub fn get_pending_interrupt(&self) -> Option<u32> {
        unsafe {
            let iar = read_volatile(GICC_IAR as *const u32);
            let irq_id = iar & 0x3FF;
            
            if irq_id == 1023 {
                None
            } else {
                Some(irq_id)
            }
        }
    }
    
    pub fn end_interrupt(&self, irq: u32) {
        unsafe {
            write_volatile(GICC_EOIR as *mut u32, irq);
        }
    }
}