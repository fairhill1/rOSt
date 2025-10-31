# 🚨 CRITICAL: UPDATE THIS FILE AFTER EVERY CHANGE 🚨

**MANDATORY:** Every time you make a code change, test something, or discover something new, you MUST update the relevant sections below. This file becomes stale in minutes if not maintained!

---

# Rust OS Development Status - ARM64 OS on QEMU

## Current Date
2025-10-31

## Current Goal
Implement file I/O using VirtIO block device driver

## Previous Goals (✅ Completed)
- ✅ Get real keyboard and mouse input working from QEMU graphical window using VirtIO input devices
- ✅ Implement VirtIO block device driver for disk read/write

## Current Status: ✅ **VirtIO Block Device Working!**

### ✅ VirtIO Block Device - WORKING! 💾
- **SECTOR READ WORKING** - Successfully reading sectors from disk!
- VirtIO block device (PCI 0:3:0) initialized successfully
- Virtqueue setup complete (descriptor, available, used rings)
- 3-descriptor chain working (header → data → status)
- Device found and initialized using modern VirtIO 1.0 protocol
- Tested with 10MB test disk image
- Ready for filesystem implementation!

### ✅ VirtIO Input - WORKING! ⌨️🐭
- **MOUSE INPUT WORKING** - Mouse movement and button clicks detected!
- **KEYBOARD INPUT WORKING** - Key presses detected!
- Both VirtIO input devices (0:1:0 and 0:2:0) initialize successfully
- DTB parser successfully reads PCI ECAM base (0x4010000000) and MMIO base (0x10000000)
- 64-bit BAR programming works correctly (BAR4 + BAR5)
- Virtqueue memory in valid RAM at 0x50000000
- Events flowing properly from QEMU to OS
- Device reports "Device ready!" and actually IS ready
- PCI ECAM base bug fixed - PciDevice methods use correct ECAM base

### 🎯 Final Solution
**The root cause was 64-bit BARs!**
- VirtIO devices set the 64-bit BAR flag (bit 2 of BAR register)
- This means the BAR uses TWO consecutive 32-bit registers
- We were only programming BAR4 (offset 0x20, lower 32 bits)
- We needed to ALSO program BAR5 (offset 0x24, upper 32 bits)
- Once both were programmed, MMIO worked immediately and events flowed!

## Key Technical Details

### Memory Map
- **RAM starts at:** 0x40000000 on ARM virt
- **Virtqueues at:** 0x50000000 (in RAM, DMA accessible)
- **PCI ECAM base:** 0x4010000000 (from DTB)
- **PCI MMIO base:** 0x10000000 (from DTB)
- **Device 0:2:0 Common Config:** 0x10004000 (MMIO + BAR offset 0x4000)
- **Device 0:2:0 Notify:** 0x10007000 (MMIO + BAR offset 0x7000)

### VirtIO Input Devices Found (Both Working!)
1. **PCI 0:1:0** - ✅ Keyboard - Working!
2. **PCI 0:2:0** - ✅ Mouse - Working!

### Recent Fixes (2025-10-31)
1. **Virtqueue memory allocation** - Changed from 0x20000000 (invalid I/O space) to 0x50000000 (valid RAM)
   - This fixed `used_idx` from 0xFFFF → 0
2. **DTB parser depth tracking** - Fixed child nodes overwriting parent node properties
3. **BAR address calculation** - Add MMIO base to BAR offsets (BAR values are offsets, not absolute addresses)
4. **PCI ECAM base bug** - Fixed `PciDevice` methods (`get_bar_address`, `read_config_*`, etc.) to use stored `ecam_base` instead of hardcoded PCI_CONFIG_BASE
   - Added `ecam_base` field to `PciDevice` struct
   - All PCI config access now uses correct ECAM base from DTB
5. **Device config capability** - Added parsing of VIRTIO_PCI_CAP_DEVICE_CFG (type=4)
   - Device config found at 0x10006000
   - Implemented query methods for device name and event types
   - However, QEMU returns 0xFF for all queries (may not be implemented)
6. **🎉 64-BIT BAR PROGRAMMING - THE FIX THAT MADE IT WORK!** 🎉
   - VirtIO devices use 64-bit BARs (flag bit 2 set in BAR register)
   - 64-bit BARs span TWO consecutive 32-bit registers
   - Must program BOTH BAR4 (offset 0x20) AND BAR5 (offset 0x24)
   - Was only programming BAR4, leaving upper 32 bits at 0
   - Once both programmed correctly: MMIO works, events flow, **INPUT WORKS!**
   - Code location: `virtio_input.rs:247-249`

## Files Modified

### Core Files
- `src/kernel/dtb.rs` - DTB parser (reads device tree at 0x40000000)
- `src/kernel/virtio_input.rs` - VirtIO input driver
- `src/kernel/virtio_blk.rs` - **NEW!** VirtIO block device driver
- `src/kernel/pci.rs` - PCI config space access (added `read_u8()`, `write_config_u32()`, `get_bar_size()`, `get_capabilities_ptr()`)
- `src/kernel/mod.rs` - Kernel init (calls DTB parser, VirtIO input, and VirtIO block init)

## VirtIO Block Driver Implementation (2025-10-31)

