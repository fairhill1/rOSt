// VirtIO-GPU driver with hardware cursor support
#![allow(dead_code)]

use crate::kernel::pci::{PciDevice, find_device};
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};

// VirtIO device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_GPU_DEVICE_ID: u16 = 0x1050;

// VirtIO GPU commands
const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
const VIRTIO_GPU_CMD_UPDATE_CURSOR: u32 = 0x0300;
const VIRTIO_GPU_CMD_MOVE_CURSOR: u32 = 0x0301;

// VirtIO GPU response types
const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;

// VirtIO GPU formats
const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM: u32 = 1;
const VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM: u32 = 2;
const VIRTIO_GPU_FORMAT_A8R8G8B8_UNORM: u32 = 3;
const VIRTIO_GPU_FORMAT_X8R8G8B8_UNORM: u32 = 4;

// VirtIO GPU flags
const VIRTIO_GPU_FLAG_FENCE: u32 = 1 << 0;

// VirtIO status register bits
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 128;

// Virtqueue descriptor flags
const VIRTQ_DESC_F_NEXT: u16 = 1;
const VIRTQ_DESC_F_WRITE: u16 = 2;

// Queue sizes
const CONTROLQ_SIZE: u16 = 64;
const CURSORQ_SIZE: u16 = 16;

// VirtIO common config register offsets
const VIRTIO_PCI_COMMON_DFSELECT: u64 = 0x00;
const VIRTIO_PCI_COMMON_DF: u64 = 0x04;
const VIRTIO_PCI_COMMON_GFSELECT: u64 = 0x08;
const VIRTIO_PCI_COMMON_GF: u64 = 0x0C;
const VIRTIO_PCI_COMMON_MSIXCFG: u64 = 0x10;
const VIRTIO_PCI_COMMON_NUMQ: u64 = 0x12;
const VIRTIO_PCI_COMMON_STATUS: u64 = 0x14;
const VIRTIO_PCI_COMMON_CFGGENERATION: u64 = 0x15;
const VIRTIO_PCI_COMMON_Q_SELECT: u64 = 0x16;
const VIRTIO_PCI_COMMON_Q_SIZE: u64 = 0x18;
const VIRTIO_PCI_COMMON_Q_MSIX: u64 = 0x1A;
const VIRTIO_PCI_COMMON_Q_ENABLE: u64 = 0x1C;
const VIRTIO_PCI_COMMON_Q_NOFF: u64 = 0x1E;
const VIRTIO_PCI_COMMON_Q_DESCLO: u64 = 0x20;
const VIRTIO_PCI_COMMON_Q_DESCHI: u64 = 0x24;
const VIRTIO_PCI_COMMON_Q_AVAILLO: u64 = 0x28;
const VIRTIO_PCI_COMMON_Q_AVAILHI: u64 = 0x2C;
const VIRTIO_PCI_COMMON_Q_USEDLO: u64 = 0x30;
const VIRTIO_PCI_COMMON_Q_USEDHI: u64 = 0x34;

// Virtqueue descriptor
#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

// Virtqueue available ring
#[repr(C, align(2))]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 64],
    used_event: u16,
}

// Virtqueue used element
#[repr(C)]
#[derive(Copy, Clone)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

// Virtqueue used ring
#[repr(C, align(4))]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 64],
    avail_event: u16,
}

