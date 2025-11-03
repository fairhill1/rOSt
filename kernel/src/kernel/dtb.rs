// Device Tree Blob (DTB) Parser for ARM64
// Parses the Flattened Device Tree (FDT) passed by QEMU at 0x40000000

use crate::kernel::uart_write_string;

// FDT Magic number (big-endian 0xd00dfeed)
const FDT_MAGIC: u32 = 0xd00dfeed;

// DTB structure tokens
const FDT_BEGIN_NODE: u32 = 0x00000001;
const FDT_END_NODE: u32 = 0x00000002;
const FDT_PROP: u32 = 0x00000003;
const FDT_NOP: u32 = 0x00000004;
const FDT_END: u32 = 0x00000009;

// Standard DTB location for QEMU ARM virt machine
const DTB_BASE_ADDR: u64 = 0x40000000;

#[repr(C)]
struct FdtHeader {
    magic: u32,              // Magic number (0xd00dfeed)
    totalsize: u32,          // Total size of DTB
    off_dt_struct: u32,      // Offset to structure block
    off_dt_strings: u32,     // Offset to strings block
    off_mem_rsvmap: u32,     // Offset to memory reserve map
    version: u32,            // Version
    last_comp_version: u32,  // Last compatible version
    boot_cpuid_phys: u32,    // Boot CPU ID
    size_dt_strings: u32,    // Size of strings block
    size_dt_struct: u32,     // Size of structure block
}

/// PCI controller information extracted from DTB
#[derive(Debug, Clone, Copy)]
pub struct PciInfo {
    pub ecam_base: u64,      // PCI ECAM base address
    pub ecam_size: u64,      // PCI ECAM region size
    pub mmio_base: u64,      // MMIO base address
    pub mmio_size: u64,      // MMIO size
}

impl Default for PciInfo {
    fn default() -> Self {
        Self {
            ecam_base: 0,
            ecam_size: 0,
            mmio_base: 0,
            mmio_size: 0,
        }
    }
}

/// Read big-endian u32 from memory
unsafe fn read_be32(addr: u64) -> u32 {
    let ptr = addr as *const u32;
    u32::from_be(core::ptr::read_volatile(ptr))
}

/// Read big-endian u64 from memory (two u32s)
unsafe fn read_be64(addr: u64) -> u64 {
    let high = read_be32(addr);
    let low = read_be32(addr + 4);
    ((high as u64) << 32) | (low as u64)
}

/// Parse the DTB and extract PCI controller information
pub fn parse_dtb() -> Option<PciInfo> {
    unsafe {
        uart_write_string("Parsing Device Tree Blob at 0x40000000...\r\n");

        // Read and validate FDT header
        let magic = read_be32(DTB_BASE_ADDR);
        if magic != FDT_MAGIC {
            uart_write_string("ERROR: Invalid DTB magic number\r\n");
            return None;
        }

        let totalsize = read_be32(DTB_BASE_ADDR + 4);
        let off_dt_struct = read_be32(DTB_BASE_ADDR + 8);
        let off_dt_strings = read_be32(DTB_BASE_ADDR + 12);

        uart_write_string("DTB header valid, size: ");
        print_hex(totalsize as u64);
        uart_write_string("\r\n");

        // Start parsing structure block
        let struct_base = DTB_BASE_ADDR + off_dt_struct as u64;
        let strings_base = DTB_BASE_ADDR + off_dt_strings as u64;

        // Search for PCI controller node
        find_pci_node(struct_base, strings_base)
    }
}

