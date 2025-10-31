// VirtIO Input Device Driver - Keyboard and Mouse Support for QEMU

extern crate alloc;
use alloc::vec::Vec;
use crate::kernel::pci::PciDevice;
use crate::kernel::uart_write_string;
use crate::kernel::usb_hid::{InputEvent, queue_input_event};

// VirtIO Input Device IDs
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_INPUT_DEVICE_ID: u16 = 0x1052; // VirtIO Input (modern)
const VIRTIO_INPUT_DEVICE_ID_LEGACY: u16 = 0x1012; // VirtIO Input (legacy)

// Modifier keys (Linux evdev key codes)
const KEY_LEFT_CTRL: u16 = 29;
const KEY_RIGHT_CTRL: u16 = 97;
const KEY_LEFT_SHIFT: u16 = 42;
const KEY_RIGHT_SHIFT: u16 = 54;
const KEY_LEFT_ALT: u16 = 56;
const KEY_RIGHT_ALT: u16 = 100;

// VirtIO Input Event Types (Linux input event codes)
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

// VirtIO Input Config Select values
const VIRTIO_INPUT_CFG_UNSET: u8 = 0x00;
const VIRTIO_INPUT_CFG_ID_NAME: u8 = 0x01;
const VIRTIO_INPUT_CFG_ID_SERIAL: u8 = 0x02;
const VIRTIO_INPUT_CFG_ID_DEVIDS: u8 = 0x03;
const VIRTIO_INPUT_CFG_PROP_BITS: u8 = 0x10;
const VIRTIO_INPUT_CFG_EV_BITS: u8 = 0x11;
const VIRTIO_INPUT_CFG_ABS_INFO: u8 = 0x12;

// Relative axes
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_WHEEL: u16 = 0x08;

// Absolute axes (for tablet)
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

// Mouse buttons
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;

// VirtIO Input Event structure
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct VirtioInputEvent {
    event_type: u16,
    code: u16,
    value: i32,
}

// Virtqueue descriptor
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

// Virtqueue available ring
#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 16],  // Queue size of 16
}

// Virtqueue used ring element
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

// Virtqueue used ring
#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 16],  // Queue size of 16
}

// VirtIO device structure
pub struct VirtioInputDevice {
    pci_device: PciDevice,
    base_addr: u64,
    notify_addr: u64,
    device_config_addr: u64,  // Device-specific config space
    notify_off_multiplier: u32,
    queue_notify_off: u16,  // Notify offset for queue 0
    event_queue_addr: u64,
    device_type: InputDeviceType,
    // Virtqueue structures
    desc_table: *mut VirtqDesc,
    avail_ring: *mut VirtqAvail,
    used_ring: *mut VirtqUsed,
    event_buffers: [*mut VirtioInputEvent; 16],
    last_used_idx: u16,
    queue_size: u16,
    // Modifier key state tracking
    modifier_state: u8,
}

#[derive(Clone, Copy, Debug)]
enum InputDeviceType {
    Keyboard,
    Mouse,
    Tablet,
}

