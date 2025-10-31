// Network protocol stack implementation
// Supports: Ethernet, ARP, IPv4, ICMP

use alloc::vec::Vec;
use core::ptr;

/// Ethernet frame header (14 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct EthernetFrame {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16, // Big-endian
}

/// ARP packet (28 bytes for IPv4/Ethernet)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct ArpPacket {
    pub hw_type: u16,      // Hardware type (1 = Ethernet)
    pub proto_type: u16,   // Protocol type (0x0800 = IPv4)
    pub hw_len: u8,        // Hardware address length (6 for MAC)
    pub proto_len: u8,     // Protocol address length (4 for IPv4)
    pub operation: u16,    // 1 = request, 2 = reply
    pub sender_mac: [u8; 6],
    pub sender_ip: [u8; 4],
    pub target_mac: [u8; 6],
    pub target_ip: [u8; 4],
}

/// IPv4 header (20 bytes minimum)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Ipv4Header {
    pub version_ihl: u8,   // Version (4 bits) + IHL (4 bits)
    pub dscp_ecn: u8,      // DSCP (6 bits) + ECN (2 bits)
    pub total_length: u16, // Big-endian
    pub identification: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
}

/// ICMP header
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub id: u16,
    pub sequence: u16,
}

// Ethernet types
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

// IP protocols
pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_TCP: u8 = 6;
pub const IP_PROTO_UDP: u8 = 17;

// ICMP types
pub const ICMP_ECHO_REPLY: u8 = 0;
pub const ICMP_ECHO_REQUEST: u8 = 8;

// ARP operations
pub const ARP_REQUEST: u16 = 0x0001;
pub const ARP_REPLY: u16 = 0x0002;

/// Convert u16 from big-endian to native
pub fn be16_to_cpu(val: u16) -> u16 {
    u16::from_be(val)
}

/// Convert u16 from native to big-endian
pub fn cpu_to_be16(val: u16) -> u16 {
    val.to_be()
}

/// Calculate IP/ICMP checksum
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Sum 16-bit words
    for i in (0..data.len()).step_by(2) {
        if i + 1 < data.len() {
            let word = ((data[i] as u32) << 8) | (data[i + 1] as u32);
            sum += word;
        } else {
            // Odd number of bytes, pad with zero
            sum += (data[i] as u32) << 8;
        }
    }

    // Add carry bits
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    // One's complement
    !sum as u16
}

/// Parse an Ethernet frame
pub fn parse_ethernet(packet: &[u8]) -> Option<(EthernetFrame, &[u8])> {
    if packet.len() < 14 {
        return None;
    }

    let frame = unsafe {
        ptr::read_unaligned(packet.as_ptr() as *const EthernetFrame)
    };

    let payload = &packet[14..];
    Some((frame, payload))
}

/// Build an Ethernet frame
pub fn build_ethernet(dst_mac: [u8; 6], src_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::new();

    packet.extend_from_slice(&dst_mac);
    packet.extend_from_slice(&src_mac);
    packet.extend_from_slice(&ethertype.to_be_bytes());  // Just convert to big-endian bytes directly
    packet.extend_from_slice(payload);

    packet
}

/// Build an ARP request
pub fn build_arp_request(src_mac: [u8; 6], src_ip: [u8; 4], target_ip: [u8; 4]) -> Vec<u8> {
    let arp = ArpPacket {
        hw_type: cpu_to_be16(1),      // Ethernet
        proto_type: cpu_to_be16(0x0800), // IPv4
        hw_len: 6,
        proto_len: 4,
        operation: cpu_to_be16(ARP_REQUEST),
        sender_mac: src_mac,
        sender_ip: src_ip,
        target_mac: [0; 6],  // Unknown
        target_ip,
    };

    let arp_bytes = unsafe {
        core::slice::from_raw_parts(&arp as *const _ as *const u8, core::mem::size_of::<ArpPacket>())
    };

    // Build Ethernet frame with ARP payload
    build_ethernet([0xff; 6], src_mac, ETHERTYPE_ARP, arp_bytes)
}

/// Build an ARP reply
pub fn build_arp_reply(src_mac: [u8; 6], src_ip: [u8; 4], target_mac: [u8; 6], target_ip: [u8; 4]) -> Vec<u8> {
    let arp = ArpPacket {
        hw_type: cpu_to_be16(1),
        proto_type: cpu_to_be16(0x0800),
        hw_len: 6,
        proto_len: 4,
        operation: cpu_to_be16(ARP_REPLY),
        sender_mac: src_mac,
        sender_ip: src_ip,
        target_mac,
        target_ip,
    };

    let arp_bytes = unsafe {
        core::slice::from_raw_parts(&arp as *const _ as *const u8, core::mem::size_of::<ArpPacket>())
    };

    build_ethernet(target_mac, src_mac, ETHERTYPE_ARP, arp_bytes)
}

/// Parse an ARP packet
pub fn parse_arp(payload: &[u8]) -> Option<ArpPacket> {
    if payload.len() < core::mem::size_of::<ArpPacket>() {
        return None;
    }

    let arp = unsafe {
        ptr::read_unaligned(payload.as_ptr() as *const ArpPacket)
    };

    Some(arp)
}

