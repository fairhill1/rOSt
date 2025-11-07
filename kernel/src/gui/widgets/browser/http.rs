use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;

/// Find the end of HTTP headers in binary data
/// Returns (start_of_separator, length_of_separator)
pub fn find_header_end(data: &[u8]) -> Option<(usize, usize)> {
    // Look for \r\n\r\n
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == b'\r' && data[i+1] == b'\n' && data[i+2] == b'\r' && data[i+3] == b'\n' {
            return Some((i, 4));
        }
    }
    // Look for \n\n
    for i in 0..data.len().saturating_sub(1) {
        if data[i] == b'\n' && data[i+1] == b'\n' {
            return Some((i, 2));
        }
    }
    None
}

/// Parse URL into host, port, and path
pub fn parse_url(url: &str) -> (String, u16, String) {
    // Remove http:// or https:// if present
    let url = url.trim_start_matches("http://").trim_start_matches("https://");

    let parts: Vec<&str> = url.splitn(2, '/').collect();
    let host_part = parts[0];
    let path = if parts.len() > 1 {
        format!("/{}", parts[1])
    } else {
        "/".to_string()
    };

    // Split host and port
    let (host, port) = if host_part.contains(':') {
        let parts: Vec<&str> = host_part.splitn(2, ':').collect();
        (parts[0].to_string(), parts[1].parse().unwrap_or(80))
    } else {
        (host_part.to_string(), 80)
    };

    (host, port, path)
}

/// Make HTTP GET request (returns HTML body only)
pub fn http_get(host: &str, port: u16, path: &str) -> Option<String> {
    unsafe {
        crate::kernel::uart_write_string("http_get: Starting (using smoltcp)\r\n");

        // Use smoltcp network stack
        let mut network_stack = crate::kernel::NETWORK_STACK.lock();
        let stack = match network_stack.as_mut() {
            Some(s) => s,
            None => {
                crate::kernel::uart_write_string("http_get: No network stack\r\n");
                return None;
            }
        };

        // Use smoltcp http_get helper
        match crate::system::net::helpers::http_get(stack, host, path, port, 10000) {
            Ok(response_data) => {
                crate::kernel::uart_write_string(&format!("http_get: Received {} bytes\r\n", response_data.len()));

                // Convert to string and extract body
                if let Ok(response) = core::str::from_utf8(&response_data) {
                    // Find the blank line that separates headers from body
                    if let Some(body_start) = response.find("\r\n\r\n") {
                        Some(response[body_start + 4..].to_string())
                    } else if let Some(body_start) = response.find("\n\n") {
                        Some(response[body_start + 2..].to_string())
                    } else {
                        Some(response.to_string())
                    }
                } else {
                    crate::kernel::uart_write_string("http_get: Invalid UTF-8 in response\r\n");
                    None
                }
            }
            Err(e) => {
                crate::kernel::uart_write_string(&format!("http_get: Error: {}\r\n", e));
                None
            }
        }
    }
}

/// Make HTTP GET request for binary data (images)
pub fn http_get_binary(host: &str, port: u16, path: &str) -> Option<Vec<u8>> {
    unsafe {
        crate::kernel::uart_write_string("http_get_binary: Starting (using smoltcp)\r\n");

        // Use smoltcp network stack
        let mut network_stack = crate::kernel::NETWORK_STACK.lock();
        let stack = match network_stack.as_mut() {
            Some(s) => s,
            None => {
                crate::kernel::uart_write_string("http_get_binary: No network stack\r\n");
                return None;
            }
        };

        // Use smoltcp http_get helper
        match crate::system::net::helpers::http_get(stack, host, path, port, 10000) {
            Ok(response_data) => {
                crate::kernel::uart_write_string(&format!("http_get_binary: Received {} bytes\r\n", response_data.len()));

                // Find the blank line that separates headers from body
                if let Some(body_start) = response_data.windows(4).position(|w| w == b"\r\n\r\n") {
                    // Print the headers
                    if let Ok(headers) = core::str::from_utf8(&response_data[0..body_start]) {
                        crate::kernel::uart_write_string(&format!(
                            "http_get_binary: HTTP Headers:\r\n{}\r\n",
                            headers
                        ));
                    }

                    let body_len = response_data.len() - (body_start + 4);
                    crate::kernel::uart_write_string(&format!(
                        "http_get_binary: Body is {} bytes\r\n",
                        body_len
                    ));
                    Some(response_data[body_start + 4..].to_vec())
                } else if let Some(body_start) = response_data.windows(2).position(|w| w == b"\n\n") {
                    let body_len = response_data.len() - (body_start + 2);
                    crate::kernel::uart_write_string(&format!(
                        "http_get_binary: Found \\n\\n at position {}, body is {} bytes\r\n",
                        body_start, body_len
                    ));
                    Some(response_data[body_start + 2..].to_vec())
                } else {
                    crate::kernel::uart_write_string("http_get_binary: No header separator found, returning all data\r\n");
                    Some(response_data)
                }
            }
            Err(e) => {
                crate::kernel::uart_write_string(&format!("http_get_binary: Error: {}\r\n", e));
                None
            }
        }
    }
}
