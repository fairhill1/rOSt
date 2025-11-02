// High-level networking helper functions using smoltcp
// These provide easy-to-use interfaces for common network operations

use crate::system::net::NetworkStack;
use crate::kernel::drivers::timer;
use smoltcp::wire::{IpAddress, Ipv4Address, Icmpv4Packet, Icmpv4Repr};
use smoltcp::socket::icmp;
use alloc::vec::Vec;
use alloc::vec;

/// Ping a host (send ICMP echo request and wait for reply)
pub fn ping(stack: &mut NetworkStack, target_ip: [u8; 4], timeout_ms: u64) -> Result<u64, &'static str> {
    // Create ICMP socket
    let icmp_handle = stack.create_icmp_socket();

    let target = IpAddress::Ipv4(Ipv4Address::new(target_ip[0], target_ip[1], target_ip[2], target_ip[3]));

    // Build ICMPv4 echo request
    let icmp_data = b"rOSt ping!";
    let ident = 0x1234u16;
    let seq_no = 1u16;

    let icmp_repr = Icmpv4Repr::EchoRequest {
        ident,
        seq_no,
        data: icmp_data,
    };

    // Calculate buffer size needed
    let icmp_packet_size = icmp_repr.buffer_len();
    let mut buffer = vec![0u8; icmp_packet_size];

    // Serialize the ICMP packet into the buffer
    let mut icmp_packet = Icmpv4Packet::new_unchecked(&mut buffer[..]);
    icmp_repr.emit(&mut icmp_packet, &smoltcp::phy::ChecksumCapabilities::default());

    // Bind socket and send echo request
    stack.with_icmp_socket(icmp_handle, |socket| {
        socket.bind(icmp::Endpoint::Ident(ident)).ok();

        // Send the ICMP packet with the serialized data
        let _ = socket.send_slice(&buffer, target);
    });

    let start_time = timer::get_time_ms();
    let mut received = false;
    let mut rtt = 0u64;

    // Poll for response with timeout
    while timer::get_time_ms() - start_time < timeout_ms {
        stack.poll();

        stack.with_icmp_socket(icmp_handle, |socket| {
            if socket.can_recv() {
                if let Ok((payload, _addr)) = socket.recv() {
                    // Parse the ICMP packet to verify it's an echo reply
                    if payload.len() >= 8 {
                        let icmp_type = payload[0];
                        let icmp_code = payload[1];
                        let reply_ident = u16::from_be_bytes([payload[4], payload[5]]);
                        let reply_seq = u16::from_be_bytes([payload[6], payload[7]]);

                        // ICMP Echo Reply: type=0, code=0
                        if icmp_type == 0 && icmp_code == 0 && reply_ident == ident && reply_seq == seq_no {
                            rtt = timer::get_time_ms() - start_time;
                            received = true;
                        }
                    }
                }
            }
        });

        if received {
            break;
        }
    }

    // Clean up socket
    stack.remove_socket(icmp_handle);

    if received {
        Ok(rtt)
    } else {
        Err("Ping timeout")
    }
}

/// DNS lookup (resolve domain name to IP addresses)
pub fn dns_lookup(stack: &mut NetworkStack, domain: &str, timeout_ms: u64) -> Result<Vec<[u8; 4]>, &'static str> {
    // Create UDP socket
    let udp_handle = stack.create_udp_socket();

    // DNS server (Google DNS)
    let dns_server = IpAddress::Ipv4(Ipv4Address::new(8, 8, 8, 8));

    // Generate query ID
    static mut QUERY_ID: u16 = 1;
    let query_id = unsafe {
        let id = QUERY_ID;
        QUERY_ID = QUERY_ID.wrapping_add(1);
        id
    };

    // Build DNS query packet
    let dns_query = crate::system::net::dns::build_dns_query(
        domain,
        crate::system::net::dns::DNS_TYPE_A,
        query_id
    );

    // Bind socket to local port 12345
    stack.with_udp_socket(udp_handle, |socket| {
        socket.bind(12345).ok();

        // Send DNS query to DNS server port 53
        let _ = socket.send_slice(&dns_query, (dns_server, 53));
    });

    let start_time = timer::get_time_ms();
    let mut addresses = Vec::new();
    let mut received = false;

    // Poll for response with timeout
    while timer::get_time_ms() - start_time < timeout_ms {
        stack.poll();

        stack.with_udp_socket(udp_handle, |socket| {
            if socket.can_recv() {
                if let Ok((payload, endpoint)) = socket.recv() {
                    // Check if response is from DNS server port 53
                    if endpoint.endpoint.port == 53 {
                        // Parse DNS response
                        if let Some(addrs) = crate::system::net::dns::parse_dns_response(payload) {
                            addresses = addrs;
                            received = true;
                        }
                    }
                }
            }
        });

        if received {
            break;
        }
    }

    // Clean up socket
    stack.remove_socket(udp_handle);

    if received {
        if addresses.is_empty() {
            Err("No A records found")
        } else {
            Ok(addresses)
        }
    } else {
        Err("DNS timeout")
    }
}

