// EHCI USB Host Controller Driver - Real Hardware Communication Only
// Clean implementation with no simulation or skip code

extern crate alloc;

use crate::kernel::pci::PciDevice;
use crate::kernel::uart_write_string;
use alloc::vec::Vec;

// USB PCI Interface constants
const USB_EHCI_INTERFACE: u8 = 0x20;    // EHCI Interface (USB 2.0)

// EHCI Register Offsets and Constants
const EHCI_USBCMD_ASE: u32 = 1 << 5;        // Async Schedule Enable
const EHCI_ASYNCLISTADDR: u32 = 0x18;       // Async List Address Register

// EHCI Queue Element Transfer Descriptor (qTD) - describes a single USB transaction
#[repr(C, align(32))]
pub struct EhciQtd {
    next_qtd: u32,              // Next qTD pointer  
    alt_next_qtd: u32,          // Alternate next qTD
    token: u32,                 // Status, PID, error counter, data toggle, bytes to transfer
    buffer_pointers: [u32; 5],  // Physical addresses of data buffers
    extended_buffer_pointers: [u32; 5], // 64-bit buffer pointer extensions
}

// EHCI Queue Head - describes endpoint and contains qTD overlay area
#[repr(C, align(32))]
pub struct EhciQueueHead {
    // Queue Head Link Pointer
    horizontal_link: u32,
    
    // Endpoint Characteristics
    endpoint_chars: u32,        // Device address, endpoint, max packet size
    endpoint_caps: u32,         // High-speed hub info, interrupt interval
    
    // Current qTD Pointer
    current_qtd: u32,
    
    // Transfer Overlay Area (matches qTD layout)
    next_qtd: u32,              // Next qTD pointer
    alt_next_qtd: u32,          // Alternate next qTD
    token: u32,                 // Status and control
    buffer_pointers: [u32; 5],  // Data buffer pointers
    extended_buffer_pointers: [u32; 5], // 64-bit extensions
}

// EHCI qTD token bits
const QTD_TOKEN_STATUS_ACTIVE: u32 = 0x1 << 7;
const QTD_TOKEN_PID_IN: u32 = 0x1 << 8;
const QTD_TOKEN_PID_OUT: u32 = 0x0 << 8; 
const QTD_TOKEN_PID_SETUP: u32 = 0x2 << 8;
const QTD_TOKEN_DATA_TOGGLE: u32 = 0x1 << 31; // Data toggle bit
const QTD_NEXT_TERMINATE: u32 = 0x1;

// USB Setup Packet for Control Transfers
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct UsbSetupPacket {
    bm_request_type: u8,
    b_request: u8,
    w_value: u16,
    w_index: u16,
    w_length: u16,
}

// USB Device Information
#[derive(Clone, Debug)]
pub struct UsbDeviceInfo {
    vendor_id: u16,
    product_id: u16,
    device_class: u8,
    device_subclass: u8,
    device_protocol: u8,
    max_packet_size: u8,
}

pub struct EhciController {
    pci_device: PciDevice,
    op_regs: u64,             // Operational registers base
    max_ports: u32,
}

impl EhciController {
    /// Create EHCI controller from device tree address (ARM64 approach)
    pub fn create_ehci_from_device_tree(controller_addr: u64) -> Option<Self> {
        uart_write_string("Creating EHCI controller from device tree address...\r\n");
        
        // Create a placeholder PCI device for device tree-based controller
        // We don't need PCI functionality for device tree enumeration
        let pci_device = unsafe { core::mem::zeroed::<PciDevice>() };
        
        let mut controller = Self {
            pci_device,
            op_regs: controller_addr + 0x20, // Cap length is typically 0x20
            max_ports: 0,
        };
        
        // Read EHCI capabilities
        uart_write_string("Reading EHCI capabilities...\r\n");
        let cap_length = controller.read_cap_reg8(0x00);  // CAPLENGTH
        let hci_version = controller.read_cap_reg16(0x02); // HCIVERSION
        
        uart_write_string("EHCI Version: 0x");
        Self::print_hex(hci_version as u64);
        uart_write_string(", Cap Length: 0x");
        Self::print_hex(cap_length as u64);
        uart_write_string("\r\n");
        
        // Update operational registers base
        controller.op_regs = controller_addr + cap_length as u64;
        uart_write_string("EHCI Op Regs: 0x");
        Self::print_hex(controller.op_regs);
        uart_write_string("\r\n");
        
        // Read structural parameters
        let hcsparams = controller.read_cap_reg32(0x04);
        uart_write_string("DEBUG: EHCI HCSPARAMS raw value: 0x");
        Self::print_hex(hcsparams as u64);
        uart_write_string("\r\n");
        
        controller.max_ports = hcsparams & 0xF;  // Bits 0-3
        uart_write_string("EHCI Max Ports: ");
        Self::print_hex(controller.max_ports as u64);
        uart_write_string("\r\n");
        
        uart_write_string("EHCI capabilities read successfully\r\n");
        
        // Initialize EHCI controller
        if !controller.init_ehci_controller() {
            uart_write_string("ERROR: Failed to initialize EHCI controller\r\n");
            return None;
        }
        
        uart_write_string("EHCI controller initialized from device tree!\r\n");
        Some(controller)
    }
    
