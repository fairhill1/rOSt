# 🚨 CRITICAL: UPDATE THIS FILE AFTER EVERY CHANGE 🚨

**MANDATORY:** Every time you make a code change, test something, or discover something new, you MUST update the relevant sections below. This file becomes stale in minutes if not maintained!

---

# Rust OS Development Status - ARM64 OS on QEMU

## Current Date
2025-10-31

## Current Goal
✅ **COMPLETE!** Interactive shell with persistent file storage working!

## Previous Goals (✅ Completed)
- ✅ Get real keyboard and mouse input working from QEMU graphical window using VirtIO input devices
- ✅ Implement VirtIO block device driver for disk read/write (read_sector, write_sector)
- ✅ Implement custom filesystem (SimpleFS) with full CRUD operations
- ✅ Build interactive shell for file operations
- ✅ Fix file persistence across reboots

## Current Status: ✅ **Full Persistent Filesystem with Interactive Shell Working!**

### ✅ SimpleFS Filesystem - FULLY WORKING WITH PERSISTENCE! 📁💾
- **ALL FILE OPERATIONS WORKING** - Complete filesystem implementation!
- **✅ FILE PERSISTENCE WORKING** - Files survive across reboots!
- **Format & Mount** - Initialize new disks and load existing filesystems
- **Create Files** - Allocate sectors and update file table (tested: 100B, 2KB, 512B files)
- **Delete Files** - Mark entries as free and update metadata
- **Write Data** - Sector-by-sector writes with proper buffer handling
- **Read Data** - Sector-by-sector reads with size validation
- **List Files** - Enumerate all stored files with size/location info
- **Interactive Shell** - Type commands to create, write, read, delete files
- All tests passing: format, mount, create, delete, read, write, verification, persistence

### ✅ VirtIO Block Device - WORKING! 💾
- **SECTOR READ/WRITE WORKING** - Full disk I/O capabilities!
- VirtIO block device (PCI 0:3:0) initialized successfully
- Virtqueue setup complete (descriptor, available, used rings)
- 3-descriptor chain working (header → data → status)
- Device found and initialized using modern VirtIO 1.0 protocol
- Tested with 10MB test disk image
- Both read_sector() and write_sector() functions fully operational

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
- **PCI ECAM base:** 0x4010000000 (from DTB)
- **PCI MMIO base:** 0x10000000 (from DTB)

**CRITICAL: VirtIO Device Memory Allocation**
To avoid conflicts, devices MUST use non-overlapping addresses:

**BAR Addresses (MMIO space starting at 0x10000000):**
- VirtIO Keyboard (0:1:0): **0x10100000** (MMIO base + 0x100000)
- VirtIO Mouse (0:2:0): **0x10200000** (MMIO base + 0x200000)
- VirtIO Block (0:3:0): **0x10300000** (MMIO base + 0x300000) ← MUST NOT use 0x100000!

**Virtqueue Memory (DMA-accessible RAM starting at 0x50000000):**
- VirtIO Keyboard: **0x50000000** (size ~0x10000)
- VirtIO Mouse: **0x50010000** (size ~0x10000)
- VirtIO Block: **0x50020000** (size ~varies) ← MUST NOT use 0x50000000!

**Why This Matters:** Each device needs its own BAR and virtqueue memory. Reusing addresses causes one device to overwrite another's configuration, breaking functionality. The keyboard stopped working when the block device was added because it initially used the same addresses.

### VirtIO Input Devices Found (Both Working!)
1. **PCI 0:1:0** - ✅ Keyboard - Working!
2. **PCI 0:2:0** - ✅ Mouse - Working!

### Recent Fixes (2025-10-31)

#### File Persistence Fixes
10. **🔥 CRITICAL: File Persistence Not Working - VirtIO Test Overwriting File Table** 🔥
   - **Problem**: Files created in shell didn't survive reboots - file table corrupted on every boot
   - **Symptom**: `file_count > 0` but `list_files()` returned empty; file entries showed garbage data
   - **Root Cause**: VirtIO block device test wrote pattern data to **sector 1** on EVERY boot
     - Filesystem stores file table at sectors 1-2
     - Test overwrote file table with `00 01 02 03...` pattern before filesystem mounted
   - **Solution**: Changed test from sector 1 → sector 1000 (high sector, no collision)
   - **Code**: `mod.rs:342` (changed `write_sector(1, ...)` → `write_sector(1000, ...)`)
   - **Impact**: Without this fix, file persistence is completely broken despite proper write/read

