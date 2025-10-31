// VirtIO Network Device Driver
// Based on VirtIO 1.3 specification

use crate::kernel::pci::{PciConfig, PciDevice};
use crate::kernel::memory;
use core::ptr;
use alloc::vec::Vec;

// VirtIO Device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_NET_DEVICE_ID_LEGACY: u16 = 0x1000;
const VIRTIO_NET_DEVICE_ID_MODERN: u16 = 0x1041;

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

// VirtIO Network Feature Bits
const VIRTIO_NET_F_CSUM: u32 = 1 << 0;
const VIRTIO_NET_F_MAC: u32 = 1 << 5;

// VirtIO Generic Feature Bits (bits 32+)
const VIRTIO_F_VERSION_1: u32 = 1 << 0;  // Bit 32 in features[1]

// Network packet constants
const QUEUE_SIZE: u16 = 128;
const MAX_PACKET_SIZE: usize = 1526; // 12-byte header + 1514-byte ethernet frame
const NET_HDR_SIZE: usize = 12;

// Memory barrier
#[inline(always)]
fn mb() {
    unsafe {
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
}

/// VirtIO Network Header (12 bytes in non-legacy mode)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct VirtioNetHdr {
    flags: u8,
    gso_type: u8,
    hdr_len: u16,
    gso_size: u16,
    csum_start: u16,
    csum_offset: u16,
    num_buffers: u16, // Only used if VIRTIO_NET_F_MRG_RXBUF
}

impl Default for VirtioNetHdr {
    fn default() -> Self {
        VirtioNetHdr {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
            num_buffers: 0,
        }
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
}

impl Virtqueue {
    /// Create a new virtqueue with the given size at a specific address
    unsafe fn new(size: u16, phys_addr: u64) -> Option<Self> {
        // Calculate memory layout according to VirtIO spec
        let desc_size = (size as usize) * core::mem::size_of::<VirtqDesc>();
        let avail_size = 6 + 2 * (size as usize);
        let used_size = 6 + 8 * (size as usize);
        let total_size = desc_size + avail_size + used_size + 64 + 4;

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
        })
    }

    /// Allocate a descriptor
    unsafe fn alloc_desc(&mut self) -> Option<u16> {
        let idx = self.free_desc;
        let desc_ptr = self.desc.add(idx as usize);

        // Update free list
        self.free_desc = (*desc_ptr).next;

        Some(idx)
    }

    /// Free a descriptor back to the free list
    unsafe fn free_desc(&mut self, idx: u16) {
        let desc_ptr = self.desc.add(idx as usize);
        (*desc_ptr).next = self.free_desc;
        (*desc_ptr).flags = 0;
        self.free_desc = idx;
    }
}

/// VirtIO Network Device
pub struct VirtioNetDevice {
    pci_device: PciDevice,
    common_cfg: *mut VirtioPciCommonCfg,
    notify_base: u64,
    notify_off_multiplier: u32,
    receiveq: Virtqueue,
    transmitq: Virtqueue,
    receiveq_notify_off: u16,
    transmitq_notify_off: u16,
    mac_addr: [u8; 6],
}

impl VirtioNetDevice {
    /// Find and initialize all VirtIO network devices
    pub fn find_and_init(ecam_base: u64, mmio_base: u64) -> Vec<VirtioNetDevice> {
        let mut devices = Vec::new();
        let config = PciConfig::with_base_addr(ecam_base);

        crate::kernel::uart_write_string("Scanning for VirtIO network devices...\r\n");

        // Scan PCI bus
        for device_num in 0..32 {
            if let Some(pci_dev) = PciDevice::new(0, device_num, 0, &config) {
                // Check if this is a VirtIO network device
                if pci_dev.vendor_id == VIRTIO_VENDOR_ID &&
                   (pci_dev.device_id == VIRTIO_NET_DEVICE_ID_MODERN ||
                    pci_dev.device_id == VIRTIO_NET_DEVICE_ID_LEGACY) {

                    crate::kernel::uart_write_string(&alloc::format!(
                        "Found VirtIO network device at 0:{}:0 (device_id=0x{:x})\r\n",
                        device_num, pci_dev.device_id
                    ));

                    if let Some(net_dev) = unsafe { Self::init_device(pci_dev, mmio_base) } {
                        devices.push(net_dev);
                    }
                }
            }
        }

        crate::kernel::uart_write_string(&alloc::format!(
            "Found {} VirtIO network device(s)\r\n", devices.len()
        ));

        devices
    }

