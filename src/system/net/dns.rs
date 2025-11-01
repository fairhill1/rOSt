// DNS resolver implementation
// Supports A record queries (IPv4 addresses)

use alloc::vec::Vec;
use alloc::string::String;
use core::ptr;

/// DNS header (12 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,  // Number of questions
    pub ancount: u16,  // Number of answers
    pub nscount: u16,  // Number of authority records
    pub arcount: u16,  // Number of additional records
}

// DNS query types
pub const DNS_TYPE_A: u16 = 1;      // IPv4 address
pub const DNS_TYPE_AAAA: u16 = 28;  // IPv6 address
pub const DNS_TYPE_CNAME: u16 = 5;  // Canonical name

// DNS query classes
pub const DNS_CLASS_IN: u16 = 1;  // Internet

// DNS header flags
pub const DNS_FLAG_RD: u16 = 0x0100;  // Recursion desired (big-endian)

/// Encode a domain name to DNS format
/// Example: "google.com" -> [6]google[3]com[0]
pub fn encode_domain_name(domain: &str) -> Vec<u8> {
    let mut encoded = Vec::new();

    for label in domain.split('.') {
        if label.len() > 63 {
            // Label too long, truncate
            encoded.push(63);
            encoded.extend_from_slice(&label.as_bytes()[..63]);
        } else {
            encoded.push(label.len() as u8);
            encoded.extend_from_slice(label.as_bytes());
        }
    }

    encoded.push(0);  // Null terminator
    encoded
}

/// Decode a domain name from DNS format
/// Handles label pointers (compression)
pub fn decode_domain_name(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut name = String::new();
    let mut pos = offset;
    let mut jumped = false;
    let mut jump_pos = 0;
    let mut first_label = true;

    loop {
        if pos >= data.len() {
            return None;
        }

        let len = data[pos];

        // Check for pointer (top 2 bits set)
        if (len & 0xC0) == 0xC0 {
            if pos + 1 >= data.len() {
                return None;
            }

            // Extract pointer offset
            let pointer = (((len & 0x3F) as u16) << 8) | (data[pos + 1] as u16);

            if !jumped {
                jump_pos = pos + 2;
                jumped = true;
            }

            pos = pointer as usize;
            continue;
        }

        // End of name
        if len == 0 {
            pos += 1;
            break;
        }

        // Add dot separator between labels
        if !first_label {
            name.push('.');
        }
        first_label = false;

        // Read label
        pos += 1;
        if pos + len as usize > data.len() {
            return None;
        }

        for i in 0..len as usize {
            name.push(data[pos + i] as char);
        }
        pos += len as usize;
    }

    let final_pos = if jumped { jump_pos } else { pos };
    Some((name, final_pos))
}

/// Build a DNS query packet
pub fn build_dns_query(domain: &str, query_type: u16, query_id: u16) -> Vec<u8> {
    let mut packet = Vec::new();

    // DNS header
    let header = DnsHeader {
        id: query_id.to_be(),
        flags: DNS_FLAG_RD,  // Recursion desired (already in big-endian)
        qdcount: 1u16.to_be(),  // 1 question
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    let header_bytes = unsafe {
        core::slice::from_raw_parts(&header as *const _ as *const u8, 12)
    };
    packet.extend_from_slice(header_bytes);

    // Question section
    let qname = encode_domain_name(domain);
    packet.extend_from_slice(&qname);
    packet.extend_from_slice(&query_type.to_be_bytes());  // QTYPE
    packet.extend_from_slice(&DNS_CLASS_IN.to_be_bytes());  // QCLASS

    packet
}

/// Parse a DNS response packet
pub fn parse_dns_response(data: &[u8]) -> Option<Vec<[u8; 4]>> {
    if data.len() < 12 {
        return None;
    }

    let header = unsafe {
        ptr::read_unaligned(data.as_ptr() as *const DnsHeader)
    };

    let ancount = u16::from_be(header.ancount);
    let qdcount = u16::from_be(header.qdcount);

    // Skip header (12 bytes)
    let mut pos = 12;

    // Skip question section
    for _ in 0..qdcount {
        // Skip QNAME
        loop {
            if pos >= data.len() {
                return None;
            }

            let len = data[pos];
            pos += 1;

            // Check for pointer
            if (len & 0xC0) == 0xC0 {
                pos += 1;  // Skip second byte of pointer
                break;
            }

            // End of name
            if len == 0 {
                break;
            }

            // Skip label
            pos += len as usize;
        }

        // Skip QTYPE and QCLASS (4 bytes)
        pos += 4;
    }

    // Parse answer section
    let mut addresses = Vec::new();

    for _ in 0..ancount {
        // Skip NAME
        loop {
            if pos >= data.len() {
                return None;
            }

            let len = data[pos];
            pos += 1;

            // Check for pointer
            if (len & 0xC0) == 0xC0 {
                pos += 1;  // Skip second byte of pointer
                break;
            }

            // End of name
            if len == 0 {
                break;
            }

            // Skip label
            pos += len as usize;
        }

        // Read TYPE, CLASS, TTL, RDLENGTH
        if pos + 10 > data.len() {
            return None;
        }

        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        // Skip CLASS (2 bytes)
        pos += 2;

        // Skip TTL (4 bytes)
        pos += 4;

        let rdlength = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        // Read RDATA
        if pos + rdlength as usize > data.len() {
            return None;
        }

        // If this is an A record (IPv4 address)
        if rtype == DNS_TYPE_A && rdlength == 4 {
            let ip = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
            addresses.push(ip);
        }

        pos += rdlength as usize;
    }

    Some(addresses)
}
