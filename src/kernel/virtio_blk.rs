// VirtIO Block Device Driver
// Based on VirtIO 1.0 specification and Stephen Brennan's implementation

use crate::kernel::pci::{PciConfig, PciDevice};
use crate::kernel::memory;
use core::ptr;
use alloc::vec::Vec;

// VirtIO Device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_BLK_DEVICE_ID_LEGACY: u16 = 0x1001;
const VIRTIO_BLK_DEVICE_ID_MODERN: u16 = 0x1042;

// VirtIO Status Register Bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 128;

// VirtIO PCI Capability Types
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

// Virtqueue descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

// VirtIO Block Request Types
const VIRTIO_BLK_T_IN: u32 = 0;  // Read
const VIRTIO_BLK_T_OUT: u32 = 1; // Write

// VirtIO Block Status
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

const SECTOR_SIZE: usize = 512;
const QUEUE_SIZE: u16 = 128;

// Memory barrier
#[inline(always)]
fn mb() {
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// VirtIO PCI Common Configuration (mapped via BAR)
#[repr(C)]
#[derive(Debug)]
struct VirtioPciCommonCfg {
    device_feature_select: u32,
    device_feature: u32,
    driver_feature_select: u32,
    driver_feature: u32,
    msix_config: u16,
    num_queues: u16,
    device_status: u8,
    config_generation: u8,
    queue_select: u16,
    queue_size: u16,
    queue_msix_vector: u16,
    queue_enable: u16,
    queue_notify_off: u16,
    queue_desc_lo: u32,
    queue_desc_hi: u32,
    queue_avail_lo: u32,
    queue_avail_hi: u32,
    queue_used_lo: u32,
    queue_used_hi: u32,
}

/// Virtqueue Descriptor
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

/// Virtqueue Available Ring
#[repr(C, packed)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    // ring follows (variable length)
}

/// Virtqueue Used Element
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

/// Virtqueue Used Ring
#[repr(C, packed)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    // ring follows (variable length)
}

/// VirtIO Block Request Header
#[repr(C, packed)]
struct VirtioBlkReqHeader {
    req_type: u32,
    reserved: u32,
    sector: u64,
}

/// Virtqueue structure
struct Virtqueue {
    // Physical address of queue memory
    phys_addr: u64,
    // Queue size
    size: u16,
    // Last seen used index
    last_seen_used: u16,
    // Next free descriptor
    free_desc: u16,

    // Pointers to queue structures (virtual addresses)
    desc: *mut VirtqDesc,
    avail: *mut VirtqAvail,
    avail_ring: *mut u16,
    used: *mut VirtqUsed,
    used_ring: *mut VirtqUsedElem,

    // Track virtual addresses for each descriptor
    desc_virt_addrs: [u64; QUEUE_SIZE as usize],
}

impl Virtqueue {
    /// Create a new virtqueue with the given size
    unsafe fn new(size: u16) -> Option<Self> {
        // Calculate memory layout according to VirtIO spec
        // Descriptor table: 16 bytes per descriptor, 64-byte aligned
        let desc_size = (size as usize) * core::mem::size_of::<VirtqDesc>();

        // Available ring: 6 + 2*size bytes, 2-byte aligned
        let avail_size = 6 + 2 * (size as usize);

        // Used ring: 6 + 8*size bytes, 4-byte aligned
        let used_size = 6 + 8 * (size as usize);

        // Total size with alignment padding
        let total_size = desc_size + avail_size + used_size + 64 + 4;

        // Allocate physical memory (must be DMA accessible)
        // We'll use 0x50000000 like we did for VirtIO input
        let phys_addr = 0x50000000u64;
        let virt_addr = phys_addr; // Identity mapped for now

        // Zero out the memory
        ptr::write_bytes(virt_addr as *mut u8, 0, total_size);

        // Set up pointers with proper alignment
        let desc = virt_addr as *mut VirtqDesc;
        let avail = (virt_addr + desc_size as u64) as *mut VirtqAvail;
        let avail_ring = (virt_addr + desc_size as u64 + 4) as *mut u16;
        let used = ((virt_addr + desc_size as u64 + avail_size as u64 + 3) & !3) as *mut VirtqUsed;
        let used_ring = (((virt_addr + desc_size as u64 + avail_size as u64 + 3) & !3) + 4) as *mut VirtqUsedElem;

        // Initialize free descriptor chain
        for i in 0..(size - 1) {
            (*desc.add(i as usize)).next = i + 1;
            (*desc.add(i as usize)).flags = 0;
        }
        (*desc.add((size - 1) as usize)).next = 0;

        Some(Virtqueue {
            phys_addr,
            size,
            last_seen_used: 0,
            free_desc: 0,
            desc,
            avail,
            avail_ring,
            used,
            used_ring,
            desc_virt_addrs: [0; QUEUE_SIZE as usize],
        })
    }