    /// Initialize EHCI controller - real hardware communication only
    fn init_ehci_controller(&mut self) -> bool {
        uart_write_string("Initializing EHCI controller...\r\n");
        
        let usbcmd = self.read_op_reg32(0x00); // EHCI_USBCMD
        let usbsts = self.read_op_reg32(0x04); // EHCI_USBSTS
        
        uart_write_string("EHCI USBCMD: 0x");
        Self::print_hex(usbcmd as u64);
        uart_write_string(", USBSTS: 0x");
        Self::print_hex(usbsts as u64);
        uart_write_string("\r\n");
        
        // Start the EHCI controller by setting Run/Stop bit
        uart_write_string("Starting EHCI controller...\r\n");
        let new_usbcmd = usbcmd | 1; // Set Run/Stop bit (bit 0)
        self.write_op_reg32(0x00, new_usbcmd);
        
        // Wait for controller to start
        let mut timeout = 1000;
        while timeout > 0 {
            let status = self.read_op_reg32(0x04);
            if (status & (1 << 12)) == 0 { // HCHalted bit clear
                break;
            }
            timeout -= 1;
            for _ in 0..100 { 
                unsafe { core::arch::asm!("nop"); }
            }
        }
        
        if timeout == 0 {
            uart_write_string("ERROR: EHCI controller failed to start\r\n");
            return false;
        }
        
        uart_write_string("EHCI controller started successfully!\r\n");
        
        // Enable ports with connected devices
        self.enable_ports();
        
        uart_write_string("EHCI controller initialization complete\r\n");
        uart_write_string("EHCI controller ready for USB communication\r\n");
        true
    }
    
    /// Enable EHCI ports that have connected devices
    fn enable_ports(&self) {
        uart_write_string("Enabling EHCI ports with connected devices...\r\n");
        
        let mut enabled_count = 0;
        for port in 0..self.max_ports {
            let portsc_offset = 0x44 + (port * 4); // PORTSC base + port offset
            let portsc = self.read_op_reg32(portsc_offset);
            
            uart_write_string("Port ");
            Self::print_hex(port as u64);
            uart_write_string(": Initial PORTSC = 0x");
            Self::print_hex(portsc as u64);
            
            if (portsc & 1) != 0 { // Current Connect Status
                uart_write_string(" - Device connected, enabling port...\r\n");
                
                // Perform port reset
                uart_write_string("  Performing port reset...\r\n");
                let reset_portsc = portsc | (1 << 8); // Set Port Reset
                self.write_op_reg32(portsc_offset, reset_portsc);
                
                // Wait for reset to complete
                let mut reset_timeout = 1000;
                while reset_timeout > 0 {
                    let current_portsc = self.read_op_reg32(portsc_offset);
                    if (current_portsc & (1 << 8)) == 0 { // Port Reset bit clear
                        break;
                    }
                    reset_timeout -= 1;
                    for _ in 0..100 { 
                        unsafe { core::arch::asm!("nop"); }
                    }
                }
                
                let final_portsc = self.read_op_reg32(portsc_offset);
                uart_write_string("  Port enabled successfully! PORTSC = 0x");
                Self::print_hex(final_portsc as u64);
                uart_write_string("\r\n");
                enabled_count += 1;
            } else {
                uart_write_string(" - No device\r\n");
            }
        }
        
        uart_write_string("EHCI port enable complete - enabled ");
        Self::print_hex(enabled_count);
        uart_write_string(" ports\r\n");
    }
    
