// TCP connection management
// Simple TCP implementation for client-side connections

use alloc::vec::Vec;
use crate::kernel::virtio_net::VirtioNetDevice;
use crate::kernel::network::*;

/// TCP connection states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    TimeWait,
}

/// TCP connection
pub struct TcpConnection {
    pub state: TcpState,
    pub local_ip: [u8; 4],
    pub remote_ip: [u8; 4],
    pub local_port: u16,
    pub remote_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub window_size: u16,
}

impl TcpConnection {
    /// Create a new TCP connection in CLOSED state
    pub fn new(local_ip: [u8; 4], remote_ip: [u8; 4], local_port: u16, remote_port: u16) -> Self {
        // Use a simple ISN based on port numbers (in real TCP, this should be random)
        let isn = (local_port as u32) * 1000 + (remote_port as u32);

        TcpConnection {
            state: TcpState::Closed,
            local_ip,
            remote_ip,
            local_port,
            remote_port,
            seq_num: isn,
            ack_num: 0,
            window_size: 8192,  // 8KB window
        }
    }

    /// Initiate connection (send SYN)
    pub fn connect(
        &mut self,
        device: &mut VirtioNetDevice,
        gateway_mac: [u8; 6],
        local_mac: [u8; 6],
    ) -> Result<(), &'static str> {
        if self.state != TcpState::Closed {
            return Err("Connection not in CLOSED state");
        }

        // Build SYN packet
        let tcp_segment = build_tcp(
            self.local_ip,
            self.remote_ip,
            self.local_port,
            self.remote_port,
            self.seq_num,
            0,  // No ACK yet
            TCP_FLAG_SYN,
            self.window_size,
            &[],  // No data in SYN
        );

        let ip_packet = build_ipv4(
            self.local_ip,
            self.remote_ip,
            IP_PROTO_TCP,
            &tcp_segment,
            1,
        );

        let eth_frame = build_ethernet(
            gateway_mac,
            local_mac,
            ETHERTYPE_IPV4,
            &ip_packet,
        );

        device.transmit(&eth_frame)?;

        // Move to SYN_SENT state
        self.state = TcpState::SynSent;
        self.seq_num = self.seq_num.wrapping_add(1);  // SYN consumes one sequence number

        Ok(())
    }

    /// Handle incoming TCP segment
    pub fn handle_segment(&mut self, tcp_hdr: &TcpHeader, _data: &[u8]) -> Result<(), &'static str> {
        let flags = u16::from_be(tcp_hdr.data_offset_flags) & 0x1FF;  // Extract lower 9 bits (flags)
        let remote_seq = be32_to_cpu(tcp_hdr.seq_num);
        let remote_ack = be32_to_cpu(tcp_hdr.ack_num);

        match self.state {
            TcpState::SynSent => {
                // Expecting SYN-ACK
                if (flags & TCP_FLAG_SYN != 0) && (flags & TCP_FLAG_ACK != 0) {
                    // Verify ACK number
                    if remote_ack != self.seq_num {
                        return Err("Invalid ACK number in SYN-ACK");
                    }

                    // Save server's sequence number
                    self.ack_num = remote_seq.wrapping_add(1);  // SYN consumes one seq number

                    // Move to ESTABLISHED state (we'll send ACK separately)
                    self.state = TcpState::Established;
                    Ok(())
                } else {
                    Err("Expected SYN-ACK, got different flags")
                }
            }
            TcpState::Established => {
                // Handle data or ACK
                if flags & TCP_FLAG_ACK != 0 {
                    // Update our sequence number based on ACK
                    // (In a real implementation, we'd track unacknowledged data)
                }

                Ok(())
            }
            TcpState::FinWait1 => {
                // Expecting ACK or FIN-ACK
                if flags & TCP_FLAG_FIN != 0 {
                    self.ack_num = remote_seq.wrapping_add(1);
                    self.state = TcpState::TimeWait;
                } else if flags & TCP_FLAG_ACK != 0 {
                    self.state = TcpState::FinWait2;
                }
                Ok(())
            }
            TcpState::FinWait2 => {
                // Expecting FIN
                if flags & TCP_FLAG_FIN != 0 {
                    self.ack_num = remote_seq.wrapping_add(1);
                    self.state = TcpState::TimeWait;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Send ACK (used after receiving SYN-ACK)
    pub fn send_ack(
        &mut self,
        device: &mut VirtioNetDevice,
        gateway_mac: [u8; 6],
        local_mac: [u8; 6],
    ) -> Result<(), &'static str> {
        let tcp_segment = build_tcp(
            self.local_ip,
            self.remote_ip,
            self.local_port,
            self.remote_port,
            self.seq_num,
            self.ack_num,
            TCP_FLAG_ACK,
            self.window_size,
            &[],
        );

        let ip_packet = build_ipv4(
            self.local_ip,
            self.remote_ip,
            IP_PROTO_TCP,
            &tcp_segment,
            2,
        );

        let eth_frame = build_ethernet(
            gateway_mac,
            local_mac,
            ETHERTYPE_IPV4,
            &ip_packet,
        );

        device.transmit(&eth_frame)?;
        Ok(())
    }

    /// Send data
    pub fn send_data(
        &mut self,
        device: &mut VirtioNetDevice,
        gateway_mac: [u8; 6],
        local_mac: [u8; 6],
        data: &[u8],
    ) -> Result<(), &'static str> {
        if self.state != TcpState::Established {
            return Err("Connection not established");
        }

        let tcp_segment = build_tcp(
            self.local_ip,
            self.remote_ip,
            self.local_port,
            self.remote_port,
            self.seq_num,
            self.ack_num,
            TCP_FLAG_PSH | TCP_FLAG_ACK,
            self.window_size,
            data,
        );

        let ip_packet = build_ipv4(
            self.local_ip,
            self.remote_ip,
            IP_PROTO_TCP,
            &tcp_segment,
            3,
        );

        let eth_frame = build_ethernet(
            gateway_mac,
            local_mac,
            ETHERTYPE_IPV4,
            &ip_packet,
        );

        device.transmit(&eth_frame)?;

        // Update sequence number
        self.seq_num = self.seq_num.wrapping_add(data.len() as u32);

        Ok(())
    }

    /// Close connection (send FIN)
    pub fn close(
        &mut self,
        device: &mut VirtioNetDevice,
        gateway_mac: [u8; 6],
        local_mac: [u8; 6],
    ) -> Result<(), &'static str> {
        if self.state != TcpState::Established {
            return Err("Connection not established");
        }

        let tcp_segment = build_tcp(
            self.local_ip,
            self.remote_ip,
            self.local_port,
            self.remote_port,
            self.seq_num,
            self.ack_num,
            TCP_FLAG_FIN | TCP_FLAG_ACK,
            self.window_size,
            &[],
        );

        let ip_packet = build_ipv4(
            self.local_ip,
            self.remote_ip,
            IP_PROTO_TCP,
            &tcp_segment,
            4,
        );

        let eth_frame = build_ethernet(
            gateway_mac,
            local_mac,
            ETHERTYPE_IPV4,
            &ip_packet,
        );

        device.transmit(&eth_frame)?;

        self.state = TcpState::FinWait1;
        self.seq_num = self.seq_num.wrapping_add(1);  // FIN consumes one sequence number

        Ok(())
    }
}