    /// Initialize a VirtIO network device
    unsafe fn init_device(pci_dev: PciDevice, mmio_base: u64) -> Option<Self> {
        crate::kernel::uart_write_string("Initializing VirtIO network device...\r\n");

        // Enable bus mastering
        pci_dev.enable_bus_mastering();

        // Parse PCI capabilities to find VirtIO structures
        let (common_cfg_addr, notify_addr, notify_off_mult, device_cfg_addr) =
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

        // 4. Feature negotiation - read what device offers
        ptr::write_volatile(&mut (*common_cfg).device_feature_select, 0);
        mb();
        let device_features = ptr::read_volatile(&(*common_cfg).device_feature);
        crate::kernel::uart_write_string(&alloc::format!(
            "Device features[0]: 0x{:08x}\r\n", device_features
        ));

        // Check high 32 bits too
        ptr::write_volatile(&mut (*common_cfg).device_feature_select, 1);
        mb();
        let device_features_high = ptr::read_volatile(&(*common_cfg).device_feature);
        crate::kernel::uart_write_string(&alloc::format!(
            "Device features[1]: 0x{:08x}\r\n", device_features_high
        ));

        // Negotiate features - accept VIRTIO_NET_F_MAC and VIRTIO_NET_F_CSUM
        // (CSUM might be required for receive to work)
        let our_features = VIRTIO_NET_F_MAC | VIRTIO_NET_F_CSUM;
        let negotiated = device_features & our_features;

        crate::kernel::uart_write_string(&alloc::format!(
            "Negotiating features[0]: 0x{:08x}\r\n", negotiated
        ));

        ptr::write_volatile(&mut (*common_cfg).driver_feature_select, 0);
        ptr::write_volatile(&mut (*common_cfg).driver_feature, negotiated);
        mb();

        // Negotiate high 32-bit features (REQUIRED: VIRTIO_F_VERSION_1 for modern devices)
        let our_features_high = VIRTIO_F_VERSION_1;
        let negotiated_high = device_features_high & our_features_high;

        crate::kernel::uart_write_string(&alloc::format!(
            "Negotiating features[1]: 0x{:08x}\r\n", negotiated_high
        ));

        ptr::write_volatile(&mut (*common_cfg).driver_feature_select, 1);
        ptr::write_volatile(&mut (*common_cfg).driver_feature, negotiated_high);
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

        // Read MAC address from device config space
        let mac_addr = if device_cfg_addr != 0 {
            let mac_ptr = device_cfg_addr as *const u8;
            [
                ptr::read_volatile(mac_ptr.add(0)),
                ptr::read_volatile(mac_ptr.add(1)),
                ptr::read_volatile(mac_ptr.add(2)),
                ptr::read_volatile(mac_ptr.add(3)),
                ptr::read_volatile(mac_ptr.add(4)),
                ptr::read_volatile(mac_ptr.add(5)),
            ]
        } else {
            [0x52, 0x54, 0x00, 0x12, 0x34, 0x56] // Default QEMU MAC
        };

        crate::kernel::uart_write_string(&alloc::format!(
            "MAC address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\r\n",
            mac_addr[0], mac_addr[1], mac_addr[2], mac_addr[3], mac_addr[4], mac_addr[5]
        ));

        // 7. Set up virtqueues (receiveq = 0, transmitq = 1)
        // Allocate at fixed addresses as documented in CLAUDE.md
        let receiveq_addr = 0x50050000u64;
        let transmitq_addr = 0x50060000u64;

        let mut receiveq = Virtqueue::new(QUEUE_SIZE, receiveq_addr)?;
        let mut transmitq = Virtqueue::new(QUEUE_SIZE, transmitq_addr)?;

        // Setup receiveq (queue 0)
        let receiveq_notify_off = Self::setup_queue(&pci_dev, common_cfg, 0, &receiveq)?;

        // Setup transmitq (queue 1)
        let transmitq_notify_off = Self::setup_queue(&pci_dev, common_cfg, 1, &transmitq)?;

        crate::kernel::uart_write_string("Virtqueues configured and enabled\r\n");

        // 8. Set DRIVER_OK
        let status = ptr::read_volatile(&(*common_cfg).device_status);
        ptr::write_volatile(&mut (*common_cfg).device_status, status | VIRTIO_STATUS_DRIVER_OK);
        mb();

        crate::kernel::uart_write_string("Device ready!\r\n");

        Some(VirtioNetDevice {
            pci_device: pci_dev,
            common_cfg,
            notify_base: notify_addr,
            notify_off_multiplier: notify_off_mult,
            receiveq,
            transmitq,
            receiveq_notify_off,
            transmitq_notify_off,
            mac_addr,
        })
    }