    /// Allocate a descriptor and associate it with a virtual address
    unsafe fn alloc_desc(&mut self, virt_addr: u64) -> Option<u16> {
        let idx = self.free_desc;
        let desc_ptr = self.desc.add(idx as usize);

        // Update free list
        self.free_desc = (*desc_ptr).next;

        // Store virtual address for this descriptor
        self.desc_virt_addrs[idx as usize] = virt_addr;

        Some(idx)
    }

    /// Free a descriptor back to the free list
    unsafe fn free_desc(&mut self, idx: u16) {
        let desc_ptr = self.desc.add(idx as usize);
        (*desc_ptr).next = self.free_desc;
        (*desc_ptr).flags = 0;
        self.free_desc = idx;
        self.desc_virt_addrs[idx as usize] = 0;
    }
}

/// VirtIO Block Device
pub struct VirtioBlkDevice {
    pci_device: PciDevice,
    common_cfg: *mut VirtioPciCommonCfg,
    notify_base: u64,
    notify_off_multiplier: u32,
    virtq: Virtqueue,
    capacity: u64,
}

impl VirtioBlkDevice {
    /// Find and initialize all VirtIO block devices
    pub fn find_and_init(ecam_base: u64, mmio_base: u64) -> Vec<VirtioBlkDevice> {
        let mut devices = Vec::new();
        let config = PciConfig::with_base_addr(ecam_base);

        crate::kernel::uart_write_string("Scanning for VirtIO block devices...\r\n");

        // Scan PCI bus
        for device_num in 0..32 {
            if let Some(pci_dev) = PciDevice::new(0, device_num, 0, &config) {
                // Check if this is a VirtIO block device
                if pci_dev.vendor_id == VIRTIO_VENDOR_ID &&
                   (pci_dev.device_id == VIRTIO_BLK_DEVICE_ID_MODERN ||
                    pci_dev.device_id == VIRTIO_BLK_DEVICE_ID_LEGACY) {

                    crate::kernel::uart_write_string(&alloc::format!(
                        "Found VirtIO block device at 0:{}:0 (device_id=0x{:x})\r\n",
                        device_num, pci_dev.device_id
                    ));

                    if let Some(blk_dev) = unsafe { Self::init_device(pci_dev, mmio_base) } {
                        devices.push(blk_dev);
                    }
                }
            }
        }

        crate::kernel::uart_write_string(&alloc::format!(
            "Found {} VirtIO block device(s)\r\n", devices.len()
        ));

        devices
    }

