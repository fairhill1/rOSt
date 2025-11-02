// Network utility functions
// These utilities are used by shell commands and other parts of the system

/// Format MAC address as string
pub fn format_mac(mac: [u8; 6]) -> alloc::string::String {
    alloc::format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
}

/// Format IP address as string
pub fn format_ip(ip: [u8; 4]) -> alloc::string::String {
    alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Parse IP address from string (e.g., "192.168.1.1")
pub fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let parts: alloc::vec::Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let mut ip = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        ip[i] = part.parse().ok()?;
    }

    Some(ip)
}