impl VirtioInputDevice {
    /// Find and initialize VirtIO input devices
    pub fn find_virtio_input_devices() -> Vec<Self> {
        uart_write_string("Scanning PCI bus for VirtIO input devices...\r\n");
        let mut devices = Vec::new();

        let config = crate::kernel::pci::PciConfig::new();

        // Scan PCI bus for VirtIO input devices
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    if let Some(pci_dev) = PciDevice::new(bus, device, function, &config) {
                        if pci_dev.vendor_id == VIRTIO_VENDOR_ID &&
                           (pci_dev.device_id == VIRTIO_INPUT_DEVICE_ID || pci_dev.device_id == VIRTIO_INPUT_DEVICE_ID_LEGACY) {
                            uart_write_string("Found VirtIO input device at ");
                            print_hex(bus as u64);
                            uart_write_string(":");
                            print_hex(device as u64);
                            uart_write_string(":");
                            print_hex(function as u64);
                            uart_write_string("\r\n");

                            // Allocate BAR4 if it's not set up
                            // Use a high MMIO address range for VirtIO devices
                            // Start at 0x10000000 + (device number * 0x10000)
                            let mmio_base = 0x10000000u64 + ((device as u64) * 0x10000);
                            uart_write_string("Allocating BAR4 at 0x");
                            print_hex(mmio_base);
                            uart_write_string("\r\n");

                            // Write the MMIO address to BAR4
                            config.write_u32(bus, device, function, 0x20, mmio_base as u32);

                            // Enable memory access and bus mastering
                            let mut cmd = config.read_u16(bus, device, function, 0x04);
                            cmd |= 0x0006; // Memory Space Enable + Bus Master Enable
                            config.write_u16(bus, device, function, 0x04, cmd);

                            // Parse VirtIO capabilities to find device registers
                            uart_write_string("Parsing VirtIO capabilities...\r\n");
                            if let Some(cap_ptr) = pci_dev.read_capability_pointer() {
                                if let Some((base_addr, notify_addr, device_config_addr, notify_off_multiplier)) = Self::parse_virtio_caps(&pci_dev, cap_ptr, mmio_base) {
                                    uart_write_string("Found VirtIO config at: 0x");
                                    print_hex(base_addr);
                                    uart_write_string("\r\n");
                                    uart_write_string("Found VirtIO notify at: 0x");
                                    print_hex(notify_addr);
                                    uart_write_string("\r\n");

                                    // Enable memory access and bus mastering on the PCI device
                                    pci_dev.enable_memory_access();
                                    pci_dev.enable_bus_mastering();

                                    let mut virtio_input = Self {
                                        pci_device: pci_dev,
                                        base_addr,
                                        notify_addr,
                                        device_config_addr,
                                        notify_off_multiplier,
                                        queue_notify_off: 0,
                                        event_queue_addr: 0,
                                        device_type: InputDeviceType::Mouse,
                                        desc_table: core::ptr::null_mut(),
                                        avail_ring: core::ptr::null_mut(),
                                        used_ring: core::ptr::null_mut(),
                                        event_buffers: [core::ptr::null_mut(); 16],
                                        last_used_idx: 0,
                                        queue_size: 16,
                                        modifier_state: 0,
                                    };

                                    if virtio_input.init() {
                                        devices.push(virtio_input);
                                    }
                                } else {
                                    uart_write_string("Failed to parse VirtIO capabilities\r\n");
                                }
                            } else {
                                uart_write_string("No PCI capabilities found\r\n");
                            }
                        }
                    }
                }
            }
        }

        uart_write_string("Found ");
        print_hex(devices.len() as u64);
        uart_write_string(" VirtIO input device(s)\r\n");
        devices
    }

    /// Find all VirtIO input devices using DTB-provided PCI addresses
    pub fn find_virtio_input_devices_with_pci_base(pci_ecam_base: u64, pci_mmio_base: u64) -> Vec<Self> {
        uart_write_string("Scanning PCI bus with DTB addresses...\r\n");
        uart_write_string("PCI ECAM base: 0x");
        print_hex(pci_ecam_base);
        uart_write_string("\r\n");
        uart_write_string("PCI MMIO base: 0x");
        print_hex(pci_mmio_base);
        uart_write_string("\r\n");

        let mut devices = Vec::new();
        let config = crate::kernel::pci::PciConfig::with_base_addr(pci_ecam_base);

        // Scan PCI bus for VirtIO input devices
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    if let Some(pci_dev) = PciDevice::new(bus, device, function, &config) {
                        if pci_dev.vendor_id == VIRTIO_VENDOR_ID &&
                           (pci_dev.device_id == VIRTIO_INPUT_DEVICE_ID || pci_dev.device_id == VIRTIO_INPUT_DEVICE_ID_LEGACY) {
                            uart_write_string("Found VirtIO input device at ");
                            print_hex(bus as u64);
                            uart_write_string(":");
                            print_hex(device as u64);
                            uart_write_string(":");
                            print_hex(function as u64);
                            uart_write_string("\r\n");

                            // PROPERLY PROGRAM BAR4:
                            // 1. Write 0xFFFFFFFF to detect size
                            config.write_u32(bus, device, function, 0x20, 0xFFFFFFFF);
                            let size_mask = config.read_u32(bus, device, function, 0x20);
                            let bar_size = !(size_mask & 0xFFFFFFF0) + 1;
                            uart_write_string("  BAR4 size: 0x");
                            print_hex(bar_size as u64);
                            uart_write_string("\r\n");

                            // 2. Allocate address - give each device 1MB space
                            let bar4_address = pci_mmio_base + ((device as u64) * 0x100000);
                            uart_write_string("  Allocating BAR4 at: 0x");
                            print_hex(bar4_address);
                            uart_write_string("\r\n");

                            // 3. Program the BAR (it's 64-bit, so program both BAR4 and BAR5)
                            config.write_u32(bus, device, function, 0x20, bar4_address as u32); // Lower 32 bits
                            config.write_u32(bus, device, function, 0x24, (bar4_address >> 32) as u32); // Upper 32 bits

                            // 4. Read back to verify
                            let readback = config.read_u32(bus, device, function, 0x20);
                            uart_write_string("  BAR4 readback: 0x");
                            print_hex(readback as u64);
                            uart_write_string(" (flags=0x");
                            print_hex((readback & 0xF) as u64);
                            uart_write_string(")\r\n");

                            // Enable memory space and bus mastering
                            let mut cmd = config.read_u16(bus, device, function, 0x04);
                            cmd |= 0x0006; // Memory Space + Bus Master
                            config.write_u16(bus, device, function, 0x04, cmd);

                            // Get capability pointer from PCI status/command register
                            let status = config.read_u16(bus, device, function, 0x06);  // Status register
                            if (status & 0x10) != 0 {  // Capabilities list bit
                                let cap_ptr = config.read_u8(bus, device, function, 0x34);  // Cap pointer
                                uart_write_string("  Capability pointer: 0x");
                                print_hex(cap_ptr as u64);
                                uart_write_string("\r\n");

                                if let Some((base_addr, notify_addr, device_config_addr, notify_off_multiplier)) = Self::parse_virtio_caps(&pci_dev, cap_ptr, bar4_address) {
                                    uart_write_string("  Common config BAR: 0x");
                                    print_hex(base_addr);
                                    uart_write_string("\r\n");
                                    uart_write_string("  Notify BAR: 0x");
                                    print_hex(notify_addr);
                                    uart_write_string("\r\n");
                                    uart_write_string("  Device config BAR: 0x");
                                    print_hex(device_config_addr);
                                    uart_write_string("\r\n");

                                    // Enable memory access and bus mastering on the PCI device
                                    pci_dev.enable_memory_access();
                                    pci_dev.enable_bus_mastering();

                                    let mut virtio_input = Self {
                                        pci_device: pci_dev,
                                        base_addr,
                                        notify_addr,
                                        device_config_addr,
                                        notify_off_multiplier,
                                        queue_notify_off: 0,
                                        event_queue_addr: 0,
                                        device_type: InputDeviceType::Mouse,
                                        desc_table: core::ptr::null_mut(),
                                        avail_ring: core::ptr::null_mut(),
                                        used_ring: core::ptr::null_mut(),
                                        event_buffers: [core::ptr::null_mut(); 16],
                                        last_used_idx: 0,
                                        queue_size: 16,
                                        modifier_state: 0,
                                    };

                                    if virtio_input.init() {
                                        devices.push(virtio_input);
                                    }
                                } else {
                                    uart_write_string("Failed to parse VirtIO capabilities\r\n");
                                }
                            } else {
                                uart_write_string("No PCI capabilities found\r\n");
                            }
                        }
                    }
                }
            }
        }

        uart_write_string("Found ");
        print_hex(devices.len() as u64);
        uart_write_string(" VirtIO input device(s)\r\n");
        devices
    }

    /// Parse VirtIO PCI capabilities to find config regions
    /// mmio_base is the PCI MMIO base address (from DTB) to add to BAR offsets
    fn parse_virtio_caps(pci_dev: &PciDevice, mut cap_ptr: u8, mmio_base: u64) -> Option<(u64, u64, u64, u32)> {
        let mut common_cfg_addr = None;
        let mut notify_addr = None;
        let mut device_cfg_addr = None;
        let mut notify_off_multiplier = 0u32;
        let mut iteration = 0;

        while cap_ptr != 0 && iteration < 64 {
            let cap_id = pci_dev.read_config_u8(cap_ptr);
            let next_ptr = pci_dev.read_config_u8(cap_ptr + 1);

            // VirtIO capability ID is 0x09
            if cap_id == 0x09 {
                let cfg_type = pci_dev.read_config_u8(cap_ptr + 3);
                let bar = pci_dev.read_config_u8(cap_ptr + 4);
                let offset = pci_dev.read_config_u32(cap_ptr + 8);

                uart_write_string("  VirtIO cap: type=");
                print_hex(cfg_type as u64);
                uart_write_string(" bar=");
                print_hex(bar as u64);
                uart_write_string(" offset=0x");
                print_hex(offset as u64);
                uart_write_string("\r\n");

                // Get BAR base address (we programmed it as absolute address)
                if let Some(bar_base) = pci_dev.get_bar_address(bar) {
                    // BAR contains absolute address, just add capability offset
                    let config_addr = bar_base + offset as u64;

                    uart_write_string("  BAR base=0x");
                    print_hex(bar_base);
                    uart_write_string(", config_addr=0x");
                    print_hex(config_addr);
                    uart_write_string("\r\n");

                    if cfg_type == 1 { // VIRTIO_PCI_CAP_COMMON_CFG
                        uart_write_string("  Found common config at 0x");
                        print_hex(config_addr);
                        uart_write_string("\r\n");
                        common_cfg_addr = Some(config_addr);
                    } else if cfg_type == 2 { // VIRTIO_PCI_CAP_NOTIFY_CFG
                        uart_write_string("  Found notify config at 0x");
                        print_hex(config_addr);
                        uart_write_string("\r\n");
                        notify_addr = Some(config_addr);
                        // Read notify_off_multiplier from offset 16 of the capability
                        notify_off_multiplier = pci_dev.read_config_u32(cap_ptr + 16);
                        uart_write_string("  Notify multiplier: ");
                        print_hex(notify_off_multiplier as u64);
                        uart_write_string("\r\n");
                    } else if cfg_type == 4 { // VIRTIO_PCI_CAP_DEVICE_CFG
                        uart_write_string("  Found device config at 0x");
                        print_hex(config_addr);
                        uart_write_string("\r\n");
                        device_cfg_addr = Some(config_addr);
                    }
                }
            }

            cap_ptr = next_ptr;
            iteration += 1;
        }

        // All three config regions must be found
        if let (Some(common), Some(notify), Some(device)) = (common_cfg_addr, notify_addr, device_cfg_addr) {
            Some((common, notify, device, notify_off_multiplier))
        } else {
            None
        }
    }

    /// Initialize VirtIO input device
    fn init(&mut self) -> bool {
        uart_write_string("Initializing VirtIO input device...\r\n");

        // Sanity check base address
        if self.base_addr < 0x10000 {
            uart_write_string("  ERROR: Base address too low (0x");
            print_hex(self.base_addr);
            uart_write_string("), likely invalid. Skipping device.\r\n");
            return false;
        }

        uart_write_string("  Base address: 0x");
        print_hex(self.base_addr);
        uart_write_string("\r\n");

        // Enable PCI memory access
        self.pci_device.enable_memory_access();

        unsafe {
            // VirtIO Common Config structure offsets
            let device_status_offset = 0x14u64;
            let device_feature_select_offset = 0x00u64;
            let device_feature_offset = 0x04u64;
            let driver_feature_select_offset = 0x08u64;
            let driver_feature_offset = 0x0Cu64;
            let queue_select_offset = 0x16u64;
            let queue_size_offset = 0x18u64;
            let queue_desc_offset = 0x20u64;
            let queue_avail_offset = 0x28u64;
            let queue_used_offset = 0x30u64;
            let queue_enable_offset = 0x1Cu64;

            // Step 1: Reset device
            uart_write_string("  Resetting device...\r\n");
            core::ptr::write_volatile((self.base_addr + device_status_offset) as *mut u8, 0);
            for _ in 0..1000 { core::arch::asm!("nop"); }

            // Test read-back
            let readback = core::ptr::read_volatile((self.base_addr + device_status_offset) as *const u8);
            uart_write_string("  After reset, status reads: 0x");
            print_hex(readback as u64);

            // Try reading device_feature_select (offset 0x00) - should work
            let feat_sel = core::ptr::read_volatile((self.base_addr + 0x00) as *const u32);
            uart_write_string(" feat_sel=0x");
            print_hex(feat_sel as u64);
            uart_write_string("\r\n");

            // Step 2: Set ACKNOWLEDGE bit (1)
            core::ptr::write_volatile((self.base_addr + device_status_offset) as *mut u8, 1);

            // Step 3: Set DRIVER bit (2)
            let mut status = 1 | 2;
            core::ptr::write_volatile((self.base_addr + device_status_offset) as *mut u8, status);

            // Step 4: Feature negotiation
            uart_write_string("  Negotiating features...\r\n");
            // Read device features (bits 0-31)
            core::ptr::write_volatile((self.base_addr + device_feature_select_offset) as *mut u32, 0);
            let _device_features_low = core::ptr::read_volatile((self.base_addr + device_feature_offset) as *const u32);
            // Read device features (bits 32-63)
            core::ptr::write_volatile((self.base_addr + device_feature_select_offset) as *mut u32, 1);
            let _device_features_high = core::ptr::read_volatile((self.base_addr + device_feature_offset) as *const u32);

            // Write driver features - accept no optional features (just negotiate version 1)
            // Feature bit 32 (VIRTIO_F_VERSION_1) is required for modern VirtIO
            core::ptr::write_volatile((self.base_addr + driver_feature_select_offset) as *mut u32, 0);
            core::ptr::write_volatile((self.base_addr + driver_feature_offset) as *mut u32, 0); // No features in low 32 bits
            core::ptr::write_volatile((self.base_addr + driver_feature_select_offset) as *mut u32, 1);
            core::ptr::write_volatile((self.base_addr + driver_feature_offset) as *mut u32, 1); // Bit 32 = VIRTIO_F_VERSION_1

            // Step 5: Set FEATURES_OK bit (8)
            uart_write_string("  Setting FEATURES_OK...\r\n");
            status |= 8;
            core::ptr::write_volatile((self.base_addr + device_status_offset) as *mut u8, status);

            // Step 6: Re-read status to verify FEATURES_OK stuck
            let status_readback = core::ptr::read_volatile((self.base_addr + device_status_offset) as *const u8);
            if (status_readback & 8) == 0 {
                uart_write_string("  ERROR: Device rejected features!\r\n");
                return false;
            }
            uart_write_string("  Features accepted!\r\n");

            // Step 7: Allocate virtqueue structures
            uart_write_string("  Allocating virtqueue memory...\r\n");

            // Allocate aligned memory for queue structures
            let desc_table_size = core::mem::size_of::<VirtqDesc>() * 16;
            let avail_size = core::mem::size_of::<VirtqAvail>();
            let used_size = core::mem::size_of::<VirtqUsed>();

            // Use high memory for virtqueue structures in RAM (RAM starts at 0x40000000 on ARM virt)
            // Use 0x50000000 to be well clear of kernel code
            static mut VIRTQ_ALLOC_OFFSET: u64 = 0;
            let base_mem = 0x50000000u64 + VIRTQ_ALLOC_OFFSET;
            VIRTQ_ALLOC_OFFSET += 0x10000; // 64KB per device

            self.desc_table = base_mem as *mut VirtqDesc;
            self.avail_ring = (base_mem + 0x1000) as *mut VirtqAvail;
            self.used_ring = (base_mem + 0x2000) as *mut VirtqUsed;

            uart_write_string("  Desc table: 0x");
            print_hex(self.desc_table as u64);
            uart_write_string("\r\n");

            // Zero out all virtqueue memory first to ensure clean state
            uart_write_string("  Zeroing virtqueue memory...\r\n");
            core::ptr::write_bytes(self.desc_table as *mut u8, 0, core::mem::size_of::<VirtqDesc>() * 16);
            core::ptr::write_bytes(self.avail_ring as *mut u8, 0, core::mem::size_of::<VirtqAvail>());
            core::ptr::write_bytes(self.used_ring as *mut u8, 0, core::mem::size_of::<VirtqUsed>());

            // Allocate event buffers
            let event_buf_base = base_mem + 0x4000;
            for i in 0..16 {
                self.event_buffers[i] = (event_buf_base + (i as u64 * core::mem::size_of::<VirtioInputEvent>() as u64)) as *mut VirtioInputEvent;
                // Zero out event buffer memory too
                core::ptr::write_bytes(self.event_buffers[i] as *mut u8, 0, core::mem::size_of::<VirtioInputEvent>());
            }

            // Initialize descriptor table
            uart_write_string("  Setting up descriptors...\r\n");
            for i in 0..16 {
                (*self.desc_table.add(i)).addr = self.event_buffers[i] as u64;
                (*self.desc_table.add(i)).len = core::mem::size_of::<VirtioInputEvent>() as u32;
                (*self.desc_table.add(i)).flags = 2; // VIRTQ_DESC_F_WRITE
                (*self.desc_table.add(i)).next = 0;
            }

            // Initialize available ring
            (*self.avail_ring).flags = 0;
            (*self.avail_ring).idx = 0;

            // Initialize used ring
            (*self.used_ring).flags = 0;
            (*self.used_ring).idx = 0;

            // Step 8: Configure virtqueue 0
            uart_write_string("  Configuring virtqueue...\r\n");
            core::ptr::write_volatile((self.base_addr + queue_select_offset) as *mut u16, 0);

            // Read queue_notify_off for queue 0 IMMEDIATELY after selecting queue (offset 0x1E in common config)
            let queue_notify_off_offset = 0x1Eu64;
            self.queue_notify_off = core::ptr::read_volatile((self.base_addr + queue_notify_off_offset) as *const u16);
            uart_write_string("  Queue 0 notify_off: ");
            print_hex(self.queue_notify_off as u64);
            uart_write_string("\r\n");

            // Set queue size to 16
            core::ptr::write_volatile((self.base_addr + queue_size_offset) as *mut u16, 16);

            // Set queue addresses
            core::ptr::write_volatile((self.base_addr + queue_desc_offset) as *mut u64, self.desc_table as u64);
            core::ptr::write_volatile((self.base_addr + queue_avail_offset) as *mut u64, self.avail_ring as u64);
            core::ptr::write_volatile((self.base_addr + queue_used_offset) as *mut u64, self.used_ring as u64);

            // Make all buffers available to device
            uart_write_string("  Making buffers available...\r\n");
            for i in 0..16u16 {
                (*self.avail_ring).ring[i as usize] = i;
            }
            // Memory barrier to ensure all ring updates are visible before idx update
            core::arch::asm!("dmb ishst");
            (*self.avail_ring).idx = 16;
            // Memory barrier to ensure idx is visible before queue enable
            core::arch::asm!("dmb ishst");

            // Enable queue 0
            core::ptr::write_volatile((self.base_addr + queue_enable_offset) as *mut u16, 1);

            // Step 9: Configure virtqueue 1 (statusq) - required by VirtIO Input spec
            uart_write_string("  Configuring statusq (queue 1)...\r\n");
            core::ptr::write_volatile((self.base_addr + queue_select_offset) as *mut u16, 1);

            // Set queue size to 16
            core::ptr::write_volatile((self.base_addr + queue_size_offset) as *mut u16, 16);

            // Allocate separate structures for queue 1
            let status_desc = (base_mem + 0x5000) as *mut VirtqDesc;
            let status_avail = (base_mem + 0x6000) as *mut VirtqAvail;
            let status_used = (base_mem + 0x7000) as *mut VirtqUsed;

            // Zero them out
            core::ptr::write_bytes(status_desc as *mut u8, 0, core::mem::size_of::<VirtqDesc>() * 16);
            core::ptr::write_bytes(status_avail as *mut u8, 0, core::mem::size_of::<VirtqAvail>());
            core::ptr::write_bytes(status_used as *mut u8, 0, core::mem::size_of::<VirtqUsed>());

            // Set queue addresses for queue 1
            core::ptr::write_volatile((self.base_addr + queue_desc_offset) as *mut u64, status_desc as u64);
            core::ptr::write_volatile((self.base_addr + queue_avail_offset) as *mut u64, status_avail as u64);
            core::ptr::write_volatile((self.base_addr + queue_used_offset) as *mut u64, status_used as u64);

            // Enable queue 1
            core::ptr::write_volatile((self.base_addr + queue_enable_offset) as *mut u16, 1);

            // Step 10: Set DRIVER_OK bit (4)
            uart_write_string("  Setting DRIVER_OK...\r\n");
            status |= 4;
            core::ptr::write_volatile((self.base_addr + device_status_offset) as *mut u8, status);

            // Step 11: Notify device that buffers are available
            uart_write_string("  Notifying device...\r\n");
            self.notify_device();

            // Verify device status - read multiple offsets to debug
            uart_write_string("  Reading device registers:\r\n");
            for offset in [0x14u64, 0x15, 0x16, 0x17, 0x18].iter() {
                let val = core::ptr::read_volatile((self.base_addr + offset) as *const u8);
                uart_write_string("    offset 0x");
                print_hex(*offset);
                uart_write_string(": 0x");
                print_hex(val as u64);
                uart_write_string("\r\n");
            }
            let final_status = core::ptr::read_volatile((self.base_addr + device_status_offset) as *const u8);
            uart_write_string("  Device status (should be 0xF): 0x");
            print_hex(final_status as u64);
            uart_write_string("\r\n");

            uart_write_string("  Device ready!\r\n");
        }

        uart_write_string("VirtIO input device initialized!\r\n");
        true
    }

    /// Poll for input events from the device by checking the used ring
    pub fn poll_events(&mut self) -> Option<InputEvent> {
        unsafe {
            // Memory barrier to ensure we see device writes
            core::arch::asm!("dmb ishld");
            // Check if there are any used buffers in the used ring
            let used_idx = core::ptr::read_volatile(&(*self.used_ring).idx);

            if self.last_used_idx == used_idx {
                // No new events
                return None;
            }

            // Get the next used element
            let used_elem_idx = (self.last_used_idx % 16) as usize;
            let used_elem = core::ptr::read_volatile(&(*self.used_ring).ring[used_elem_idx]);

            // Read the event from the buffer
            let desc_idx = used_elem.id as usize;
            if desc_idx >= 16 {
                return None;
            }

            let event_buf = self.event_buffers[desc_idx];
            let event = core::ptr::read_volatile(event_buf);

            // Increment our used index
            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            // Make the buffer available again
            let avail_idx = core::ptr::read_volatile(&(*self.avail_ring).idx);
            let ring_idx = (avail_idx % 16) as usize;
            (*self.avail_ring).ring[ring_idx] = desc_idx as u16;
            // Memory barrier before updating idx
            core::arch::asm!("dmb ishst");
            core::ptr::write_volatile(&mut (*self.avail_ring).idx, avail_idx.wrapping_add(1));

            // Notify device about the new buffer
            self.notify_device();

            // Convert VirtIO input event to our InputEvent
            self.convert_virtio_event(&event)
        }
    }

    /// Convert VirtIO input event to our InputEvent format
    fn convert_virtio_event(&mut self, event: &VirtioInputEvent) -> Option<InputEvent> {
        match event.event_type {
            EV_REL => {
                // Relative movement (mouse)
                match event.code {
                    REL_X | REL_Y => {
                        let x_delta = if event.code == REL_X { event.value as i8 } else { 0 };
                        let y_delta = if event.code == REL_Y { event.value as i8 } else { 0 };
                        Some(InputEvent::MouseMove { x_delta, y_delta })
                    }
                    REL_WHEEL => {
                        Some(InputEvent::MouseWheel { delta: event.value as i8 })
                    }
                    _ => None,
                }
            }
            EV_ABS => {
                // Absolute position (tablet/touchpad)
                // Convert absolute to relative for now
                match event.code {
                    ABS_X => Some(InputEvent::MouseMove { x_delta: (event.value / 100) as i8, y_delta: 0 }),
                    ABS_Y => Some(InputEvent::MouseMove { x_delta: 0, y_delta: (event.value / 100) as i8 }),
                    _ => None,
                }
            }
            EV_KEY => {
                // Track modifier key state
                match event.code {
                    KEY_LEFT_CTRL => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 0; // MOD_LEFT_CTRL
                        } else {
                            self.modifier_state &= !(1 << 0);
                        }
                    }
                    KEY_RIGHT_CTRL => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 4; // MOD_RIGHT_CTRL
                        } else {
                            self.modifier_state &= !(1 << 4);
                        }
                    }
                    KEY_LEFT_SHIFT => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 1; // MOD_LEFT_SHIFT
                        } else {
                            self.modifier_state &= !(1 << 1);
                        }
                    }
                    KEY_RIGHT_SHIFT => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 5; // MOD_RIGHT_SHIFT
                        } else {
                            self.modifier_state &= !(1 << 5);
                        }
                    }
                    KEY_LEFT_ALT => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 2; // MOD_LEFT_ALT
                        } else {
                            self.modifier_state &= !(1 << 2);
                        }
                    }
                    KEY_RIGHT_ALT => {
                        if event.value != 0 {
                            self.modifier_state |= 1 << 6; // MOD_RIGHT_ALT
                        } else {
                            self.modifier_state &= !(1 << 6);
                        }
                    }
                    _ => {}
                }

                // Key or button press
                if event.code >= BTN_LEFT && event.code <= BTN_MIDDLE {
                    let button = (event.code - BTN_LEFT) as u8;
                    let pressed = event.value != 0;
                    Some(InputEvent::MouseButton { button, pressed })
                } else {
                    // Keyboard key
                    if event.value != 0 {
                        Some(InputEvent::KeyPressed { key: event.code as u8, modifiers: self.modifier_state })
                    } else {
                        Some(InputEvent::KeyReleased { key: event.code as u8, modifiers: self.modifier_state })
                    }
                }
            }
            _ => None,
        }
    }

    /// Notify the device that there are new buffers in the available ring
    fn notify_device(&self) {
        unsafe {
            // Memory barrier to ensure all writes are visible before notify
            core::arch::asm!("dmb ishst");
            // Calculate the correct notify address for queue 0
            // If queue_notify_off is 0xFFFF, use queue number (0) directly
            // Otherwise use: notify_addr + (queue_notify_off * notify_off_multiplier)
            let queue_notify_addr = if self.queue_notify_off == 0xFFFF {
                // QEMU VirtIO Input returns 0xFFFF - use simple queue-based addressing
                self.notify_addr + (0u64 * self.notify_off_multiplier as u64)
            } else {
                self.notify_addr + (self.queue_notify_off as u64 * self.notify_off_multiplier as u64)
            };
            // Write the queue number (0) to the calculated notify address
            core::ptr::write_volatile(queue_notify_addr as *mut u16, 0);
        }
    }

    /// Query device name via device config space
    fn query_device_name(&self) {
        unsafe {
            uart_write_string("  Querying device name...\r\n");

            // Device config offsets:
            // select: 0, subsel: 1, size: 2, reserved: 3-7, data: 8+

            // Write select = VIRTIO_INPUT_CFG_ID_NAME
            core::ptr::write_volatile((self.device_config_addr + 0) as *mut u8, VIRTIO_INPUT_CFG_ID_NAME);
            // Write subsel = 0
            core::ptr::write_volatile((self.device_config_addr + 1) as *mut u8, 0);

            // Full memory barrier to ensure writes are visible before reads
            core::arch::asm!("dmb ish");

            // Small delay for device to process
            for _ in 0..10000 { core::arch::asm!("nop"); }

            // Read size with memory barrier
            core::arch::asm!("dmb ishld");
            let size = core::ptr::read_volatile((self.device_config_addr + 2) as *const u8);
            uart_write_string("  Device name size: 0x");
            print_hex(size as u64);
            uart_write_string("\r\n");

            // 0xFF typically means not supported/no data
            if size > 0 && size < 128 {
                uart_write_string("  Device name: ");
                // Read name from data field (offset 8)
                for i in 0..size {
                    let c = core::ptr::read_volatile((self.device_config_addr + 8 + i as u64) as *const u8);
                    if c >= 32 && c < 127 {  // Printable ASCII
                        core::ptr::write_volatile(0x09000000 as *mut u8, c);
                    }
                }
                uart_write_string("\r\n");
            } else if size == 0xFF {
                uart_write_string("  Device config returned 0xFF - may not be supported by QEMU\r\n");
            }
        }
    }

    /// Query supported event types via device config space
    fn query_event_types(&self) {
        unsafe {
            uart_write_string("  Querying supported event types...\r\n");

            // Query each event type we care about
            for ev_type in [EV_KEY, EV_REL, EV_ABS].iter() {
                // Write select = VIRTIO_INPUT_CFG_EV_BITS
                core::ptr::write_volatile((self.device_config_addr + 0) as *mut u8, VIRTIO_INPUT_CFG_EV_BITS);
                // Write subsel = event type
                core::ptr::write_volatile((self.device_config_addr + 1) as *mut u8, *ev_type as u8);

                // Memory barrier
                core::arch::asm!("dmb ishst");

                // Read size
                let size = core::ptr::read_volatile((self.device_config_addr + 2) as *const u8);

                if size > 0 {
                    uart_write_string("  Event type ");
                    print_hex(*ev_type as u64);
                    uart_write_string(" supported (bitmap size: ");
                    print_hex(size as u64);
                    uart_write_string(" bytes)\r\n");

                    // Read first few bytes of bitmap to see what's supported
                    if size >= 4 {
                        let bitmap = core::ptr::read_volatile((self.device_config_addr + 8) as *const u32);
                        uart_write_string("  Bitmap: 0x");
                        print_hex(bitmap as u64);
                        uart_write_string("\r\n");
                    }
                }
            }
        }
    }
}