    /// Initialize a VirtIO block device
    unsafe fn init_device(pci_dev: PciDevice, mmio_base: u64) -> Option<Self> {
        crate::kernel::uart_write_string("Initializing VirtIO block device...\r\n");

        // Enable bus mastering
        pci_dev.enable_bus_mastering();

        // Parse PCI capabilities to find VirtIO structures
        let (common_cfg_addr, notify_addr, notify_off_mult) =
            Self::parse_capabilities(&pci_dev, mmio_base)?;

        let common_cfg = common_cfg_addr as *mut VirtioPciCommonCfg;

        // Device initialization sequence (VirtIO spec 3.1)

        // 1. Reset device
        ptr::write_volatile(&mut (*common_cfg).device_status, 0);
        mb();

        // 2. Set ACKNOWLEDGE bit
        ptr::write_volatile(&mut (*common_cfg).device_status, VIRTIO_STATUS_ACKNOWLEDGE);
        mb();

        // 3. Set DRIVER bit
        let status = ptr::read_volatile(&(*common_cfg).device_status);
        ptr::write_volatile(&mut (*common_cfg).device_status, status | VIRTIO_STATUS_DRIVER);
        mb();

        crate::kernel::uart_write_string("Device acknowledged, driver bit set\r\n");

        // 4. Feature negotiation (we don't need any special features for now)
        ptr::write_volatile(&mut (*common_cfg).driver_feature_select, 0);
        ptr::write_volatile(&mut (*common_cfg).driver_feature, 0);
        mb();

        // 5. Set FEATURES_OK
        let status = ptr::read_volatile(&(*common_cfg).device_status);
        ptr::write_volatile(&mut (*common_cfg).device_status, status | VIRTIO_STATUS_FEATURES_OK);
        mb();

        // 6. Re-read status to ensure FEATURES_OK is still set
        let status = ptr::read_volatile(&(*common_cfg).device_status);
        if (status & VIRTIO_STATUS_FEATURES_OK) == 0 {
            crate::kernel::uart_write_string("ERROR: Device rejected our features\r\n");
            return None;
        }

        crate::kernel::uart_write_string("Features negotiated successfully\r\n");

        // 7. Set up virtqueue
        let mut virtq = Virtqueue::new(QUEUE_SIZE)?;

        // Select queue 0 (block devices only have one queue)
        ptr::write_volatile(&mut (*common_cfg).queue_select, 0);
        mb();

        let queue_size = ptr::read_volatile(&(*common_cfg).queue_size);
        crate::kernel::uart_write_string(&alloc::format!(
            "Device reports queue size: {}\r\n", queue_size
        ));

        // Set queue size
        ptr::write_volatile(&mut (*common_cfg).queue_size, QUEUE_SIZE);

        // Set queue addresses
        let desc_phys = virtq.phys_addr;
        let avail_phys = desc_phys + (QUEUE_SIZE as u64 * 16);
        let used_phys = (avail_phys + 6 + 2 * QUEUE_SIZE as u64 + 3) & !3;

        ptr::write_volatile(&mut (*common_cfg).queue_desc_lo, desc_phys as u32);
        ptr::write_volatile(&mut (*common_cfg).queue_desc_hi, (desc_phys >> 32) as u32);
        ptr::write_volatile(&mut (*common_cfg).queue_avail_lo, avail_phys as u32);
        ptr::write_volatile(&mut (*common_cfg).queue_avail_hi, (avail_phys >> 32) as u32);
        ptr::write_volatile(&mut (*common_cfg).queue_used_lo, used_phys as u32);
        ptr::write_volatile(&mut (*common_cfg).queue_used_hi, (used_phys >> 32) as u32);
        mb();

        // Enable the queue
        ptr::write_volatile(&mut (*common_cfg).queue_enable, 1);
        mb();

        crate::kernel::uart_write_string("Virtqueue configured and enabled\r\n");

        // 8. Set DRIVER_OK
        let status = ptr::read_volatile(&(*common_cfg).device_status);
        ptr::write_volatile(&mut (*common_cfg).device_status, status | VIRTIO_STATUS_DRIVER_OK);
        mb();

        crate::kernel::uart_write_string("Device ready!\r\n");

        // Read device capacity (from device-specific config space)
        // For now we'll skip this and just report success

        Some(VirtioBlkDevice {
            pci_device: pci_dev,
            common_cfg,
            notify_base: notify_addr,
            notify_off_multiplier: notify_off_mult,
            virtq,
            capacity: 0,
        })
    }

    /// Parse PCI capabilities to find VirtIO structures
    unsafe fn parse_capabilities(pci_dev: &PciDevice, mmio_base: u64) -> Option<(u64, u64, u32)> {
        let mut cap_ptr = pci_dev.get_capabilities_ptr()? as u16;
        let mut common_cfg_addr = None;
        let mut notify_addr = None;
        let mut notify_off_mult = 0u32;

        // Read and program BAR4 (where VirtIO capabilities point)
        let bar4_size = pci_dev.get_bar_size(4)?;
        let bar4_addr = mmio_base + 0x100000; // Allocate space in MMIO region

        pci_dev.write_config_u32(0x20, bar4_addr as u32);
        pci_dev.write_config_u32(0x24, (bar4_addr >> 32) as u32);

        crate::kernel::uart_write_string(&alloc::format!(
            "BAR4: size=0x{:x}, allocated at 0x{:x}\r\n", bar4_size, bar4_addr
        ));

        // Iterate through capability list
        while cap_ptr != 0 && cap_ptr < 0xFF {
            let cap_id = pci_dev.read_config_u8(cap_ptr as u8);

            if cap_id == 0x09 { // Vendor-specific capability
                let cfg_type = pci_dev.read_config_u8((cap_ptr + 3) as u8);
                let bar = pci_dev.read_config_u8((cap_ptr + 4) as u8);
                let offset = pci_dev.read_config_u32((cap_ptr + 8) as u8);

                if bar == 4 {
                    let addr = bar4_addr + offset as u64;

                    match cfg_type {
                        VIRTIO_PCI_CAP_COMMON_CFG => {
                            common_cfg_addr = Some(addr);
                            crate::kernel::uart_write_string(&alloc::format!(
                                "Found common cfg at 0x{:x}\r\n", addr
                            ));
                        }
                        VIRTIO_PCI_CAP_NOTIFY_CFG => {
                            notify_addr = Some(addr);
                            notify_off_mult = pci_dev.read_config_u32((cap_ptr + 16) as u8);
                            crate::kernel::uart_write_string(&alloc::format!(
                                "Found notify at 0x{:x} (mult={})\r\n", addr, notify_off_mult
                            ));
                        }
                        _ => {}
                    }
                }
            }

            cap_ptr = pci_dev.read_config_u8((cap_ptr + 1) as u8) as u16;
        }

        Some((common_cfg_addr?, notify_addr?, notify_off_mult))
    }