// VirtIO GPU command structures
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VirtioGpuCtrlHdr {
    pub hdr_type: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub padding: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct VirtioGpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct VirtioGpuDisplayOne {
    r: VirtioGpuRect,
    enabled: u32,
    flags: u32,
}

#[repr(C, packed)]
struct VirtioGpuRespDisplayInfo {
    hdr: VirtioGpuCtrlHdr,
    pmodes: [VirtioGpuDisplayOne; 16],
}

#[repr(C, packed)]
struct VirtioGpuResourceCreate2d {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
}

#[repr(C, packed)]
struct VirtioGpuSetScanout {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    scanout_id: u32,
    resource_id: u32,
}

#[repr(C, packed)]
struct VirtioGpuResourceFlush {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    resource_id: u32,
    padding: u32,
}

#[repr(C, packed)]
struct VirtioGpuTransferToHost2d {
    hdr: VirtioGpuCtrlHdr,
    r: VirtioGpuRect,
    offset: u64,
    resource_id: u32,
    padding: u32,
}

#[repr(C, packed)]
struct VirtioGpuResourceAttachBacking {
    hdr: VirtioGpuCtrlHdr,
    resource_id: u32,
    nr_entries: u32,
}

#[repr(C, packed)]
struct VirtioGpuMemEntry {
    addr: u64,
    length: u32,
    padding: u32,
}

#[repr(C, packed)]
struct VirtioGpuCursorPos {
    scanout_id: u32,
    x: u32,
    y: u32,
    padding: u32,
}

#[repr(C, packed)]
struct VirtioGpuUpdateCursor {
    hdr: VirtioGpuCtrlHdr,
    pos: VirtioGpuCursorPos,
    resource_id: u32,
    hot_x: u32,
    hot_y: u32,
    padding: u32,
}

// Virtqueue implementation
struct Virtqueue {
    desc_table: *mut VirtqDesc,
    avail_ring: *mut VirtqAvail,
    used_ring: *mut VirtqUsed,
    queue_size: u16,
    last_used_idx: u16,
    free_head: u16,
    num_free: u16,
}

impl Virtqueue {
    fn new(size: u16, desc_addr: u64, avail_addr: u64, used_addr: u64) -> Self {
        let desc_table = desc_addr as *mut VirtqDesc;
        let avail_ring = avail_addr as *mut VirtqAvail;
        let used_ring = used_addr as *mut VirtqUsed;

        unsafe {
            // Initialize descriptor table with free list
            for i in 0..size {
                let desc = desc_table.add(i as usize);
                write_volatile(&mut (*desc).addr, 0);
                write_volatile(&mut (*desc).len, 0);
                write_volatile(&mut (*desc).flags, 0);
                write_volatile(&mut (*desc).next, (i + 1) % size);
            }

            // Initialize available ring
            write_volatile(&mut (*avail_ring).flags, 0);
            write_volatile(&mut (*avail_ring).idx, 0);

            // Initialize used ring
            write_volatile(&mut (*used_ring).flags, 0);
            write_volatile(&mut (*used_ring).idx, 0);
        }

        Virtqueue {
            desc_table,
            avail_ring,
            used_ring,
            queue_size: size,
            last_used_idx: 0,
            free_head: 0,
            num_free: size,
        }
    }

    fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }
        let desc_idx = self.free_head;
        self.free_head = unsafe { read_volatile(&(*self.desc_table.add(desc_idx as usize)).next) };
        self.num_free -= 1;
        Some(desc_idx)
    }

    fn free_desc(&mut self, desc_idx: u16) {
        unsafe {
            write_volatile(&mut (*self.desc_table.add(desc_idx as usize)).next, self.free_head);
        }
        self.free_head = desc_idx;
        self.num_free += 1;
    }

    fn add_buffer(&mut self, buffers: &[(u64, u32, bool)]) -> Option<u16> {
        if buffers.is_empty() || buffers.len() > self.num_free as usize {
            return None;
        }

        let head = self.alloc_desc()?;
        let mut current = head;

        for (i, &(addr, len, device_writable)) in buffers.iter().enumerate() {
            let is_last = i == buffers.len() - 1;

            unsafe {
                let desc = self.desc_table.add(current as usize);
                write_volatile(&mut (*desc).addr, addr);
                write_volatile(&mut (*desc).len, len);

                let mut flags = 0u16;
                if device_writable {
                    flags |= VIRTQ_DESC_F_WRITE;
                }
                if !is_last {
                    flags |= VIRTQ_DESC_F_NEXT;
                    let next = self.alloc_desc()?;
                    write_volatile(&mut (*desc).next, next);
                    write_volatile(&mut (*desc).flags, flags);
                    current = next;
                } else {
                    write_volatile(&mut (*desc).flags, flags);
                }
            }
        }

        // Add to available ring
        unsafe {
            let avail_idx = read_volatile(&(*self.avail_ring).idx);
            let ring_idx = (avail_idx % self.queue_size) as usize;
            write_volatile(&mut (*self.avail_ring).ring[ring_idx], head);
            write_volatile(&mut (*self.avail_ring).idx, avail_idx.wrapping_add(1));
        }

        Some(head)
    }

    fn get_used_buffer(&mut self) -> Option<(u16, u32)> {
        unsafe {
            let used_idx = read_volatile(&(*self.used_ring).idx);
            if self.last_used_idx == used_idx {
                return None;
            }

            let ring_idx = (self.last_used_idx % self.queue_size) as usize;
            let elem = read_volatile(&(*self.used_ring).ring[ring_idx]);
            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            Some((elem.id as u16, elem.len))
        }
    }
}

pub struct VirtioGpuDriver {
    pci_device: PciDevice,
    common_cfg: u64,
    notify_base: u64,
    notify_off_multiplier: u32,
    controlq: Option<Virtqueue>,
    cursorq: Option<Virtqueue>,
    controlq_notify_off: u16,
    cursorq_notify_off: u16,
    width: u32,
    height: u32,
    framebuffer_addr: u64,
    framebuffer_resource_id: u32,
    cursor_resource_id: u32,
    cursor_cmd_buffer: u64, // Reusable buffer for cursor commands
    control_cmd_buffer: u64, // Reusable buffer for control commands
    control_resp_buffer: u64, // Reusable buffer for control responses
}

