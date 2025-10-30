# XHCI Real Implementation Plan for ARM64 USB Keyboard Input

## Overview
This document outlines the step-by-step plan to implement **real** USB keyboard input using proper XHCI (eXtensible Host Controller Interface) programming on ARM64/QEMU.

## Current Status
- ✅ XHCI controller detection working
- ✅ PCI configuration and BAR mapping complete  
- ✅ Capability register parsing functional
- ❌ **Need to implement**: Real XHCI ring management and USB enumeration

## Implementation Plan

### Phase 1: XHCI Ring Buffer Foundation
**Goal**: Set up proper command and event rings for XHCI communication

#### Step 1.1: Memory Management for Rings
```rust
// Allocate physically contiguous memory for:
- Command Ring (256 TRBs = 4KB)
- Event Ring (256 TRBs = 4KB)  
- Event Ring Segment Table (16 bytes)
- Device Context Base Address Array (256 * 8 bytes = 2KB)
```

#### Step 1.2: TRB (Transfer Request Block) Management
```rust
pub struct XhciTrb {
    parameter: u64,     // TRB-specific parameter
    status: u32,        // Status and length fields
    control: u32,       // Control field with TRB type
}
```

#### Step 1.3: Ring Buffer Logic
- Implement cycle bit management for ring ownership
- Handle ring wraparound with Link TRBs
- Enqueue/dequeue pointer management

### Phase 2: Command Ring Setup and Operation
**Goal**: Send commands to XHCI controller and receive responses

#### Step 2.1: Command Ring Initialization
```rust
fn setup_command_ring() {
    // 1. Allocate command ring memory
    // 2. Initialize all TRBs to zero
    // 3. Set up Link TRB for ring wraparound
    // 4. Write CRCR register (64-bit write required on ARM64)
}
```

#### Step 2.2: Command Ring Control Register (CRCR) Programming
```rust
// Critical ARM64 requirement: Must write all 64 bits
let crcr_value = command_ring_dma_addr | CRCR_RCS | CRCR_CA;
write_op_reg64(XHCI_OP_CRCR, crcr_value);
```

#### Step 2.3: Basic Command Operations
- Implement `queue_command_trb()` 
- Implement `ring_command_doorbell()`
- Handle command completion events

### Phase 3: Event Ring Setup and Interrupt Handling
**Goal**: Receive events from XHCI controller including device connections

#### Step 3.1: Event Ring Segment Table Setup
```rust
fn setup_event_ring() {
    // 1. Allocate Event Ring Segment Table (ERST)
    // 2. Set up single segment pointing to event ring
    // 3. Configure runtime registers for interrupter 0
}
```

#### Step 3.2: Runtime Register Programming
```rust
// Runtime registers for primary interrupter
runtime_regs.ir[0].erst_size = 1;
runtime_regs.ir[0].erst_base = erst_dma_addr;
runtime_regs.ir[0].erst_dequeue = event_ring_dma_addr;
```

#### Step 3.3: Event Processing Loop
- Implement `poll_event_ring()`
- Handle different event types (Command Completion, Port Status Change, Transfer)
- Update Event Ring Dequeue Pointer

### Phase 4: USB Device Enumeration
**Goal**: Detect and configure USB devices (keyboards)

#### Step 4.1: Device Context Base Address Array (DCBAA)
```rust
fn setup_dcbaa() {
    // Allocate array of pointers to device contexts
    // Write DCBAAP operational register
}
```

#### Step 4.2: Slot Management Commands  
```rust
// Command sequence for device enumeration:
1. Enable Slot Command -> Get Slot ID
2. Address Device Command -> Assign USB address  
3. Configure Endpoint Command -> Set up endpoints
```

#### Step 4.3: Port Status Monitoring
- Detect device connection events
- Initiate enumeration for connected devices
- Identify HID class devices (keyboards)

### Phase 5: HID Interrupt Transfer Implementation
**Goal**: Set up periodic transfers to receive keyboard input

#### Step 5.1: Transfer Ring Setup
```rust
fn setup_transfer_ring(slot_id: u8, endpoint_id: u8) {
    // Create transfer ring for keyboard interrupt endpoint
    // Configure endpoint context with transfer ring address
}
```

#### Step 5.2: USB HID Descriptor Parsing
- Get Device Descriptor
- Get Configuration Descriptor  
- Get HID Report Descriptor
- Identify keyboard input format

#### Step 5.3: Interrupt Transfer Scheduling
```rust
fn schedule_keyboard_transfers() {
    // Queue interrupt IN transfers for keyboard endpoint
    // Set up periodic polling (typically every 8ms)
    // Process HID reports when transfers complete
}
```

### Phase 6: Real Keyboard Input Processing
**Goal**: Convert HID reports to input events

#### Step 6.1: HID Report Parsing
```rust
fn process_keyboard_report(data: &[u8]) -> Option<InputEvent> {
    // Parse standard USB HID keyboard report:
    // Byte 0: Modifier keys (Ctrl, Alt, Shift, etc.)
    // Byte 1: Reserved
    // Bytes 2-7: Key scan codes (up to 6 simultaneous)
}
```

#### Step 6.2: Input Event Generation
- Convert HID scan codes to InputEvent enum
- Handle key press and release detection
- Queue events for GUI consumption

#### Step 6.3: Integration with Existing System
- Connect to current `usb_hid::InputEvent` system
- Ensure events are properly queued and processed
- Remove simulation code, use only real input

## Technical Constraints and Considerations

### ARM64-Specific Requirements
1. **64-bit Register Writes**: CRCR must be written as single 64-bit operation
2. **Memory Alignment**: All ring buffers must be 64-byte aligned
3. **Cache Coherency**: Ensure proper cache management for DMA buffers
4. **Endianness**: Handle little-endian requirements for XHCI structures

### QEMU Compatibility
1. **qemu-xhci Device**: Use `-device qemu-xhci` for testing
2. **USB Keyboard Device**: Add with `-device usb-kbd` 
3. **Interrupt Simulation**: QEMU properly simulates XHCI interrupt behavior
4. **Register Implementation**: QEMU implements full XHCI register set

### Memory Management
1. **Physical Contiguity**: All ring buffers need physically contiguous memory
2. **DMA Addresses**: Use physical addresses for all XHCI structures  
3. **Memory Barriers**: Proper synchronization for shared memory structures
4. **Alignment Requirements**: 64-byte alignment for optimal performance

## Success Criteria
1. **Real Keyboard Detection**: Detect actual USB keyboard devices
2. **Key Press Events**: Generate InputEvent for real key presses
3. **No Simulation**: Remove all simulated/fake input code
4. **GUI Ready**: Events work with future GUI framework
5. **Stable Operation**: Handle connect/disconnect gracefully

## Testing Plan
1. **Unit Tests**: Test individual ring operations
2. **Command Tests**: Verify command/response cycle  
3. **Integration Tests**: Full enumeration sequence
4. **Real Hardware**: Test with actual keyboard input
5. **Performance Tests**: Ensure low-latency input processing

## Rollback Strategy
If implementation becomes too complex:
1. Keep current working detection code
2. Implement simplified HID polling
3. Focus on minimal viable keyboard input
4. Defer advanced features to later iterations

---

**Status**: Ready to begin Phase 1 implementation
**Next Step**: Implement memory management and ring buffer allocation