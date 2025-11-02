// smoltcp-based network stack
// High-level networking API using smoltcp

use crate::kernel::drivers::timer;
use crate::system::net::smoltcp_device::SmoltcpVirtioNetDevice;
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle};
use smoltcp::socket::{tcp, udp, icmp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
use alloc::vec::Vec;

// Import the vec! macro
extern crate alloc;
use alloc::vec;

/// Network stack managing smoltcp interface and sockets
pub struct NetworkStack {
    interface: Interface,
    sockets: SocketSet<'static>,
    device: SmoltcpVirtioNetDevice,
}

impl NetworkStack {
    /// Create a new network stack with the given device and configuration
    pub fn new(
        mut device: SmoltcpVirtioNetDevice,
        ip_addr: [u8; 4],
        gateway: [u8; 4],
    ) -> Self {
        let mac = device.mac_address();
        let ethernet_addr = EthernetAddress::from_bytes(&mac);

        // Create interface configuration
        let config = Config::new(ethernet_addr.into());

        // Create interface
        let mut interface = Interface::new(config, &mut device, Instant::ZERO);

        // Configure IP address
        let ip_cidr = IpCidr::new(
            IpAddress::v4(ip_addr[0], ip_addr[1], ip_addr[2], ip_addr[3]),
            24, // /24 subnet mask
        );
        interface.update_ip_addrs(|addrs| {
            addrs.push(ip_cidr).unwrap();
        });

        // Configure default gateway
        interface.routes_mut().add_default_ipv4_route(
            Ipv4Address::new(gateway[0], gateway[1], gateway[2], gateway[3])
        ).ok();

        // Create socket set
        let sockets = SocketSet::new(Vec::new());

        NetworkStack {
            interface,
            sockets,
            device,
        }
    }

    /// Get current time as smoltcp Instant
    fn now() -> Instant {
        let millis = timer::get_time_ms();
        Instant::from_millis(millis as i64)
    }

    /// Poll the network stack (process packets, update timers, etc.)
    pub fn poll(&mut self) {
        let timestamp = Self::now();
        self.interface.poll(timestamp, &mut self.device, &mut self.sockets);
    }

    /// Add receive buffers to the underlying VirtIO device
    pub fn add_receive_buffers(&mut self, count: usize) -> Result<(), &'static str> {
        self.device.inner_mut().add_receive_buffers(count)
    }

    /// Create a new TCP socket
    pub fn create_tcp_socket(&mut self) -> SocketHandle {
        // Create TCP socket with buffers large enough for HTTP responses
        // Increased to 32KB to handle larger web pages and images
        let tcp_rx_buffer = tcp::SocketBuffer::new(vec![0; 32768]);
        let tcp_tx_buffer = tcp::SocketBuffer::new(vec![0; 8192]);
        let tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);

        self.sockets.add(tcp_socket)
    }

    /// Create a new UDP socket
    pub fn create_udp_socket(&mut self) -> SocketHandle {
        // Create UDP socket with reasonable buffers
        let udp_rx_buffer = udp::PacketBuffer::new(
            vec![udp::PacketMetadata::EMPTY; 8],
            vec![0; 4096]
        );
        let udp_tx_buffer = udp::PacketBuffer::new(
            vec![udp::PacketMetadata::EMPTY; 8],
            vec![0; 4096]
        );
        let udp_socket = udp::Socket::new(udp_rx_buffer, udp_tx_buffer);

        self.sockets.add(udp_socket)
    }

    /// Create a new ICMP socket
    pub fn create_icmp_socket(&mut self) -> SocketHandle {
        // Create ICMP socket with reasonable buffers
        let icmp_rx_buffer = icmp::PacketBuffer::new(
            vec![icmp::PacketMetadata::EMPTY; 8],
            vec![0; 256]
        );
        let icmp_tx_buffer = icmp::PacketBuffer::new(
            vec![icmp::PacketMetadata::EMPTY; 8],
            vec![0; 256]
        );
        let icmp_socket = icmp::Socket::new(icmp_rx_buffer, icmp_tx_buffer);

        self.sockets.add(icmp_socket)
    }

    /// Access a TCP socket with a closure
    pub fn with_tcp_socket<F, R>(&mut self, handle: SocketHandle, f: F) -> R
    where
        F: FnOnce(&mut tcp::Socket) -> R,
    {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        f(socket)
    }

    /// Access a UDP socket with a closure
    pub fn with_udp_socket<F, R>(&mut self, handle: SocketHandle, f: F) -> R
    where
        F: FnOnce(&mut udp::Socket) -> R,
    {
        let socket = self.sockets.get_mut::<udp::Socket>(handle);
        f(socket)
    }

    /// Access an ICMP socket with a closure
    pub fn with_icmp_socket<F, R>(&mut self, handle: SocketHandle, f: F) -> R
    where
        F: FnOnce(&mut icmp::Socket) -> R,
    {
        let socket = self.sockets.get_mut::<icmp::Socket>(handle);
        f(socket)
    }

    /// Remove a socket from the set
    pub fn remove_socket(&mut self, handle: SocketHandle) {
        self.sockets.remove(handle);
    }

    /// Get the interface's IP address
    pub fn ip_address(&self) -> Option<[u8; 4]> {
        self.interface.ip_addrs().iter().find_map(|addr| {
            if let IpAddress::Ipv4(ipv4) = addr.address() {
                // Ipv4Address::octets() returns [u8; 4]
                Some(ipv4.octets())
            } else {
                None
            }
        })
    }

    /// Get the MAC address
    pub fn mac_address(&self) -> [u8; 6] {
        self.device.mac_address()
    }

    /// Get the interface for socket operations
    pub fn interface_mut(&mut self) -> &mut Interface {
        &mut self.interface
    }

    /// Connect a TCP socket (helper that manages Context)
    pub fn tcp_connect(
        &mut self,
        handle: SocketHandle,
        remote_endpoint: smoltcp::wire::IpEndpoint,
        local_port: u16,
    ) -> Result<(), smoltcp::socket::tcp::ConnectError> {
        let mut cx = self.interface.context();

        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.connect(&mut cx, remote_endpoint, local_port)
    }
}