/// HTTP GET request
/// Returns the HTTP response body (or full response if include_headers is true)
pub fn http_get(
    stack: &mut NetworkStack,
    host: &str,
    path: &str,
    port: u16,
    timeout_ms: u64,
) -> Result<Vec<u8>, &'static str> {
    // Step 1: Resolve host to IP (if it's a domain name)
    let server_ip = if let Some(ip) = crate::system::net::network::parse_ip(host) {
        // Already an IP address
        IpAddress::Ipv4(Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]))
    } else {
        // Need to resolve via DNS
        let addresses = dns_lookup(stack, host, timeout_ms)?;
        if addresses.is_empty() {
            return Err("DNS resolution failed");
        }
        let ip = addresses[0];
        IpAddress::Ipv4(Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]))
    };

    // Step 2: Create TCP socket
    let tcp_handle = stack.create_tcp_socket();

    // Step 3: Connect to server
    let start_time = timer::get_time_ms();

    // Use dynamic local port to avoid conflicts
    static mut LOCAL_PORT_COUNTER: u16 = 49152;
    let local_port = unsafe {
        let port = LOCAL_PORT_COUNTER;
        LOCAL_PORT_COUNTER = if LOCAL_PORT_COUNTER >= 65000 { 49152 } else { LOCAL_PORT_COUNTER + 1 };
        port
    };

    // Initiate connection
    use smoltcp::wire::IpEndpoint;
    let remote_endpoint = IpEndpoint::new(server_ip, port);

    if let Err(_) = stack.tcp_connect(tcp_handle, remote_endpoint, local_port) {
        stack.remove_socket(tcp_handle);
        return Err("Failed to initiate TCP connection");
    }

    // Wait for connection to establish
    let mut connected = false;
    while timer::get_time_ms() - start_time < timeout_ms {
        stack.poll();

        let is_active = stack.with_tcp_socket(tcp_handle, |socket| {
            socket.may_send() && socket.may_recv()
        });

        if is_active {
            connected = true;
            break;
        }

        // Small delay to avoid busy loop
        timer::delay_us(1000); // 1ms
    }

    if !connected {
        stack.remove_socket(tcp_handle);
        return Err("TCP connection timeout");
    }

    // Step 4: Send HTTP GET request
    let http_request = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, host
    );

    stack.with_tcp_socket(tcp_handle, |socket| {
        socket.send_slice(http_request.as_bytes()).ok();
    });

    // Step 5: Receive HTTP response
    let mut response_data = Vec::new();
    let recv_start = timer::get_time_ms();
    let mut last_recv_time = recv_start;
    let mut poll_count = 0;
    let mut content_length: Option<usize> = None;
    let mut headers_complete = false;

    loop {
        stack.poll();
        poll_count += 1;

        let mut received_data = false;
        let can_recv = stack.with_tcp_socket(tcp_handle, |socket| socket.can_recv());

        // Drain socket buffer completely - loop until can_recv() is false
        stack.with_tcp_socket(tcp_handle, |socket| {
            while socket.can_recv() {
                if let Ok(_) = socket.recv(|buffer| {
                    let len = buffer.len();
                    if len > 0 {
                        crate::kernel::uart_write_string(&alloc::format!(
                            "[HTTP] Poll {}: recv {} bytes (total: {})\r\n",
                            poll_count, len, response_data.len() + len
                        ));
                        response_data.extend_from_slice(buffer);
                        received_data = true;
                    }
                    (len, ())
                }) {
                    // Continue draining
                } else {
                    break;
                }
            }

            if !received_data && poll_count % 500 == 0 {
                // Periodically log socket state during idle time
                crate::kernel::uart_write_string(&alloc::format!(
                    "[HTTP] Poll {}: can_recv={}, total={} bytes\r\n",
                    poll_count, can_recv, response_data.len()
                ));
            }
        });

        if received_data {
            last_recv_time = timer::get_time_ms();

            // Try to parse Content-Length from headers if we haven't yet
            if !headers_complete {
                if let Some(header_end) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
                    headers_complete = true;
                    if let Ok(headers) = core::str::from_utf8(&response_data[0..header_end]) {
                        for line in headers.lines() {
                            if line.to_lowercase().starts_with("content-length:") {
                                if let Some(len_str) = line.split(':').nth(1) {
                                    if let Ok(len) = len_str.trim().parse::<usize>() {
                                        content_length = Some(len);
                                        crate::kernel::uart_write_string(&alloc::format!(
                                            "[HTTP] Parsed Content-Length: {} bytes\r\n", len
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Check if we've received the complete response based on Content-Length
            if let Some(expected_len) = content_length {
                if headers_complete {
                    let header_end = response_data.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                    let body_len = response_data.len() - (header_end + 4);
                    if body_len >= expected_len {
                        crate::kernel::uart_write_string(&alloc::format!(
                            "[HTTP] Complete: got {} of {} body bytes\r\n",
                            body_len, expected_len
                        ));
                        break;
                    }
                }
            }
        }

        // Check if socket is no longer active (connection closed)
        let is_active = stack.with_tcp_socket(tcp_handle, |socket| socket.is_active());
        if !is_active {
            // Connection closed - try to read any remaining data in the receive buffer
            crate::kernel::uart_write_string("[HTTP] Connection closed, draining receive buffer\r\n");
            stack.with_tcp_socket(tcp_handle, |socket| {
                while socket.can_recv() {
                    if let Ok(_) = socket.recv(|buffer| {
                        let len = buffer.len();
                        if len > 0 {
                            crate::kernel::uart_write_string(&alloc::format!(
                                "[HTTP] Final drain: recv {} bytes (total: {})\r\n",
                                len, response_data.len() + len
                            ));
                            response_data.extend_from_slice(buffer);
                        }
                        (len, ())
                    }) {
                        // Continue draining
                    } else {
                        break;
                    }
                }
            });

            if !response_data.is_empty() {
                crate::kernel::uart_write_string(&alloc::format!(
                    "[HTTP] Connection closed, got {} bytes total\r\n", response_data.len()
                ));
                break;
            } else if timer::get_time_ms() - recv_start > 100 {
                // Connection closed but no data after 100ms - probably failed
                crate::kernel::uart_write_string("[HTTP] Connection closed with no data\r\n");
                break;
            }
        }

        // Exit if no data received for 5 seconds (idle timeout)
        if !response_data.is_empty() && timer::get_time_ms() - last_recv_time > 5000 {
            crate::kernel::uart_write_string(&alloc::format!(
                "[HTTP] Idle timeout (waited 5s, no more data)\r\n"
            ));
            break;
        }

        // Absolute timeout check
        if timer::get_time_ms() - recv_start > timeout_ms {
            crate::kernel::uart_write_string("[HTTP] Absolute timeout\r\n");
            break;
        }

        // Small delay to avoid busy loop
        timer::delay_us(1000); // 1ms
    }

    // Clean up socket
    stack.remove_socket(tcp_handle);

    if response_data.is_empty() {
        Err("No HTTP response received")
    } else {
        Ok(response_data)
    }
}
