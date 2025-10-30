// PCI bus scanning and device management
#![allow(dead_code)]

extern crate alloc;

// PCI configuration space registers
const PCI_VENDOR_ID: u8 = 0x00;
const PCI_DEVICE_ID: u8 = 0x02;
const PCI_COMMAND: u8 = 0x04;
const PCI_STATUS: u8 = 0x06;
const PCI_CLASS_CODE: u8 = 0x08;
const PCI_HEADER_TYPE: u8 = 0x0E;
const PCI_BAR0: u8 = 0x10;

// PCI command register bits
const PCI_COMMAND_IO: u16 = 1 << 0;
const PCI_COMMAND_MEMORY: u16 = 1 << 1;
const PCI_COMMAND_MASTER: u16 = 1 << 2;

// ARM64 QEMU virt machine PCI base addresses
const PCI_CONFIG_BASE: u64 = 0x4010000000;
const PCI_MMIO_BASE: u64 = 0x10000000;
const PCI_MMIO_SIZE: u64 = 0x2eff0000;

pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u32,
    pub bar0: u32,
    pub device_info: PciDeviceInfo,
}

#[derive(Clone, Debug)]
pub struct PciDeviceInfo {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

pub struct PciConfig {
    base_addr: u64,
}

impl PciConfig {
    pub fn new() -> Self {
        PciConfig {
            base_addr: PCI_CONFIG_BASE,
        }
    }

    fn config_address(&self, bus: u8, device: u8, function: u8, offset: u8) -> u64 {
        self.base_addr + 
        ((bus as u64) << 20) + 
        ((device as u64) << 15) + 
        ((function as u64) << 12) + 
        (offset as u64)
    }

    pub fn read_u16(&self, bus: u8, device: u8, function: u8, offset: u8) -> u16 {
        let addr = self.config_address(bus, device, function, offset);
        unsafe { core::ptr::read_volatile(addr as *const u16) }
    }

    pub fn read_u32(&self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let addr = self.config_address(bus, device, function, offset);
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    pub fn write_u16(&self, bus: u8, device: u8, function: u8, offset: u8, value: u16) {
        let addr = self.config_address(bus, device, function, offset);
        unsafe { core::ptr::write_volatile(addr as *mut u16, value) }
    }

    pub fn write_u32(&self, bus: u8, device: u8, function: u8, offset: u8, value: u32) {
        let addr = self.config_address(bus, device, function, offset);
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }
}

impl PciDevice {
    pub fn new(bus: u8, device: u8, function: u8, config: &PciConfig) -> Option<Self> {
        let vendor_id = config.read_u16(bus, device, function, PCI_VENDOR_ID);
        
        // Check if device exists
        if vendor_id == 0xFFFF {
            return None;
        }

        let device_id = config.read_u16(bus, device, function, PCI_DEVICE_ID);
        let class_code = config.read_u32(bus, device, function, PCI_CLASS_CODE);
        let bar0 = config.read_u32(bus, device, function, PCI_BAR0);

        let device_info = PciDeviceInfo {
            bus,
            device,
            function,
            vendor_id,
            device_id,
            class_code: (class_code >> 24) as u8,
            subclass: ((class_code >> 16) & 0xFF) as u8,
            prog_if: ((class_code >> 8) & 0xFF) as u8,
        };

        Some(PciDevice {
            bus,
            device,
            function,
            vendor_id,
            device_id,
            class_code,
            bar0,
            device_info,
        })
    }

    pub fn enable_memory_access(&self) {
        let config = PciConfig::new();
        let mut command = config.read_u16(self.bus, self.device, self.function, PCI_COMMAND);
        command |= PCI_COMMAND_MEMORY;
        config.write_u16(self.bus, self.device, self.function, PCI_COMMAND, command);
    }

    pub fn enable_bus_mastering(&self) {
        let config = PciConfig::new();
        let mut command = config.read_u16(self.bus, self.device, self.function, PCI_COMMAND);
        command |= PCI_COMMAND_MASTER;
        config.write_u16(self.bus, self.device, self.function, PCI_COMMAND, command);
    }