/// Find and parse the PCI controller node in the device tree
unsafe fn find_pci_node(struct_base: u64, strings_base: u64) -> Option<PciInfo> {
    let mut offset = 0u64;
    let mut depth = 0usize;
    let mut in_pci_node = false;
    let mut pci_node_depth = 0usize;  // Track the depth at which we found the PCI node
    let mut pci_info = PciInfo::default();

    loop {
        let token = read_be32(struct_base + offset);
        offset += 4;

        match token {
            FDT_BEGIN_NODE => {
                // Read node name (null-terminated string)
                let name_start = struct_base + offset;
                let name = read_cstring(name_start);

                // Check if this is a PCI controller node
                // QEMU virt machine uses "pcie@10000000" or similar
                if name.starts_with("pcie@") || name.starts_with("pci@") {
                    uart_write_string("Found PCI node: ");
                    uart_write_string(&name);
                    uart_write_string("\r\n");
                    in_pci_node = true;
                    pci_node_depth = depth;  // Remember the depth where we found the PCI node
                }

                // Skip to next 4-byte aligned position after string
                while read_be32(struct_base + offset - 4) & 0xFF != 0 {
                    offset += 1;
                    if offset % 4 != 0 {
                        offset = (offset + 3) & !3;
                    }
                }
                offset = (offset + 3) & !3;
                depth += 1;
            }

            FDT_END_NODE => {
                depth = depth.saturating_sub(1);
                // Check if we're exiting the PCI node
                if in_pci_node && depth == pci_node_depth {
                    in_pci_node = false;
                    // We've finished parsing the PCI node
                    if pci_info.ecam_base != 0 {
                        return Some(pci_info);
                    }
                }
            }

            FDT_PROP => {
                let len = read_be32(struct_base + offset);
                offset += 4;
                let nameoff = read_be32(struct_base + offset);
                offset += 4;

                // Only parse properties that are direct children of the PCI node
                if in_pci_node && depth == pci_node_depth + 1 {
                    // Read property name
                    let prop_name = read_cstring(strings_base + nameoff as u64);

                    // Look for "reg" property which contains ECAM base address
                    if prop_name == "reg" && len >= 16 {
                        // reg property format: <address-cells size-cells>
                        // For PCI ECAM: typically 64-bit address + 64-bit size
                        pci_info.ecam_base = read_be64(struct_base + offset);
                        pci_info.ecam_size = read_be64(struct_base + offset + 8);

                        uart_write_string("PCI ECAM base: 0x");
                        print_hex(pci_info.ecam_base);
                        uart_write_string(", size: 0x");
                        print_hex(pci_info.ecam_size);
                        uart_write_string("\r\n");
                    }

                    // Look for "ranges" property for MMIO mapping
                    if prop_name == "ranges" && len >= 24 {
                        // Skip the first entry (usually config space), look for MMIO
                        // Format varies, but typically has multiple entries
                        // We'll take the second entry as MMIO
                        if len >= 56 {  // At least 2 entries
                            pci_info.mmio_base = read_be64(struct_base + offset + 32);
                            pci_info.mmio_size = read_be64(struct_base + offset + 48);

                            uart_write_string("PCI MMIO base: 0x");
                            print_hex(pci_info.mmio_base);
                            uart_write_string(", size: 0x");
                            print_hex(pci_info.mmio_size);
                            uart_write_string("\r\n");
                        }
                    }
                }

                // Skip property value (align to 4 bytes)
                offset += len as u64;
                offset = (offset + 3) & !3;
            }

            FDT_NOP => {
                // Skip NOPs
            }

            FDT_END => {
                break;
            }

            _ => {
                uart_write_string("Unknown DTB token\r\n");
                break;
            }
        }
    }

    if pci_info.ecam_base != 0 {
        Some(pci_info)
    } else {
        None
    }
}

/// Read a null-terminated C string from memory
unsafe fn read_cstring(addr: u64) -> &'static str {
    let mut len = 0;
    while core::ptr::read_volatile((addr + len) as *const u8) != 0 {
        len += 1;
        if len > 256 {
            break; // Safety limit
        }
    }

    let slice = core::slice::from_raw_parts(addr as *const u8, len as usize);
    core::str::from_utf8_unchecked(slice)
}

/// Print a 64-bit hex value to UART
fn print_hex(value: u64) {
    let hex_chars = b"0123456789abcdef";
    let mut buffer = [0u8; 16];

    for i in 0..16 {
        let nibble = ((value >> (60 - i * 4)) & 0xF) as usize;
        buffer[i] = hex_chars[nibble];
    }

    unsafe {
        for &byte in &buffer {
            core::ptr::write_volatile(0x09000000 as *mut u8, byte);
        }
    }
}