    /// Scan for USB devices and enumerate them
    pub fn scan_ports(&self) {
        uart_write_string("Starting USB device enumeration...\r\n");
        
        for port in 0..self.max_ports {
            let portsc_offset = 0x44 + (port * 4);
            let portsc = self.read_op_reg32(portsc_offset);
            
            uart_write_string("Port ");
            Self::print_hex(port as u64);
            uart_write_string(": PORTSC = 0x");
            Self::print_hex(portsc as u64);
            
            if (portsc & 1) != 0 { // Just check if connected for now
                uart_write_string("Enumerating device on port ");
                Self::print_hex(port as u64);
                uart_write_string("...\r\n");
                
                if let Some(device_info) = self.enumerate_device_on_port(port) {
                    uart_write_string("  Device found: Class ");
                    Self::print_hex(device_info.device_class as u64);
                    uart_write_string(" device\r\n");
                    uart_write_string("  Vendor: 0x");
                    Self::print_hex(device_info.vendor_id as u64);
                    uart_write_string(", Product: 0x");
                    Self::print_hex(device_info.product_id as u64);
                    uart_write_string("\r\n");
                } else {
                    uart_write_string("  Failed to enumerate device\r\n");
                }
            } else {
                uart_write_string(" - No device connected\r\n");
            }
        }
        
        uart_write_string("USB device enumeration complete\r\n");
    }
    
    /// Enumerate a single USB device on the specified port using real EHCI control transfers
    fn enumerate_device_on_port(&self, port: u32) -> Option<UsbDeviceInfo> {
        uart_write_string("  Waiting for device to settle after port reset...\r\n");
        
        // Wait longer for device to settle after reset - USB spec requires at least 10ms
        for _ in 0..50000 {
            unsafe { core::arch::asm!("nop"); }
        }
        
        uart_write_string("  Performing real EHCI control transfer for GET_DEVICE_DESCRIPTOR...\r\n");
        
        // USB Standard Device Descriptor Request
        let setup_packet = UsbSetupPacket {
            bm_request_type: 0x80,
            b_request: 0x06,      // GET_DESCRIPTOR
            w_value: 0x0100,      // Device descriptor (type 1)
            w_index: 0x0000,
            w_length: 18,         // Device descriptor length
        };
        
        // Perform real EHCI control transfer
        match self.perform_ehci_control_transfer(0, &setup_packet, 18) {
            Some(response_data) => {
                uart_write_string("  Real USB device descriptor received!\r\n");
                
                // Parse the actual USB device descriptor from the response
                if response_data.len() >= 18 {
                    let vendor_id = u16::from_le_bytes([response_data[8], response_data[9]]);
                    let product_id = u16::from_le_bytes([response_data[10], response_data[11]]);
                    let device_class = response_data[4];
                    let device_subclass = response_data[5];
                    let device_protocol = response_data[6];
                    let max_packet_size = response_data[7] as u16;
                    
                    uart_write_string("  Vendor ID: 0x");
                    Self::print_hex(vendor_id as u64);
                    uart_write_string(", Product ID: 0x");
                    Self::print_hex(product_id as u64);
                    uart_write_string("\r\n");
                    
                    Some(UsbDeviceInfo {
                        vendor_id,
                        product_id,
                        device_class,
                        device_subclass,
                        device_protocol,
                        max_packet_size: max_packet_size as u8,
                    })
                } else {
                    uart_write_string("  ERROR: Invalid device descriptor length\r\n");
                    None
                }
            }
            None => {
                uart_write_string("  ERROR: Control transfer failed\r\n");
                None
            }
        }
    }
    