impl VirtioGpuDriver {
    pub fn new() -> Option<Self> {
        if let Some(pci_device) = find_device(VIRTIO_VENDOR_ID, VIRTIO_GPU_DEVICE_ID) {
            pci_device.enable_memory_access();
            pci_device.enable_bus_mastering();

            Some(VirtioGpuDriver {
                pci_device,
                common_cfg: 0,
                notify_base: 0,
                notify_off_multiplier: 0,
                controlq: None,
                cursorq: None,
                controlq_notify_off: 0,
                cursorq_notify_off: 0,
                width: 0,
                height: 0,
                framebuffer_addr: 0,
                framebuffer_resource_id: 1,
                cursor_resource_id: 2,
                cursor_cmd_buffer: 0,
                control_cmd_buffer: 0,
                control_resp_buffer: 0,
            })
        } else {
            None
        }
    }

    fn allocate_bar4(&mut self) -> Result<(), &'static str> {
        // VirtIO GPU is typically the first GPU device on the PCI bus (device 1)
        // Allocate BAR4 at 0x10100000 (between device 0 and device 2)
        let bar4_address = 0x10100000u64;

        crate::kernel::uart_write_string("VirtIO GPU: Allocating BAR4 at 0x");
        let mut addr = bar4_address;
        for _ in 0..16 {
            let digit = (addr >> 60) & 0xF;
            let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
            unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
            addr <<= 4;
        }
        crate::kernel::uart_write_string("\r\n");

        // Program BAR4 (64-bit BAR, so we need BAR4 and BAR5)
        self.pci_device.write_config_u32(0x20, bar4_address as u32); // Lower 32 bits
        self.pci_device.write_config_u32(0x24, (bar4_address >> 32) as u32); // Upper 32 bits

        // Verify
        let readback = self.pci_device.read_config_u32(0x20);
        crate::kernel::uart_write_string("VirtIO GPU: BAR4 readback: 0x");
        let mut rb = readback as u64;
        for _ in 0..8 {
            let digit = (rb >> 28) & 0xF;
            let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
            unsafe { core::ptr::write_volatile(0x09000000 as *mut u8, ch); }
            rb <<= 4;
        }
        crate::kernel::uart_write_string("\r\n");