/// Build an IPv4 packet
pub fn build_ipv4(src_ip: [u8; 4], dst_ip: [u8; 4], protocol: u8, payload: &[u8], id: u16) -> Vec<u8> {
    let total_len = 20 + payload.len();

    let mut header = Ipv4Header {
        version_ihl: 0x45,  // Version 4, IHL = 5 (20 bytes)
        dscp_ecn: 0,
        total_length: cpu_to_be16(total_len as u16),
        identification: cpu_to_be16(id),
        flags_fragment: 0,
        ttl: 64,
        protocol,
        checksum: 0,  // Will calculate after
        src_ip,
        dst_ip,
    };

    // Calculate checksum
    let header_bytes = unsafe {
        core::slice::from_raw_parts(&header as *const _ as *const u8, 20)
    };
    header.checksum = cpu_to_be16(checksum(header_bytes));

    let mut packet = Vec::new();
    let final_header_bytes = unsafe {
        core::slice::from_raw_parts(&header as *const _ as *const u8, 20)
    };
    packet.extend_from_slice(final_header_bytes);
    packet.extend_from_slice(payload);

    packet
}

/// Parse an IPv4 packet
pub fn parse_ipv4(payload: &[u8]) -> Option<(Ipv4Header, &[u8])> {
    if payload.len() < 20 {
        return None;
    }

    let header = unsafe {
        ptr::read_unaligned(payload.as_ptr() as *const Ipv4Header)
    };

    let ihl = (header.version_ihl & 0x0F) as usize * 4;
    if payload.len() < ihl {
        return None;
    }

    let data = &payload[ihl..];
    Some((header, data))
}

/// Build an ICMP echo request (ping)
pub fn build_icmp_echo_request(id: u16, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut icmp = IcmpHeader {
        icmp_type: ICMP_ECHO_REQUEST,
        code: 0,
        checksum: 0,
        id: cpu_to_be16(id),
        sequence: cpu_to_be16(seq),
    };

    // Build full packet for checksum
    let mut packet = Vec::new();
    let header_bytes = unsafe {
        core::slice::from_raw_parts(&icmp as *const _ as *const u8, 8)
    };
    packet.extend_from_slice(header_bytes);
    packet.extend_from_slice(payload);

    // Calculate checksum
    icmp.checksum = cpu_to_be16(checksum(&packet));

    // Rebuild with correct checksum
    packet.clear();
    let final_header_bytes = unsafe {
        core::slice::from_raw_parts(&icmp as *const _ as *const u8, 8)
    };
    packet.extend_from_slice(final_header_bytes);
    packet.extend_from_slice(payload);

    packet
}

/// Build an ICMP echo reply (pong)
pub fn build_icmp_echo_reply(id: u16, seq: u16, payload: &[u8]) -> Vec<u8> {
    let mut icmp = IcmpHeader {
        icmp_type: ICMP_ECHO_REPLY,
        code: 0,
        checksum: 0,
        id: cpu_to_be16(id),
        sequence: cpu_to_be16(seq),
    };

    // Build full packet for checksum
    let mut packet = Vec::new();
    let header_bytes = unsafe {
        core::slice::from_raw_parts(&icmp as *const _ as *const u8, 8)
    };
    packet.extend_from_slice(header_bytes);
    packet.extend_from_slice(payload);

    // Calculate checksum
    icmp.checksum = cpu_to_be16(checksum(&packet));

    // Rebuild with correct checksum
    packet.clear();
    let final_header_bytes = unsafe {
        core::slice::from_raw_parts(&icmp as *const _ as *const u8, 8)
    };
    packet.extend_from_slice(final_header_bytes);
    packet.extend_from_slice(payload);

    packet
}

/// Parse an ICMP packet
pub fn parse_icmp(payload: &[u8]) -> Option<(IcmpHeader, &[u8])> {
    if payload.len() < 8 {
        return None;
    }

    let header = unsafe {
        ptr::read_unaligned(payload.as_ptr() as *const IcmpHeader)
    };

    let data = &payload[8..];
    Some((header, data))
}

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
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let mut ip = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        ip[i] = part.parse().ok()?;
    }

    Some(ip)
}

/// Simple ARP cache entry
pub struct ArpCacheEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
}

/// Simple ARP cache (fixed size)
pub struct ArpCache {
    entries: [Option<ArpCacheEntry>; 16],
}

impl ArpCache {
    pub fn new() -> Self {
        ArpCache {
            entries: [None, None, None, None, None, None, None, None,
                     None, None, None, None, None, None, None, None],
        }
    }

    pub fn add(&mut self, ip: [u8; 4], mac: [u8; 6]) {
        // Check if entry already exists
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.ip == ip {
                    e.mac = mac;
                    return;
                }
            }
        }

        // Find empty slot
        for entry in self.entries.iter_mut() {
            if entry.is_none() {
                *entry = Some(ArpCacheEntry { ip, mac });
                return;
            }
        }

        // Cache full, replace first entry (simple FIFO)
        self.entries[0] = Some(ArpCacheEntry { ip, mac });
    }

    pub fn lookup(&self, ip: [u8; 4]) -> Option<[u8; 6]> {
        for entry in self.entries.iter() {
            if let Some(e) = entry {
                if e.ip == ip {
                    return Some(e.mac);
                }
            }
        }
        None
    }
}