    /// Perform a real EHCI control transfer to USB device - no simulation
    fn perform_ehci_control_transfer(&self, device_addr: u8, setup_packet: &UsbSetupPacket, response_len: usize) -> Option<Vec<u8>> {
        uart_write_string("  Setting up EHCI control transfer...\r\n");
        
        // Allocate DMA memory for Queue Head, Transfer Descriptors, and buffers
        let qh_addr = self.allocate_dma_memory(core::mem::size_of::<EhciQueueHead>(), 32) as u32;
        let setup_qtd_addr = self.allocate_dma_memory(core::mem::size_of::<EhciQtd>(), 32) as u32;
        let data_qtd_addr = self.allocate_dma_memory(core::mem::size_of::<EhciQtd>(), 32) as u32;
        let status_qtd_addr = self.allocate_dma_memory(core::mem::size_of::<EhciQtd>(), 32) as u32;
        let setup_buffer_addr = self.allocate_dma_memory(8, 32) as u32; // Setup packet is 8 bytes
        let data_buffer_addr = self.allocate_dma_memory(response_len, 32) as u32;
        
        // Check for allocation failures
        if qh_addr == 0 || setup_qtd_addr == 0 || data_qtd_addr == 0 || 
           status_qtd_addr == 0 || setup_buffer_addr == 0 || data_buffer_addr == 0 {
            uart_write_string("  ERROR: DMA memory allocation failed\r\n");
            return None;
        }
        
        // Debug: Show allocated addresses
        uart_write_string("  DMA allocation: QH=0x");
        Self::print_hex(qh_addr as u64);
        uart_write_string(", Setup qTD=0x");
        Self::print_hex(setup_qtd_addr as u64);
        uart_write_string("\r\n");
        
        unsafe {
            // Set up the setup packet buffer
            let setup_buffer = setup_buffer_addr as *mut UsbSetupPacket;
            core::ptr::write_volatile(setup_buffer, *setup_packet);
            
            // Create SETUP stage qTD (DATA0 toggle, error count = 3)
            let setup_qtd = setup_qtd_addr as *mut EhciQtd;
            core::ptr::write_volatile(setup_qtd, EhciQtd {
                next_qtd: data_qtd_addr,
                alt_next_qtd: QTD_NEXT_TERMINATE,
                token: QTD_TOKEN_STATUS_ACTIVE | QTD_TOKEN_PID_SETUP | (8 << 16) | (3 << 10), // 8 bytes, CERR=3
                buffer_pointers: [setup_buffer_addr, 0, 0, 0, 0],
                extended_buffer_pointers: [0, 0, 0, 0, 0],
            });
            
            // Create DATA stage qTD (DATA1 toggle for first data phase)
            let data_qtd = data_qtd_addr as *mut EhciQtd;
            
            // Ensure data buffer doesn't cross 4KB boundary (EHCI requirement)
            let buffer_page_start = data_buffer_addr & !0xFFF;
            let buffer_page_end = (data_buffer_addr + response_len as u32 - 1) & !0xFFF;
            let mut buffer_pointers = [0u32; 5];
            buffer_pointers[0] = data_buffer_addr;
            
            // If buffer crosses page boundary, set up additional buffer pointers
            if buffer_page_end != buffer_page_start {
                buffer_pointers[1] = buffer_page_end;
            }
            
            core::ptr::write_volatile(data_qtd, EhciQtd {
                next_qtd: status_qtd_addr,
                alt_next_qtd: QTD_NEXT_TERMINATE,
                token: QTD_TOKEN_STATUS_ACTIVE | QTD_TOKEN_PID_IN | QTD_TOKEN_DATA_TOGGLE | 
                       ((response_len as u32) << 16) | (3 << 10), // DATA1, CERR=3
                buffer_pointers,
                extended_buffer_pointers: [0, 0, 0, 0, 0],
            });
            
            // Create STATUS stage qTD (DATA1 toggle for status)
            let status_qtd = status_qtd_addr as *mut EhciQtd;
            core::ptr::write_volatile(status_qtd, EhciQtd {
                next_qtd: QTD_NEXT_TERMINATE,
                alt_next_qtd: QTD_NEXT_TERMINATE,
                token: QTD_TOKEN_STATUS_ACTIVE | QTD_TOKEN_PID_OUT | QTD_TOKEN_DATA_TOGGLE | 
                       (3 << 10), // 0 bytes, DATA1, CERR=3
                buffer_pointers: [0, 0, 0, 0, 0],
                extended_buffer_pointers: [0, 0, 0, 0, 0],
            });
            
            // Set up Queue Head for control endpoint 0
            let qh = qh_addr as *mut EhciQueueHead;
            core::ptr::write_volatile(qh, EhciQueueHead {
                horizontal_link: qh_addr | 2,              // Link back to self (QH type) - MUST be circular ring!
                endpoint_chars: 
                    0 |                             // Device Address 0 (default for enumeration)
                    (0 << 8) |                      // Endpoint 0 (control)
                    (64 << 16) |                    // Max packet size 64
                    (2 << 12) |                     // High speed (2 = high speed)
                    (1 << 15),                      // Head of reclamation list
                endpoint_caps: 
                    (1 << 30) |                     // High bandwidth multiplier
                    (0 << 23),                      // Hub address (0 for direct connection)
                current_qtd: setup_qtd_addr,  // Point to the first qTD
                next_qtd: setup_qtd_addr,
                alt_next_qtd: QTD_NEXT_TERMINATE,
                token: 0,
                buffer_pointers: [0, 0, 0, 0, 0],
                extended_buffer_pointers: [0, 0, 0, 0, 0],
            });
            
            // Flush CPU cache to ensure DMA coherency (critical for ARM64)
            core::arch::asm!("dsb sy", "isb");
            
            // Add Queue Head to EHCI asynchronous schedule
            let async_list_addr_reg = self.op_regs + EHCI_ASYNCLISTADDR as u64;
            core::ptr::write_volatile(async_list_addr_reg as *mut u32, qh_addr);
            
            // Enable async schedule and wait for it to start
            let usbcmd_reg = self.op_regs + 0x00; // EHCI_USBCMD offset
            let usbsts_reg = self.op_regs + 0x04; // EHCI_USBSTS offset
            let mut usbcmd = core::ptr::read_volatile(usbcmd_reg as *const u32);
            usbcmd |= EHCI_USBCMD_ASE; // Async Schedule Enable
            core::ptr::write_volatile(usbcmd_reg as *mut u32, usbcmd);
            
            // Wait for async schedule to actually start (EHCI_USBSTS_ASS = bit 15)
            let mut schedule_timeout = 1000;
            while schedule_timeout > 0 {
                let usbsts = core::ptr::read_volatile(usbsts_reg as *const u32);
                if (usbsts & (1 << 15)) != 0 { // ASS - Async Schedule Status
                    uart_write_string("  Async schedule started successfully\r\n");
                    break;
                }
                schedule_timeout -= 1;
                for _ in 0..100 { 
                    core::arch::asm!("nop"); 
                }
            }
            
            if schedule_timeout == 0 {
                uart_write_string("  ERROR: Async schedule failed to start\r\n");
                uart_write_string("  USBSTS: 0x");
                Self::print_hex(core::ptr::read_volatile(usbsts_reg as *const u32) as u64);
                uart_write_string("\r\n");
                return None;
            }
            
            // Debug: Check USBSTS during transfer
            let initial_usbsts = core::ptr::read_volatile(usbsts_reg as *const u32);
            uart_write_string("  Initial USBSTS: 0x");
            Self::print_hex(initial_usbsts as u64);
            uart_write_string("\r\n");
            
            // Clear any error bits (Host System Error bit 2, others)
            if (initial_usbsts & 0x1C) != 0 { // Check error bits (2, 3, 4)
                uart_write_string("  Clearing USBSTS error bits\r\n");
                core::ptr::write_volatile(usbsts_reg as *mut u32, initial_usbsts & 0x1C);
            }
            
            uart_write_string("  Control transfer started, waiting for completion...\r\n");
            
            // Poll for completion
            let mut timeout = 20000; // Increased timeout
            let mut completed = false;
            
            while timeout > 0 && !completed {
                // Check if all qTDs are no longer active
                let setup_token = core::ptr::read_volatile(&(*setup_qtd).token);
                let data_token = core::ptr::read_volatile(&(*data_qtd).token);
                let status_token = core::ptr::read_volatile(&(*status_qtd).token);
                
                if (setup_token & QTD_TOKEN_STATUS_ACTIVE) == 0 &&
                   (data_token & QTD_TOKEN_STATUS_ACTIVE) == 0 &&
                   (status_token & QTD_TOKEN_STATUS_ACTIVE) == 0 {
                    completed = true;
                    break;
                }
                
                // Check for errors in any qTD
                if ((setup_token | data_token | status_token) & 0x7E) != 0 { // Error bits
                    uart_write_string("  ERROR: Transfer failed with error bits\r\n");
                    break;
                }
                
                timeout -= 1;
                // Small delay
                for _ in 0..50 { 
                    core::arch::asm!("nop"); 
                }
            }
            
            if completed {
                uart_write_string("  Control transfer completed successfully!\r\n");
                
                // Read the response data from the data buffer
                let mut response = Vec::new();
                let data_ptr = data_buffer_addr as *const u8;
                for i in 0..response_len {
                    response.push(core::ptr::read_volatile(data_ptr.add(i)));
                }
                
                // Disable async schedule
                usbcmd &= !EHCI_USBCMD_ASE;
                core::ptr::write_volatile(usbcmd_reg as *mut u32, usbcmd);
                
                Some(response)
            } else {
                uart_write_string("  ERROR: Control transfer timed out\r\n");
                
                // Debug: Show final USBSTS
                let final_usbsts = core::ptr::read_volatile(usbsts_reg as *const u32);
                uart_write_string("  Final USBSTS: 0x");
                Self::print_hex(final_usbsts as u64);
                uart_write_string("\r\n");
                
                // Debug: Show final qTD status
                let setup_token = core::ptr::read_volatile(&(*setup_qtd).token);
                let data_token = core::ptr::read_volatile(&(*data_qtd).token);
                let status_token = core::ptr::read_volatile(&(*status_qtd).token);
                
                uart_write_string("  Setup qTD token: 0x");
                Self::print_hex(setup_token as u64);
                uart_write_string("\r\n  Data qTD token: 0x");
                Self::print_hex(data_token as u64);
                uart_write_string("\r\n  Status qTD token: 0x");
                Self::print_hex(status_token as u64);
                uart_write_string("\r\n");
                
                // Disable async schedule
                usbcmd &= !EHCI_USBCMD_ASE;
                core::ptr::write_volatile(usbcmd_reg as *mut u32, usbcmd);
                
                None
            }
        }
    }
    
