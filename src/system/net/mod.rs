// Network stack module

pub mod network;
pub mod dns;
pub mod smoltcp_device;
pub mod stack;
pub mod helpers;

// Re-export commonly used types
pub use smoltcp_device::SmoltcpVirtioNetDevice;
pub use stack::NetworkStack;
