// smoltcp device wrapper for VirtIO-Net
// Implements smoltcp::phy::Device trait for VirtioNetDevice

use crate::kernel::drivers::virtio::net::VirtioNetDevice;
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use alloc::vec::Vec;

/// Wrapper for VirtIO-Net device that implements smoltcp Device trait
pub struct SmoltcpVirtioNetDevice {
    device: VirtioNetDevice,
    rx_buffer: [u8; 4096], // Increased from 2048 to handle larger packets
}

impl SmoltcpVirtioNetDevice {
    pub fn new(device: VirtioNetDevice) -> Self {
        SmoltcpVirtioNetDevice {
            device,
            rx_buffer: [0u8; 4096],
        }
    }

    /// Get a mutable reference to the underlying VirtIO device
    pub fn inner_mut(&mut self) -> &mut VirtioNetDevice {
        &mut self.device
    }

    /// Get MAC address from the device
    pub fn mac_address(&self) -> [u8; 6] {
        self.device.mac_address()
    }
}

/// RX token for receiving packets
pub struct VirtioRxToken {
    buffer: Vec<u8>,
}

impl RxToken for VirtioRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buffer[..])
    }
}

/// TX token for transmitting packets
pub struct VirtioTxToken<'a> {
    device: &'a mut VirtioNetDevice,
}

impl<'a> TxToken for VirtioTxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = Vec::new();
        buffer.resize(len, 0);
        let result = f(&mut buffer);

        // Transmit the packet
        let _ = self.device.transmit(&buffer);

        result
    }
}

impl Device for SmoltcpVirtioNetDevice {
    type RxToken<'a> = VirtioRxToken where Self: 'a;
    type TxToken<'a> = VirtioTxToken<'a> where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Try to receive a packet
        match self.device.receive(&mut self.rx_buffer) {
            Ok(len) if len > 0 => {
                // Copy received data to a Vec
                let mut buffer = Vec::new();
                buffer.extend_from_slice(&self.rx_buffer[..len]);

                // Return both RX and TX tokens (smoltcp may want to respond immediately)
                Some((
                    VirtioRxToken { buffer },
                    VirtioTxToken { device: &mut self.device },
                ))
            }
            _ => None,
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        // Always ready to transmit (we handle buffering in the device)
        Some(VirtioTxToken { device: &mut self.device })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514; // Standard Ethernet MTU
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}