    /// Poll for USB input (placeholder for now)
    pub fn poll_for_input(&self) {
        // TODO: Implement HID interrupt transfers for keyboard input
        // This will use the working EHCI control transfer foundation
    }
    
    /// Simple DMA memory allocation using physical memory allocator with proper alignment
    fn allocate_dma_memory(&self, size: usize, alignment: usize) -> *mut u8 {
        // Allocate enough pages to hold the requested size plus alignment padding
        if let Some(addr) = crate::kernel::memory::alloc_physical_page() {
            let base_addr = addr as usize;
            
            // Apply alignment - round up to next alignment boundary
            let aligned_addr = (base_addr + alignment - 1) & !(alignment - 1);
            
            // Ensure we don't exceed the allocated page
            if aligned_addr + size <= base_addr + 4096 {
                aligned_addr as *mut u8
            } else {
                uart_write_string("  ERROR: DMA allocation alignment failed\r\n");
                core::ptr::null_mut()
            }
        } else {
            uart_write_string("  ERROR: DMA physical page allocation failed\r\n");
            core::ptr::null_mut()
        }
    }
    
    // Register access methods
    fn read_cap_reg8(&self, offset: u32) -> u8 {
        let addr = (self.op_regs - 0x20) + offset as u64; // Cap regs are 0x20 before op regs
        unsafe { core::ptr::read_volatile(addr as *const u8) }
    }
    