11. **File Table Buffer Overflow - 640 Bytes into 512-Byte Sector**
   - **Problem**: File table (32 entries × 20 bytes = 640 bytes) doesn't fit in one 512-byte sector
   - **Symptom**: Only first ~25 file entries could be stored; remaining entries lost
   - **Solution**: Expanded file table to use **2 sectors** (sectors 1-2)
     - Updated format() to write across both sectors
     - Updated mount() to read from both sectors
     - Updated create_file() and delete_file() to write both sectors
   - **Code**: `filesystem.rs:18` (added `FILE_TABLE_SECTORS = 2`), multiple read/write loops
   - **Impact**: Now supports full 32-file capacity

#### Shell Implementation Fixes
12. **Dangling Pointer to Block Devices**
   - **Problem**: Shell stored raw pointer to block devices that went out of scope
   - **Symptom**: Use-after-free causing undefined behavior
   - **Solution**:
     - Created static `BLOCK_DEVICES` storage in mod.rs
     - Changed Shell to store device index instead of pointer
     - Shell methods access device via static reference using index
   - **Code**: `mod.rs:36` (static storage), `shell.rs:15` (device_index field)

#### VirtIO Block Device Fixes
9. **🔥 CRITICAL: Memory Conflicts Breaking Keyboard Input** 🔥
   - **Problem**: Block device allocated BAR and virtqueue at same addresses as keyboard
   - **Symptom**: Mouse worked, but keyboard completely non-functional after adding block device
   - **Root Cause**: Two conflicts:
     1. BAR address: Block device used `0x10100000` (same as keyboard's BAR)
     2. Virtqueue memory: Block device used `0x50000000` (same as keyboard's virtqueue)
   - **Solution**:
     - BAR: Changed block device from `0x10100000` → `0x10300000`
     - Virtqueue: Changed block device from `0x50000000` → `0x50020000`
   - **Code**: `virtio_blk.rs:158` (virtqueue), `virtio_blk.rs:379` (BAR)
   - **Memory Layout**:
     - BARs: Keyboard `0x10100000`, Mouse `0x10200000`, Block `0x10300000`
     - Virtqueues: Keyboard `0x50000000`, Mouse `0x50010000`, Block `0x50020000`
   - **Impact**: Keyboard input completely broken without this fix

#### Filesystem Implementation Fixes
7. **🔥 CRITICAL: Stack Overflow in create_file() and delete_file()** 🔥
   - **Problem**: 512-byte sector buffers allocated on stack caused stack overflow
   - **Symptom**: Functions would execute but never return - hung after printing last debug message
   - **Solution**: Use static buffers (`CREATE_BUFFER`, `TEMP_BUFFER`) instead of stack allocation
   - **Code**: `filesystem.rs:300-324` (create_file), `filesystem.rs:366-394` (delete_file)
   - **Impact**: Without this fix, file operations appear to work but system hangs

8. **Legacy VirtIO Block Device Causing "Zero Sized Buffers" Error**
   - **Problem**: System tried to initialize both modern (0x1042) and legacy (0x1001) block devices
   - **Symptom**: QEMU error "virtio: zero sized buffers are not allowed" after certain operations
   - **Root Cause**: Legacy VirtIO device driver has bugs, doesn't work properly
   - **Solution**: Skip legacy devices, only initialize modern VirtIO (device_id == 0x1042)
   - **Code**: `virtio_blk.rs:238-255`
   - **Impact**: All filesystem operations now work reliably

#### VirtIO Input Fixes
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
- `src/kernel/virtio_blk.rs` - **NEW!** VirtIO block device driver (read/write sectors)
- `src/kernel/filesystem.rs` - **NEW!** SimpleFS filesystem implementation (format, mount, create, delete, read, write)
- `src/kernel/shell.rs` - **NEW!** Interactive shell for file operations (ls, cat, create, rm, write, clear)
- `src/kernel/pci.rs` - PCI config space access (added `read_u8()`, `write_config_u32()`, `get_bar_size()`, `get_capabilities_ptr()`)
- `src/kernel/mod.rs` - Kernel init, static block device storage, filesystem tests

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
- ✅ Sector writes (tested with round-trip verification)
- ✅ Completion polling (busy-wait on used ring index)

## SimpleFS Filesystem Implementation (2025-10-31)

### Filesystem Design
Simple custom filesystem with superblock, file table, and data sectors:

**Disk Layout:**
- **Sector 0**: Superblock (512 bytes) - magic number, version, total sectors, file count
- **Sectors 1-2**: File table (1024 bytes) - up to 32 file entries (20 bytes each)
- **Sectors 3-10**: Reserved for future use
- **Sectors 11+**: Data blocks (file contents)

**File Entry Structure (20 bytes):**
- name: 8 bytes (null-terminated)
- start_sector: 2 bytes (starting sector number)
- size_sectors: 2 bytes (number of sectors allocated)
- size_bytes: 4 bytes (actual file size in bytes)
- flags: 1 byte (0x01 = used, 0x00 = free)
- reserved: 3 bytes (future use)

### Key Implementation Details

1. **Format Operation**
   - Creates empty superblock with magic number 0x524F5354 ("ROST")
   - Initializes empty file table with all entries marked as free
   - Sets next_free_sector to 10 (first data sector)

2. **Mount Operation**
   - Reads and validates superblock (checks magic number and version)
   - Loads file table into memory
   - Calculates next_free_sector by scanning used files

3. **Create File Operation**
   - Validates filename (1-8 characters)
   - Checks for duplicates
   - Allocates sectors based on size (rounds up to 512-byte boundaries)
   - Updates file table and superblock
   - Writes both to disk

4. **Delete File Operation**
   - Finds file entry in table
   - Marks entry as free (flags = 0x00)
   - Updates superblock file count
   - Writes updated table and superblock to disk
   - **NOTE**: Does NOT reclaim sectors (no defragmentation)

5. **Write File Operation**
   - Validates file exists and data fits in allocated space
   - Writes data sector-by-sector (512 bytes at a time)
   - Pads last sector with zeros if needed
   - Returns error if data exceeds allocated size

6. **Read File Operation**
   - Validates file exists and buffer is large enough
   - Reads data sector-by-sector
   - Returns actual bytes read (size_bytes, not padded sector size)

### Critical Gotchas
- **Stack Overflow**: NEVER allocate 512-byte buffers on stack - use static buffers!
- **Packed Structs**: Use `ptr::read_volatile()` and `ptr::write_volatile()` for all field access
- **No Defragmentation**: Deleted files don't reclaim sectors (simple implementation)
- **Fixed File Table**: Maximum 32 files (hard limit in sector 1)

### Test Results
All tests passing:
- ✅ Format disk
- ✅ Mount filesystem (0 files initially)
- ✅ Create files: hello (100B), test (2KB), data (512B)
- ✅ List files (shows all 3)
- ✅ Reject duplicate file creation
- ✅ Delete file (test)
- ✅ List after deletion (shows 2 remaining)
- ✅ Write data to files (37 bytes text, 400 bytes binary)
- ✅ Read data back (verification success)
- ✅ Binary data round-trip (all 400 bytes match)

### What's Next
- Build simple shell for interactive file operations
- Add directory support (subdirectories)
- Implement defragmentation for deleted files
- Add file metadata (timestamps, permissions)

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
qemu-img create -f raw test_disk.img 10M

# Deploy and Run WITH PERSISTENT FILESYSTEM (RECOMMENDED)
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
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
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
# - DEVICE ORDER MATTERS: test_disk.img MUST come BEFORE uefi_disk for persistence
#   - Device 0:3:0 = test_disk.img (persistent storage)
#   - Device 0:4:0 = uefi_disk (boot disk, rejected by OS)
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