    /// Read a sector from the block device
    pub fn read_sector(&mut self, sector: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        unsafe {
            // Allocate memory for request header
            let header_phys = 0x50100000u64;
            let header = header_phys as *mut VirtioBlkReqHeader;

            // Allocate memory for status byte
            let status_phys = 0x50100200u64;
            let status_ptr = status_phys as *mut u8;

            // Fill in request header
            ptr::write_volatile(ptr::addr_of_mut!((*header).req_type), VIRTIO_BLK_T_IN);
            ptr::write_volatile(ptr::addr_of_mut!((*header).reserved), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*header).sector), sector);

            // Initialize status to 0xFF (will be overwritten by device)
            ptr::write_volatile(status_ptr, 0xFF);

            // Build 3-descriptor chain
            let d1 = self.virtq.alloc_desc(header as u64).ok_or("No descriptors available")?;
            let d2 = self.virtq.alloc_desc(buffer.as_ptr() as u64).ok_or("No descriptors available")?;
            let d3 = self.virtq.alloc_desc(status_phys).ok_or("No descriptors available")?;

            // Descriptor 1: Request header (read-only for device)
            (*self.virtq.desc.add(d1 as usize)).addr = header_phys;
            (*self.virtq.desc.add(d1 as usize)).len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;
            (*self.virtq.desc.add(d1 as usize)).flags = VIRTQ_DESC_F_NEXT;
            (*self.virtq.desc.add(d1 as usize)).next = d2;

            // Descriptor 2: Data buffer (write for device on read)
            (*self.virtq.desc.add(d2 as usize)).addr = buffer.as_ptr() as u64;
            (*self.virtq.desc.add(d2 as usize)).len = SECTOR_SIZE as u32;
            (*self.virtq.desc.add(d2 as usize)).flags = VIRTQ_DESC_F_WRITE | VIRTQ_DESC_F_NEXT;
            (*self.virtq.desc.add(d2 as usize)).next = d3;

            // Descriptor 3: Status byte (write for device)
            (*self.virtq.desc.add(d3 as usize)).addr = status_phys;
            (*self.virtq.desc.add(d3 as usize)).len = 1;
            (*self.virtq.desc.add(d3 as usize)).flags = VIRTQ_DESC_F_WRITE;
            (*self.virtq.desc.add(d3 as usize)).next = 0;

            // Add to available ring
            let avail_idx = ptr::read_volatile(ptr::addr_of!((*self.virtq.avail).idx));
            ptr::write_volatile(self.virtq.avail_ring.add(avail_idx as usize % QUEUE_SIZE as usize), d1);
            mb();
            ptr::write_volatile(ptr::addr_of_mut!((*self.virtq.avail).idx), avail_idx.wrapping_add(1));
            mb();

            // Notify device
            let queue_notify_off = ptr::read_volatile(&(*self.common_cfg).queue_notify_off);
            let notify_addr = self.notify_base + (queue_notify_off as u64 * self.notify_off_multiplier as u64);
            ptr::write_volatile(notify_addr as *mut u16, 0);
            mb();

            crate::kernel::uart_write_string("Read request submitted, waiting for completion...\r\n");

            // Poll for completion (busy wait)
            let start_used_idx = self.virtq.last_seen_used;
            loop {
                let used_idx = ptr::read_volatile(ptr::addr_of!((*self.virtq.used).idx));
                if used_idx != start_used_idx {
                    break;
                }
                // Small delay
                for _ in 0..1000 {
                    core::arch::asm!("nop");
                }
            }

            // Check status
            let final_status = ptr::read_volatile(status_ptr);
            if final_status != VIRTIO_BLK_S_OK {
                crate::kernel::uart_write_string(&alloc::format!(
                    "ERROR: Read failed with status {}\r\n", final_status
                ));
                return Err("Read failed");
            }

            // Free descriptors
            self.virtq.free_desc(d1);
            self.virtq.free_desc(d2);
            self.virtq.free_desc(d3);

            self.virtq.last_seen_used = ptr::read_volatile(ptr::addr_of!((*self.virtq.used).idx));

            crate::kernel::uart_write_string("Sector read successfully!\r\n");
            Ok(())
        }
    }
}