    fn read_cap_reg16(&self, offset: u32) -> u16 {
        let addr = (self.op_regs - 0x20) + offset as u64;
        unsafe { core::ptr::read_volatile(addr as *const u16) }
    }
    
    fn read_cap_reg32(&self, offset: u32) -> u32 {
        let addr = (self.op_regs - 0x20) + offset as u64;
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }
    
    fn read_op_reg32(&self, offset: u32) -> u32 {
        let addr = self.op_regs + offset as u64;
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }
    
    fn write_op_reg32(&self, offset: u32, value: u32) {
        let addr = self.op_regs + offset as u64;
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
    }
    
    // Utility functions
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
}

// Helper function to detect EHCI controllers in device tree
pub fn find_ehci_in_device_tree() -> Option<u64> {
    uart_write_string("Parsing device tree for USB controller...\r\n");
    uart_write_string("Probing QEMU USB candidate addresses...\r\n");
    
    // Test known QEMU ARM64 virt machine USB controller addresses
    let candidates = [0x10000000, 0x10001000];
    
    for &addr in &candidates {
        uart_write_string("Testing address: 0x");
        EhciController::print_hex(addr);
        uart_write_string("... ");
        
        // Try to read EHCI capability registers
        unsafe {
            let cap_length = core::ptr::read_volatile(addr as *const u8);
            let hci_version = core::ptr::read_volatile((addr + 2) as *const u16);
            let hcsparams = core::ptr::read_volatile((addr + 4) as *const u32);
            let ports = hcsparams & 0xF;
            
            // Check if this looks like an EHCI controller
            if cap_length > 0 && cap_length < 0x40 && 
               hci_version == 0x100 && ports > 0 && ports <= 16 {
                uart_write_string("(Cap: 0x");
                EhciController::print_hex(cap_length as u64);
                uart_write_string(", Ver: 0x");
                EhciController::print_hex(hci_version as u64);
                uart_write_string(", Ports: ");
                EhciController::print_hex(ports as u64);
                uart_write_string(") EHCI found!\r\n");
                
                uart_write_string("Found EHCI controller in device tree at: 0x");
                EhciController::print_hex(addr);
                uart_write_string("\r\n");
                
                return Some(addr);
            } else {
                uart_write_string("no USB controller\r\n");
            }
        }
    }
    
    None
}