        Ok(())
    }

    pub fn initialize(&mut self) -> Result<(), &'static str> {
        crate::kernel::uart_write_string("VirtIO GPU: Starting initialization\r\n");

        // Allocate BAR4 before reading capabilities
        self.allocate_bar4()?;

        // Find VirtIO capabilities
        self.find_capabilities()?;

        // Reset device
        self.reset_device();

        // Set ACKNOWLEDGE
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE);

        // Set DRIVER
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);

        // Negotiate features
        self.negotiate_features()?;

        // Set FEATURES_OK
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);

        // Verify FEATURES_OK
        if !self.check_features_ok() {
            return Err("Device rejected features");
        }

        // Set up virtqueues
        self.setup_virtqueues()?;

        // Allocate reusable buffers
        self.cursor_cmd_buffer = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate cursor buffer")?;
        self.control_cmd_buffer = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate control cmd buffer")?;
        self.control_resp_buffer = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate control resp buffer")?;

        // Set DRIVER_OK
        self.set_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK);

        crate::kernel::uart_write_string("VirtIO GPU: Device initialized\r\n");
        Ok(())
    }

    fn find_capabilities(&mut self) -> Result<(), &'static str> {
        let mut cap_ptr = self.pci_device.read_capability_pointer().ok_or("No capabilities")?;

        let mut common_cfg_bar = None;
        let mut common_cfg_offset = None;
        let mut notify_bar = None;
        let mut notify_offset = None;
        let mut notify_off_multiplier = None;

        while cap_ptr != 0 {
            let cap_id = self.pci_device.read_config_u8(cap_ptr);
            let next_ptr = self.pci_device.read_config_u8(cap_ptr + 1);

            if cap_id == 0x09 { // VirtIO capability
                let cfg_type = self.pci_device.read_config_u8(cap_ptr + 3);
                let bar = self.pci_device.read_config_u8(cap_ptr + 4);
                let offset = self.pci_device.read_config_u32(cap_ptr + 8);

                match cfg_type {
                    1 => { // Common config
                        common_cfg_bar = Some(bar);
                        common_cfg_offset = Some(offset);
                    }
                    2 => { // Notify config
                        notify_bar = Some(bar);
                        notify_offset = Some(offset);
                        notify_off_multiplier = Some(self.pci_device.read_config_u32(cap_ptr + 16));
                    }
                    _ => {}
                }
            }

            cap_ptr = next_ptr;
        }

        // Calculate actual addresses
        if let (Some(bar), Some(offset)) = (common_cfg_bar, common_cfg_offset) {
            let bar_addr = self.pci_device.get_bar_address(bar).ok_or("Invalid BAR")?;
            self.common_cfg = bar_addr + offset as u64;
        } else {
            return Err("No common config found");
        }

        if let (Some(bar), Some(offset)) = (notify_bar, notify_offset) {
            let bar_addr = self.pci_device.get_bar_address(bar).ok_or("Invalid BAR")?;
            self.notify_base = bar_addr + offset as u64;
            self.notify_off_multiplier = notify_off_multiplier.unwrap_or(0);
        } else {
            return Err("No notify config found");
        }

        crate::kernel::uart_write_string("VirtIO GPU: Found capabilities\r\n");
        Ok(())
    }

    fn reset_device(&self) {
        unsafe {
            let status_reg = (self.common_cfg + VIRTIO_PCI_COMMON_STATUS) as *mut u8;
            write_volatile(status_reg, 0);
        }
    }

    fn set_status(&self, status: u8) {
        unsafe {
            let status_reg = (self.common_cfg + VIRTIO_PCI_COMMON_STATUS) as *mut u8;
            write_volatile(status_reg, status);
        }
    }

    fn get_status(&self) -> u8 {
        unsafe {
            let status_reg = (self.common_cfg + VIRTIO_PCI_COMMON_STATUS) as *mut u8;
            read_volatile(status_reg)
        }
    }

    fn check_features_ok(&self) -> bool {
        (self.get_status() & VIRTIO_STATUS_FEATURES_OK) != 0
    }

    fn negotiate_features(&self) -> Result<(), &'static str> {
        unsafe {
            let dfselect_reg = (self.common_cfg + VIRTIO_PCI_COMMON_DFSELECT) as *mut u32;
            let df_reg = (self.common_cfg + VIRTIO_PCI_COMMON_DF) as *mut u32;
            let gfselect_reg = (self.common_cfg + VIRTIO_PCI_COMMON_GFSELECT) as *mut u32;
            let gf_reg = (self.common_cfg + VIRTIO_PCI_COMMON_GF) as *mut u32;

            // Read device features[0:31]
            write_volatile(dfselect_reg, 0);
            let device_features_low = read_volatile(df_reg);

            // Read device features[32:63]
            write_volatile(dfselect_reg, 1);
            let device_features_high = read_volatile(df_reg);

            // Accept all features for now
            write_volatile(gfselect_reg, 0);
            write_volatile(gf_reg, device_features_low);
            write_volatile(gfselect_reg, 1);
            write_volatile(gf_reg, device_features_high);
        }

        Ok(())
    }

    fn setup_virtqueues(&mut self) -> Result<(), &'static str> {
        // Allocate memory for control queue
        let controlq_desc_addr = self.alloc_queue_memory()?;
        let controlq_avail_addr = controlq_desc_addr + (CONTROLQ_SIZE as u64 * 16);
        let controlq_used_addr = controlq_avail_addr + (4 + CONTROLQ_SIZE as u64 * 2 + 2);

        // Allocate memory for cursor queue
        let cursorq_desc_addr = self.alloc_queue_memory()?;
        let cursorq_avail_addr = cursorq_desc_addr + (CURSORQ_SIZE as u64 * 16);
        let cursorq_used_addr = cursorq_avail_addr + (4 + CURSORQ_SIZE as u64 * 2 + 2);

        // Setup control queue (queue 0)
        self.controlq_notify_off = self.setup_queue(0, CONTROLQ_SIZE, controlq_desc_addr, controlq_avail_addr, controlq_used_addr)?;
        self.controlq = Some(Virtqueue::new(CONTROLQ_SIZE, controlq_desc_addr, controlq_avail_addr, controlq_used_addr));

        // Setup cursor queue (queue 1)
        self.cursorq_notify_off = self.setup_queue(1, CURSORQ_SIZE, cursorq_desc_addr, cursorq_avail_addr, cursorq_used_addr)?;
        self.cursorq = Some(Virtqueue::new(CURSORQ_SIZE, cursorq_desc_addr, cursorq_avail_addr, cursorq_used_addr));

        crate::kernel::uart_write_string("VirtIO GPU: Virtqueues set up\r\n");
        Ok(())
    }

    fn alloc_queue_memory(&self) -> Result<u64, &'static str> {
        crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate queue memory")
    }

    fn setup_queue(&self, queue_idx: u16, queue_size: u16, desc_addr: u64, avail_addr: u64, used_addr: u64) -> Result<u16, &'static str> {
        unsafe {
            let qselect_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_SELECT) as *mut u16;
            let qsize_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_SIZE) as *mut u16;
            let qenable_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_ENABLE) as *mut u16;
            let qnoff_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_NOFF) as *mut u16;
            let qdesclo_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_DESCLO) as *mut u32;
            let qdeschi_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_DESCHI) as *mut u32;
            let qavaillow_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_AVAILLO) as *mut u32;
            let qavailhi_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_AVAILHI) as *mut u32;
            let qusedlo_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_USEDLO) as *mut u32;
            let qusedhi_reg = (self.common_cfg + VIRTIO_PCI_COMMON_Q_USEDHI) as *mut u32;

            // Select queue
            write_volatile(qselect_reg, queue_idx);

            // Set queue size
            write_volatile(qsize_reg, queue_size);

            // Set descriptor table address
            write_volatile(qdesclo_reg, (desc_addr & 0xFFFFFFFF) as u32);
            write_volatile(qdeschi_reg, (desc_addr >> 32) as u32);

            // Set available ring address
            write_volatile(qavaillow_reg, (avail_addr & 0xFFFFFFFF) as u32);
            write_volatile(qavailhi_reg, (avail_addr >> 32) as u32);

            // Set used ring address
            write_volatile(qusedlo_reg, (used_addr & 0xFFFFFFFF) as u32);
            write_volatile(qusedhi_reg, (used_addr >> 32) as u32);

            // Read notify offset
            let notify_off = read_volatile(qnoff_reg);

            // Enable queue
            write_volatile(qenable_reg, 1);

            Ok(notify_off)
        }
    }

    fn notify_queue(&self, queue_idx: u16, notify_off: u16) {
        unsafe {
            let notify_addr = self.notify_base + (notify_off as u64 * self.notify_off_multiplier as u64);
            let notify_reg = notify_addr as *mut u16;
            write_volatile(notify_reg, queue_idx);
        }
    }

    fn send_command(&mut self, cmd: &[u8], resp: &mut [u8]) -> Result<(), &'static str> {

        // Use pre-allocated reusable buffers
        let cmd_buf = self.control_cmd_buffer;
        let resp_buf = self.control_resp_buffer;

        // Copy command to buffer
        unsafe {
            let cmd_ptr = cmd_buf as *mut u8;
            for (i, &byte) in cmd.iter().enumerate() {
                write_volatile(cmd_ptr.add(i), byte);
            }
        }

        // Add buffers to queue
        let buffers = [
            (cmd_buf, cmd.len() as u32, false),
            (resp_buf, resp.len() as u32, true),
        ];

        let desc_idx = {
            let controlq = self.controlq.as_mut().ok_or("Control queue not initialized")?;
            controlq.add_buffer(&buffers).ok_or("Failed to add buffer to queue")?
        };

        // Notify device
        let notify_off = self.controlq_notify_off;
        self.notify_queue(0, notify_off);

        // Wait for response
        for i in 0..1000000 {
            let controlq = self.controlq.as_mut().ok_or("Control queue not initialized")?;
            if let Some((used_idx, _len)) = controlq.get_used_buffer() {
                if used_idx == desc_idx {
                    // Copy response
                    unsafe {
                        let resp_ptr = resp_buf as *const u8;
                        for (i, byte) in resp.iter_mut().enumerate() {
                            *byte = read_volatile(resp_ptr.add(i));
                        }
                    }

                    // Free descriptors (buffers are reused, don't free them)
                    controlq.free_desc(desc_idx);
                    if buffers.len() > 1 {
                        controlq.free_desc((desc_idx + 1) % controlq.queue_size);
                    }

                    return Ok(());
                }
            }

            // Print progress every 100000 iterations
            if i % 100000 == 0 && i > 0 {
                crate::kernel::uart_write_string(".");
            }
        }

        crate::kernel::uart_write_string("\r\nVirtIO GPU: Command timeout!\r\n");
        Err("Command timeout")
    }

    pub fn get_display_info(&mut self) -> Result<(), &'static str> {
        let cmd = VirtioGpuCtrlHdr {
            hdr_type: VIRTIO_GPU_CMD_GET_DISPLAY_INFO,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let mut resp = VirtioGpuRespDisplayInfo {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: 0,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            pmodes: [VirtioGpuDisplayOne {
                r: VirtioGpuRect { x: 0, y: 0, width: 0, height: 0 },
                enabled: 0,
                flags: 0,
            }; 16],
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuRespDisplayInfo>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        // Extract display info
        unsafe {
            let pmode0_ptr = &resp.pmodes[0] as *const VirtioGpuDisplayOne;
            let pmode0_addr = pmode0_ptr as usize;
            // Calculate offsets within packed struct
            let rect_offset = core::mem::offset_of!(VirtioGpuDisplayOne, r);
            let width_offset = core::mem::offset_of!(VirtioGpuRect, width);
            let height_offset = core::mem::offset_of!(VirtioGpuRect, height);

            let width_ptr = (pmode0_addr + rect_offset + width_offset) as *const u32;
            let height_ptr = (pmode0_addr + rect_offset + height_offset) as *const u32;

            self.width = core::ptr::read_unaligned(width_ptr);
            self.height = core::ptr::read_unaligned(height_ptr);
        }

        if self.width == 0 || self.height == 0 {
            self.width = 1024;
            self.height = 768;
        }

        crate::kernel::uart_write_string("VirtIO GPU: Display ");
        // Print width/height
        crate::kernel::uart_write_string("\r\n");

        Ok(())
    }

    pub fn create_framebuffer(&mut self) -> Result<(), &'static str> {
        // Allocate framebuffer memory
        let fb_size = (self.width * self.height * 4) as usize;
        let fb_pages = (fb_size + 4095) / 4096;

        self.framebuffer_addr = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate framebuffer")?;

        // Create 2D resource
        self.create_2d_resource(self.framebuffer_resource_id, self.width, self.height)?;

        // Attach backing memory
        self.attach_backing(self.framebuffer_resource_id, self.framebuffer_addr, fb_size as u32)?;

        // Clear framebuffer to black
        unsafe {
            let fb_ptr = self.framebuffer_addr as *mut u32;
            for i in 0..(self.width * self.height) {
                write_volatile(fb_ptr.add(i as usize), 0xFF000000);
            }
        }

        // Transfer to host
        self.transfer_to_host_2d(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;

        // Set scanout
        self.set_scanout(0, self.framebuffer_resource_id, 0, 0, self.width, self.height)?;

        // Flush
        self.flush_resource(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;

        crate::kernel::uart_write_string("VirtIO GPU: Framebuffer created\r\n");
        Ok(())
    }

    fn create_2d_resource(&mut self, resource_id: u32, width: u32, height: u32) -> Result<(), &'static str> {
        let cmd = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            resource_id,
            format: VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM,
            width,
            height,
        };

        let mut resp_hdr = VirtioGpuCtrlHdr {
            hdr_type: 0,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<VirtioGpuResourceCreate2d>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp_hdr as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        Ok(())
    }

    fn attach_backing(&mut self, resource_id: u32, addr: u64, length: u32) -> Result<(), &'static str> {
        // Combined command with mem entry
        #[repr(C, packed)]
        struct AttachBackingCmd {
            hdr: VirtioGpuResourceAttachBacking,
            entry: VirtioGpuMemEntry,
        }

        let cmd = AttachBackingCmd {
            hdr: VirtioGpuResourceAttachBacking {
                hdr: VirtioGpuCtrlHdr {
                    hdr_type: VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING,
                    flags: 0,
                    fence_id: 0,
                    ctx_id: 0,
                    padding: 0,
                },
                resource_id,
                nr_entries: 1,
            },
            entry: VirtioGpuMemEntry {
                addr,
                length,
                padding: 0,
            },
        };

        let mut resp_hdr = VirtioGpuCtrlHdr {
            hdr_type: 0,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<AttachBackingCmd>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp_hdr as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        Ok(())
    }

    fn transfer_to_host_2d(&mut self, resource_id: u32, x: u32, y: u32, width: u32, height: u32) -> Result<(), &'static str> {
        let cmd = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect { x, y, width, height },
            offset: 0,
            resource_id,
            padding: 0,
        };

        let mut resp_hdr = VirtioGpuCtrlHdr {
            hdr_type: 0,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<VirtioGpuTransferToHost2d>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp_hdr as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        Ok(())
    }

    fn set_scanout(&mut self, scanout_id: u32, resource_id: u32, x: u32, y: u32, width: u32, height: u32) -> Result<(), &'static str> {
        let cmd = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect { x, y, width, height },
            scanout_id,
            resource_id,
        };

        let mut resp_hdr = VirtioGpuCtrlHdr {
            hdr_type: 0,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<VirtioGpuSetScanout>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp_hdr as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        Ok(())
    }

    fn flush_resource(&mut self, resource_id: u32, x: u32, y: u32, width: u32, height: u32) -> Result<(), &'static str> {
        let cmd = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect { x, y, width, height },
            resource_id,
            padding: 0,
        };

        let mut resp_hdr = VirtioGpuCtrlHdr {
            hdr_type: 0,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            padding: 0,
        };

        let cmd_bytes = unsafe {
            core::slice::from_raw_parts(&cmd as *const _ as *const u8, core::mem::size_of::<VirtioGpuResourceFlush>())
        };

        let resp_bytes = unsafe {
            core::slice::from_raw_parts_mut(&mut resp_hdr as *mut _ as *mut u8, core::mem::size_of::<VirtioGpuCtrlHdr>())
        };

        self.send_command(cmd_bytes, resp_bytes)?;

        Ok(())
    }

    pub fn create_cursor(&mut self, cursor_data: &[u32; 64 * 64]) -> Result<(), &'static str> {
        // Allocate cursor memory (64x64 RGBA)
        let cursor_addr = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate cursor memory")?;

        // Copy cursor data
        unsafe {
            let cursor_ptr = cursor_addr as *mut u32;
            for (i, &pixel) in cursor_data.iter().enumerate() {
                write_volatile(cursor_ptr.add(i), pixel);
            }
        }

        // Create 64x64 resource
        self.create_2d_resource(self.cursor_resource_id, 64, 64)?;

        // Attach backing memory
        self.attach_backing(self.cursor_resource_id, cursor_addr, (64 * 64 * 4) as u32)?;

        // Transfer to host
        self.transfer_to_host_2d(self.cursor_resource_id, 0, 0, 64, 64)?;

        crate::kernel::uart_write_string("VirtIO GPU: Cursor created\r\n");
        Ok(())
    }

    pub fn update_cursor(&mut self, x: u32, y: u32, hot_x: u32, hot_y: u32) -> Result<(), &'static str> {
        let cursorq = self.cursorq.as_mut().ok_or("Cursor queue not initialized")?;

        let cmd = VirtioGpuUpdateCursor {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_UPDATE_CURSOR,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            pos: VirtioGpuCursorPos {
                scanout_id: 0,
                x,
                y,
                padding: 0,
            },
            resource_id: self.cursor_resource_id,
            hot_x,
            hot_y,
            padding: 0,
        };

        // Allocate buffer for command
        let cmd_buf = crate::kernel::memory::alloc_physical_page().ok_or("Failed to allocate cmd buffer")?;

        // Copy command to buffer
        unsafe {
            let cmd_ptr = cmd_buf as *mut VirtioGpuUpdateCursor;
            write_volatile(cmd_ptr, cmd);
        }

        // Add buffer to cursor queue (no response needed)
        let buffers = [(cmd_buf, core::mem::size_of::<VirtioGpuUpdateCursor>() as u32, false)];
        let desc_idx = cursorq.add_buffer(&buffers).ok_or("Failed to add buffer to queue")?;

        // Notify device
        self.notify_queue(1, self.cursorq_notify_off);

        Ok(())
    }

    pub fn move_cursor(&mut self, x: u32, y: u32) -> Result<(), &'static str> {
        static mut CURSOR_MOVE_COUNT: u32 = 0;
        unsafe {
            CURSOR_MOVE_COUNT += 1;
            if CURSOR_MOVE_COUNT <= 20 {
                crate::kernel::uart_write_string("VirtIO GPU: move_cursor to ");
                let mut pos = x as u64;
                for _ in 0..8 {
                    let digit = (pos >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    core::ptr::write_volatile(0x09000000 as *mut u8, ch);
                    pos <<= 4;
                }
                crate::kernel::uart_write_string(",");
                let mut pos = y as u64;
                for _ in 0..8 {
                    let digit = (pos >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    core::ptr::write_volatile(0x09000000 as *mut u8, ch);
                    pos <<= 4;
                }
                crate::kernel::uart_write_string("\r\n");
            }
        }

        // Use static buffer to avoid allocating/freeing on every move
        let cmd = VirtioGpuUpdateCursor {
            hdr: VirtioGpuCtrlHdr {
                hdr_type: VIRTIO_GPU_CMD_MOVE_CURSOR,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                padding: 0,
            },
            pos: VirtioGpuCursorPos {
                scanout_id: 0,
                x,
                y,
                padding: 0,
            },
            resource_id: self.cursor_resource_id, // Keep resource_id to prevent hiding
            hot_x: 0,
            hot_y: 0,
            padding: 0,
        };

        // Write to static buffer
        unsafe {
            let cmd_ptr = self.cursor_cmd_buffer as *mut VirtioGpuUpdateCursor;
            write_volatile(cmd_ptr, cmd);
        }

        // Directly write to descriptor 0 of cursor queue (dedicated for cursor moves)
        let cursorq = self.cursorq.as_mut().ok_or("Cursor queue not initialized")?;
        unsafe {
            // Use descriptor 0 exclusively for cursor moves
            let desc = cursorq.desc_table.add(0);
            write_volatile(&mut (*desc).addr, self.cursor_cmd_buffer);
            write_volatile(&mut (*desc).len, core::mem::size_of::<VirtioGpuUpdateCursor>() as u32);
            write_volatile(&mut (*desc).flags, 0); // No NEXT, no WRITE

            // Add to available ring
            let avail_idx = read_volatile(&(*cursorq.avail_ring).idx);
            let ring_idx = (avail_idx % cursorq.queue_size) as usize;
            write_volatile(&mut (*cursorq.avail_ring).ring[ring_idx], 0); // Always use descriptor 0

            // Memory barrier before updating idx
            core::arch::asm!("dmb ishst");
            write_volatile(&mut (*cursorq.avail_ring).idx, avail_idx.wrapping_add(1));
        }

        // Notify device
        self.notify_queue(1, self.cursorq_notify_off);

        Ok(())
    }

    pub fn get_framebuffer_info(&self) -> (u64, u32, u32, u32) {
        (self.framebuffer_addr, self.width, self.height, self.width * 4)
    }

    pub fn draw_test_pattern(&mut self) -> Result<(), &'static str> {
        if self.framebuffer_addr == 0 {
            return Err("Framebuffer not initialized");
        }

        unsafe {
            let fb_ptr = self.framebuffer_addr as *mut u32;

            // Draw gradient background
            for y in 0..self.height {
                for x in 0..self.width {
                    let r = ((x * 255) / self.width) as u32;
                    let g = ((y * 255) / self.height) as u32;
                    let b = 128;
                    let color = 0xFF000000 | (r << 16) | (g << 8) | b;
                    write_volatile(fb_ptr.add((y * self.width + x) as usize), color);
                }
            }
        }

        // Transfer and flush
        self.transfer_to_host_2d(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;
        self.flush_resource(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;

        crate::kernel::uart_write_string("VirtIO GPU: Test pattern drawn\r\n");
        Ok(())
    }

    pub fn create_default_cursor(&mut self) -> Result<(), &'static str> {
        let mut cursor_data = [0u32; 64 * 64];

        // Classic arrow cursor with black outline and white fill - 20x20 size
        const BLACK: u32 = 0xFF000000;
        const WHITE: u32 = 0xFFFFFFFF;
        const TRANS: u32 = 0x00000000;

        // Define cursor pixel by pixel (20 rows)
        #[rustfmt::skip]
        const CURSOR: [[u32; 20]; 20] = [
            [BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, WHITE, WHITE, WHITE, BLACK, BLACK, BLACK, BLACK, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, WHITE, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, WHITE, BLACK, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, WHITE, BLACK, TRANS, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [BLACK, BLACK, TRANS, TRANS, TRANS, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, BLACK, WHITE, WHITE, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
            [TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, BLACK, BLACK, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS, TRANS],
        ];

        // Copy cursor to 64x64 buffer
        for y in 0..20 {
            for x in 0..20 {
                let idx = (y * 64 + x) as usize;
                cursor_data[idx] = CURSOR[y][x];
            }
        }

        self.create_cursor(&cursor_data)?;

        // Initialize cursor at center of screen
        let center_x = self.width / 2;
        let center_y = self.height / 2;
        self.update_cursor(center_x, center_y, 0, 0)?;

        crate::kernel::uart_write_string("VirtIO GPU: Hardware cursor initialized\r\n");
        Ok(())
    }

    pub fn handle_mouse_move(&mut self, x_delta: i32, y_delta: i32, screen_width: u32, screen_height: u32, cursor_x: &mut u32, cursor_y: &mut u32) {
        // Apply direct movement (no damping for now to test)
        let new_x = (*cursor_x as i32 + x_delta).max(0).min(screen_width as i32 - 1) as u32;
        let new_y = (*cursor_y as i32 + y_delta).max(0).min(screen_height as i32 - 1) as u32;

        if new_x != *cursor_x || new_y != *cursor_y {
            *cursor_x = new_x;
            *cursor_y = new_y;

            // Move hardware cursor (ignore errors during mouse movement)
            let _ = self.move_cursor(new_x, new_y);
        }
    }

    // Flush framebuffer to display (transfer + flush)
    pub fn flush_display(&mut self) -> Result<(), &'static str> {
        self.transfer_to_host_2d(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;
        self.flush_resource(self.framebuffer_resource_id, 0, 0, self.width, self.height)?;
        Ok(())
    }
}