    /// Parse PCI capabilities to find VirtIO structures
    unsafe fn parse_capabilities(pci_dev: &PciDevice, mmio_base: u64) -> Option<(u64, u64, u32, u64)> {
        let mut cap_ptr = pci_dev.get_capabilities_ptr()? as u16;
        let mut common_cfg_addr = None;
        let mut notify_addr = None;
        let mut notify_off_mult = 0u32;
        let mut device_cfg_addr = None;

        // Read and program BAR4 (where VirtIO capabilities point)
        let bar4_size = pci_dev.get_bar_size(4)?;
        // Allocate at 0x10500000 as documented in CLAUDE.md
        let bar4_addr = 0x10500000u64;

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
                        VIRTIO_PCI_CAP_DEVICE_CFG => {
                            device_cfg_addr = Some(addr);
                            crate::kernel::uart_write_string(&alloc::format!(
                                "Found device cfg at 0x{:x}\r\n", addr
                            ));
                        }
                        _ => {}
                    }
                }
            }

            cap_ptr = pci_dev.read_config_u8((cap_ptr + 1) as u8) as u16;
        }

        Some((common_cfg_addr?, notify_addr?, notify_off_mult, device_cfg_addr.unwrap_or(0)))
    }

    /// Setup a virtqueue
    unsafe fn setup_queue(pci_dev: &PciDevice, common_cfg: *mut VirtioPciCommonCfg,
                          queue_idx: u16, virtq: &Virtqueue) -> Option<u16> {
        // Select queue
        ptr::write_volatile(&mut (*common_cfg).queue_select, queue_idx);
        mb();

        let queue_size = ptr::read_volatile(&(*common_cfg).queue_size);

        // Validate queue size
        if queue_size == 0 || queue_size == 0xFFFF || queue_size > 1024 {
            crate::kernel::uart_write_string(&alloc::format!(
                "Invalid/broken queue size: {} - REJECTING DEVICE\r\n", queue_size
            ));
            return None;
        }

        crate::kernel::uart_write_string(&alloc::format!(
            "Queue {} size: {} - OK!\r\n", queue_idx, queue_size
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

        // Read notify offset before enabling
        let notify_off = ptr::read_volatile(&(*common_cfg).queue_notify_off);

        // Enable the queue
        ptr::write_volatile(&mut (*common_cfg).queue_enable, 1);
        mb();

        Some(notify_off)
    }

    /// Get MAC address
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_addr
    }

    /// Transmit a packet
    pub fn transmit(&mut self, packet: &[u8]) -> Result<(), &'static str> {
        if packet.len() > 1514 {
            return Err("Packet too large");
        }

        unsafe {
            // Allocate memory for header + packet
            let buffer_phys = 0x50070000u64;
            let header = buffer_phys as *mut VirtioNetHdr;
            let packet_data = (buffer_phys + NET_HDR_SIZE as u64) as *mut u8;

            // Fill in header (all zeros for simple packet)
            ptr::write_volatile(header, VirtioNetHdr::default());

            // Copy packet data
            for (i, &byte) in packet.iter().enumerate() {
                ptr::write_volatile(packet_data.add(i), byte);
            }

            mb();

            // Build 2-descriptor chain (header + data)
            let d1 = self.transmitq.alloc_desc().ok_or("No descriptors available")?;
            let d2 = self.transmitq.alloc_desc().ok_or("No descriptors available")?;

            // Descriptor 1: Header (read-only for device)
            (*self.transmitq.desc.add(d1 as usize)).addr = buffer_phys;
            (*self.transmitq.desc.add(d1 as usize)).len = NET_HDR_SIZE as u32;
            (*self.transmitq.desc.add(d1 as usize)).flags = VIRTQ_DESC_F_NEXT;
            (*self.transmitq.desc.add(d1 as usize)).next = d2;

            // Descriptor 2: Packet data (read-only for device)
            (*self.transmitq.desc.add(d2 as usize)).addr = packet_data as u64;
            (*self.transmitq.desc.add(d2 as usize)).len = packet.len() as u32;
            (*self.transmitq.desc.add(d2 as usize)).flags = 0; // No flags
            (*self.transmitq.desc.add(d2 as usize)).next = 0;

            // Add to available ring
            let avail_idx = ptr::read_volatile(ptr::addr_of!((*self.transmitq.avail).idx));
            ptr::write_volatile(self.transmitq.avail_ring.add(avail_idx as usize % QUEUE_SIZE as usize), d1);
            mb();
            ptr::write_volatile(ptr::addr_of_mut!((*self.transmitq.avail).idx), avail_idx.wrapping_add(1));
            mb();

            // Notify device
            let notify_addr = self.notify_base + (self.transmitq_notify_off as u64 * self.notify_off_multiplier as u64);
            ptr::write_volatile(notify_addr as *mut u16, 1); // Queue 1 = transmitq
            mb();

            // Poll for completion (busy wait with timeout)
            let start_used_idx = self.transmitq.last_seen_used;
            for _ in 0..100000 {
                let used_idx = ptr::read_volatile(ptr::addr_of!((*self.transmitq.used).idx));
                if used_idx != start_used_idx {
                    // Free descriptors
                    self.transmitq.free_desc(d1);
                    self.transmitq.free_desc(d2);
                    self.transmitq.last_seen_used = used_idx;
                    return Ok(());
                }
                for _ in 0..100 {
                    core::arch::asm!("nop");
                }
            }

            Err("Transmit timeout")
        }
    }

    /// Check for received packets (non-blocking)
    pub fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, &'static str> {
        unsafe {
            let used_idx = ptr::read_volatile(ptr::addr_of!((*self.receiveq.used).idx));

            if used_idx == self.receiveq.last_seen_used {
                return Err("No packets available");
            }

            // Get used buffer
            let ring_idx = (self.receiveq.last_seen_used % QUEUE_SIZE) as usize;
            let used_elem = ptr::read_volatile(&(*self.receiveq.used_ring.add(ring_idx)));

            let desc_idx = used_elem.id as u16;
            let total_len = used_elem.len as usize;

            // Read descriptor to get packet address
            let desc = ptr::read_volatile(&(*self.receiveq.desc.add(desc_idx as usize)));

            let packet_addr = desc.addr + NET_HDR_SIZE as u64; // Skip header
            let packet_len = if total_len > NET_HDR_SIZE {
                total_len - NET_HDR_SIZE
            } else {
                0
            };

            // Copy packet data to buffer
            let copy_len = packet_len.min(buffer.len());
            let src = packet_addr as *const u8;
            for i in 0..copy_len {
                buffer[i] = ptr::read_volatile(src.add(i));
            }

            // Free descriptor
            self.receiveq.free_desc(desc_idx);
            self.receiveq.last_seen_used = used_idx;

            Ok(copy_len)
        }
    }

    /// Add receive buffers to the receive queue
    pub fn add_receive_buffers(&mut self, count: usize) -> Result<(), &'static str> {
        unsafe {
            // Allocate memory for receive buffers starting at 0x50080000
            let mut buffer_addr = 0x50080000u64;

            for _ in 0..count {
                // Zero out the buffer memory
                ptr::write_bytes(buffer_addr as *mut u8, 0, 0x1000);

                // Allocate descriptor
                let desc_idx = self.receiveq.alloc_desc().ok_or("No descriptors available")?;

                // Setup descriptor (header + data, writable by device)
                (*self.receiveq.desc.add(desc_idx as usize)).addr = buffer_addr;
                (*self.receiveq.desc.add(desc_idx as usize)).len = MAX_PACKET_SIZE as u32;
                (*self.receiveq.desc.add(desc_idx as usize)).flags = VIRTQ_DESC_F_WRITE;
                (*self.receiveq.desc.add(desc_idx as usize)).next = 0;

                // Add to available ring
                let avail_idx = ptr::read_volatile(ptr::addr_of!((*self.receiveq.avail).idx));
                ptr::write_volatile(self.receiveq.avail_ring.add(avail_idx as usize % QUEUE_SIZE as usize), desc_idx);
                mb();
                ptr::write_volatile(ptr::addr_of_mut!((*self.receiveq.avail).idx), avail_idx.wrapping_add(1));
                mb();

                // Move to next buffer
                buffer_addr += 0x1000; // 4KB per buffer
            }

            // Notify device that buffers are available
            let notify_addr = self.notify_base + (self.receiveq_notify_off as u64 * self.notify_off_multiplier as u64);
            ptr::write_volatile(notify_addr as *mut u16, 0); // Queue 0 = receiveq
            mb();

            Ok(())
        }
    }
}