### Key Implementation Details
Based on [Stephen Brennan's blog post](https://brennan.io/2020/03/22/sos-block-device/) and VirtIO 1.0 spec:

1. **Device Discovery & Initialization**
   - Scan PCI bus for vendor 0x1AF4, device 0x1042 (modern) or 0x1001 (legacy)
   - Parse VirtIO PCI capabilities to find common_cfg and notify registers
   - Standard VirtIO handshake: ACKNOWLEDGE → DRIVER → FEATURES_OK → DRIVER_OK

2. **Virtqueue Structure** (3-ring architecture)
   - **Descriptor table**: Array of buffer descriptors (addr, len, flags, next)
   - **Available ring**: Driver writes descriptor chains here
   - **Used ring**: Device writes completed descriptors here
   - Memory allocated at 0x50000000 (same region as VirtIO input, different offset)

3. **3-Descriptor Chain for Reads** (Critical insight from blog!)
   - **Desc 1**: Request header (16 bytes: type, reserved, sector) - READ-ONLY for device
   - **Desc 2**: Data buffer (512 bytes) - WRITE for device (on read requests)
   - **Desc 3**: Status byte (1 byte) - WRITE for device
   - All READ-ONLY descriptors MUST come before WRITE descriptors (VirtIO spec requirement)

4. **Memory Barriers**
   - Use ARM `dsb sy` instruction after critical operations
   - Ensures writes are visible to device before notification

5. **Critical Gotchas**
   - **MUST use modern VirtIO**: `virtio-blk-pci,disable-legacy=on`
   - Legacy device (0x1001) hangs on completion polling
   - Modern device (0x1042) works perfectly
   - **Packed struct fields**: Use `ptr::addr_of!()` and `ptr::addr_of_mut!()` to avoid UB
   - **BAR programming**: Same 64-bit BAR issue as input devices (program BAR4 + BAR5)

### What Works
- ✅ Device detection and initialization
- ✅ Virtqueue allocation and configuration
- ✅ Sector reads (tested with sector 0)
- ✅ Completion polling (busy-wait on used ring index)

### What's Next
- Add write_sector() function
- Implement simple filesystem (FAT or custom)
- Add file operations (open, read, write, close)
- Build simple shell for file interaction

## ✅ GOAL ACHIEVED - WORKING INPUT!

### What to do next
1. **Test keyboard input** - Type keys in QEMU window and verify output
2. **Test mouse buttons** - Click in QEMU window and verify button events
3. **Integrate with GUI** - Connect input events to your GUI/applications
4. **Celebrate!** 🎉 You have a working OS with keyboard and mouse input!

### Debugging Tips for Future Issues
**IMPORTANT: When stuck or unsure, USE WEB SEARCH!**
- Search for official docs (Linux kernel source, VirtIO spec, QEMU docs)
- Search for similar issues on OSDev forums, Stack Overflow
- Search for reference implementations
- Don't spend hours guessing - 5 minutes of searching can save hours of debugging!

### Build/Test Commands
```bash
# Build (must specify UEFI target explicitly)
cargo build --release --target aarch64-unknown-uefi --bin uefi_boot

# Create test disk image (10MB, only needed once)
dd if=/dev/zero of=test_disk.img bs=1M count=10

# Deploy and Run WITH BLOCK DEVICE (RECOMMENDED - includes all features)
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
  -nodefaults \
  -M virt \
  -cpu cortex-a57 \
  -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device ramfb \
  -display cocoa \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -drive format=raw,file=fat:rw:uefi_disk \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -serial stdio

# Deploy and Run WITHOUT BLOCK DEVICE (input only)
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
  -nodefaults \
  -M virt \
  -cpu cortex-a57 \
  -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device ramfb \
  -display cocoa \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio

# IMPORTANT NOTES:
# - Must focus on QEMU graphical window (not terminal) for input to be captured!
# - For block device: MUST use virtio-blk-pci with disable-legacy=on (not virtio-blk-device)
# - Legacy VirtIO (0x1001) hangs on reads; modern VirtIO (0x1042) works perfectly
```

## Known Issues (Resolved!)

### ✅ Issue: No input events - **FIXED!**
- **Status:** SOLVED - 64-bit BAR programming was the issue
- **Solution:** Program both BAR4 (lower 32 bits) AND BAR5 (upper 32 bits)
- **Root cause:** VirtIO devices use 64-bit BARs, we were only programming half
- **Code:** `virtio_input.rs:247-249`

### ✅ Issue: First VirtIO device fails - **FIXED!**
- **Status:** SOLVED - both devices now initialize
- **Solution:** Proper BAR sizing and programming for all devices
- **Note:** Device 0:1:0 was keyboard, 0:2:0 was mouse

### Issue: queue_notify_off = 0xFFFF
- **Status:** Handled via fallback
- **Impact:** None - we use queue number directly if notify_off is invalid
- **Code:** Works correctly with fallback logic

### Issue: Device config space returns 0xFF
- **Status:** Not critical - device works without it
- **Symptom:** All device config queries (name, event types) return size=0xFF
- **Impact:** None - events flow without device config queries
- **Note:** QEMU's VirtIO Input may not fully implement device config space

## Debug Output Patterns

### ✅ SUCCESS Output (WORKING!)
```
Found VirtIO input device at 0:1:0
  BAR4 size: 0x4000
  Allocating BAR4 at: 0x10100000
  BAR4 readback: 0x1010000C (flags=0xC)
  Found common config at 0x10100000
  Device ready!

Mouse moved: 1, -2
Mouse button 0 pressed
Key pressed: 0x1C (Enter)
```

### Key Indicators of Success:
- BAR4 readback shows valid address (not 0x00000000)
- Flag bits include 0xC (64-bit BAR)
- "Mouse moved" messages appear when you move mouse in QEMU window
- "Mouse button" messages appear when you click
- "Key pressed" messages appear when you type

### What Fixed It:
Programming BOTH BAR4 and BAR5 for 64-bit BARs:
```rust
config.write_u32(bus, device, function, 0x20, bar4_address as u32);      // Lower 32
config.write_u32(bus, device, function, 0x24, (bar4_address >> 32) as u32); // Upper 32
```

---

# 🚨 REMINDER: UPDATE THIS FILE AFTER EVERY CHANGE 🚨