fn print_hex(n: u64) {
    let hex_chars = b"0123456789ABCDEF";
    if n == 0 {
        uart_write_string("0");
        return;
    }

    let mut buffer = [0u8; 16];
    let mut i = 0;
    let mut num = n;

    while num > 0 && i < 16 {
        buffer[i] = hex_chars[(num % 16) as usize];
        num /= 16;
        i += 1;
    }

    // Print in reverse order
    for j in 0..i {
        unsafe {
            core::ptr::write_volatile(0x09000000 as *mut u8, buffer[i - 1 - j]);
        }
    }
}

// Global VirtIO input devices
static mut VIRTIO_INPUT_DEVICES: Option<Vec<VirtioInputDevice>> = None;

/// Initialize VirtIO input subsystem
pub fn init_virtio_input() {
    uart_write_string("Initializing VirtIO input subsystem...\r\n");

    let devices = VirtioInputDevice::find_virtio_input_devices();

    unsafe {
        VIRTIO_INPUT_DEVICES = Some(devices);
    }

    uart_write_string("VirtIO input subsystem ready!\r\n");
}

/// Initialize VirtIO input using DTB-provided PCI ECAM base address
pub fn init_virtio_input_with_pci_base(pci_ecam_base: u64, pci_mmio_base: u64) {
    uart_write_string("Initializing VirtIO input subsystem with DTB PCI addresses...\r\n");

    let devices = VirtioInputDevice::find_virtio_input_devices_with_pci_base(pci_ecam_base, pci_mmio_base);

    unsafe {
        VIRTIO_INPUT_DEVICES = Some(devices);
    }

    uart_write_string("VirtIO input subsystem ready!\r\n");
}

/// Poll all VirtIO input devices for events
pub fn poll_virtio_input() {
    unsafe {
        if let Some(ref mut devices) = VIRTIO_INPUT_DEVICES {
            for device in devices.iter_mut() {
                if let Some(event) = device.poll_events() {
                    // Handle mouse movement for hardware cursor
                    if let InputEvent::MouseMove { x_delta, y_delta } = event {
                        crate::kernel::handle_mouse_movement(x_delta as i32, y_delta as i32);
                    }

                    queue_input_event(event);
                }
            }
        }
    }
}