    pub fn get_bar_address(&self, bar_index: u8) -> Option<u64> {
        if bar_index >= 6 {
            return None;
        }

        let config = PciConfig::new();
        let bar_offset = PCI_BAR0 + (bar_index * 4);
        let bar_value = config.read_u32(self.bus, self.device, self.function, bar_offset);

        // Check if BAR is unimplemented (all 0s) or invalid (all 1s)
        if bar_value == 0 || bar_value == 0xFFFFFFFF {
            return None;
        }

        // Check if it's a memory BAR (bit 0 = 0) or I/O BAR (bit 0 = 1)
        if (bar_value & 1) == 0 {
            // Memory BAR - mask off the lower 4 bits (flags)
            let address = (bar_value & 0xFFFFFFF0) as u64;
            
            // DEBUG output
            unsafe {
                let uart_base = 0x09000000 as *mut u8;
                let debug_msg = b"DEBUG: Memory BAR masked=0x";
                for &byte in debug_msg {
                    core::ptr::write_volatile(uart_base, byte);
                }
                let mut addr = address;
                for _ in 0..8 {
                    let digit = (addr >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    core::ptr::write_volatile(uart_base, ch);
                    addr <<= 4;
                }
                core::ptr::write_volatile(uart_base, b'\r');
                core::ptr::write_volatile(uart_base, b'\n');
            }
            
            // For VirtIO devices, be more permissive with address validation
            // VirtIO devices might use smaller addresses that are still valid
            if address > 0 {
                Some(address)
            } else {
                None
            }
        } else {
            // I/O BAR - mask off the lower 2 bits (flags)
            let address = (bar_value & 0xFFFFFFFC) as u64;
            
            unsafe {
                let uart_base = 0x09000000 as *mut u8;
                let debug_msg = b"DEBUG: I/O BAR address=0x";
                for &byte in debug_msg {
                    core::ptr::write_volatile(uart_base, byte);
                }
                let mut addr = address;
                for _ in 0..8 {
                    let digit = (addr >> 28) & 0xF;
                    let ch = if digit < 10 { b'0' + digit as u8 } else { b'A' + (digit - 10) as u8 };
                    core::ptr::write_volatile(uart_base, ch);
                    addr <<= 4;
                }
                core::ptr::write_volatile(uart_base, b'\r');
                core::ptr::write_volatile(uart_base, b'\n');
            }
            
            // I/O BARs should have valid I/O port addresses (typically < 0x10000)
            if address > 0 && address < 0x10000 {
                Some(address)
            } else {
                None
            }
        }
    }
    
    pub fn read_capability_pointer(&self) -> Option<u8> {
        let config = PciConfig::new();
        let status = config.read_u16(self.bus, self.device, self.function, PCI_STATUS);
        
        // Check if capabilities list is present (bit 4)
        if (status & (1 << 4)) != 0 {
            // Capabilities pointer is at offset 0x34
            let cap_ptr = config.read_u32(self.bus, self.device, self.function, 0x34) as u8;
            if cap_ptr != 0 {
                Some(cap_ptr)
            } else {
                None
            }
        } else {
            None
        }
    }
    
    pub fn read_config_u8(&self, offset: u8) -> u8 {
        let config = PciConfig::new();
        let addr = config.config_address(self.bus, self.device, self.function, offset);
        unsafe { core::ptr::read_volatile(addr as *const u8) }
    }
    
    pub fn read_config_u16(&self, offset: u8) -> u16 {
        let config = PciConfig::new();
        config.read_u16(self.bus, self.device, self.function, offset)
    }
    
    pub fn read_config_u32(&self, offset: u8) -> u32 {
        let config = PciConfig::new();
        config.read_u32(self.bus, self.device, self.function, offset)
    }
}

pub fn scan_pci_bus() -> alloc::vec::Vec<PciDevice> {
    let mut devices = alloc::vec::Vec::new();
    let config = PciConfig::new();

    // Scan PCI bus 0, devices 0-31, function 0
    for device in 0..32 {
        if let Some(pci_device) = PciDevice::new(0, device, 0, &config) {
            devices.push(pci_device);
        }
    }

    devices
}

pub fn find_device(vendor_id: u16, device_id: u16) -> Option<PciDevice> {
    let devices = scan_pci_bus();
    
    for device in devices {
        if device.vendor_id == vendor_id && device.device_id == device_id {
            return Some(device);
        }
    }
    
    None
}

pub fn print_pci_devices() {
    let devices = scan_pci_bus();
    
    // For now, just count devices since we can't easily print after UEFI exit
    let _device_count = devices.len();
}

/// Enumerate all PCI devices and return with device info
pub fn enumerate_pci_devices() -> alloc::vec::Vec<PciDevice> {
    scan_pci_bus()
}

impl PciDevice {
    /// Enable bus mastering for the device
    pub fn enable_bus_master(&self) {
        self.enable_bus_mastering();
    }
}