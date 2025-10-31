# rOSt - Rust ARM64 Operating System

**Last Updated:** 2025-11-01 (Added full networking support with VirtIO-Net!)

## What Works

✅ **Full GUI Desktop Environment**
- Tiling window manager with menu bar
- Multiple windows (Terminal, Text Editor, File Explorer, About dialog)
- Click to focus, close button on each window
- Auto-tiling layout (1-4 windows supported)

✅ **Text Editor**
- Full-featured text editor with syntax highlighting support
- Mouse selection (click and drag to select)
- Keyboard shortcuts: Ctrl+S (save), Ctrl+A (select all), Ctrl+C/X/V (copy/cut/paste)
- Undo/Redo support (Ctrl+Z/Ctrl+Y)
- Line numbers in gutter
- Arrow key navigation with Shift selection
- Open files from filesystem, save back to disk

✅ **File Explorer**
- Visual file browser with icons and file sizes
- Single-click to select, double-click to open in editor
- Keyboard navigation: Arrow keys to navigate, Enter to open
- Toolbar buttons: Refresh, New File, Delete, Rename
- Scrolling support for large file lists
- Hardware-independent double-click detection (500ms)

✅ **Full Persistent Filesystem with Interactive Shell**
- Create, read, write, delete, rename files that survive reboots
- Interactive shell: `ls`, `cat`, `create`, `rm`, `rename/mv`, `write`, `clear`, `help`
- Custom SimpleFS filesystem (32 files max, up to 10MB disk)

✅ **ARM Generic Timer**
- Hardware-independent microsecond-precision timing
- Uses ARM Generic Timer (CNTPCT_EL0, CNTFRQ_EL0)
- Enables consistent double-click detection across different CPU speeds

✅ **Real-Time Clock (RTC)**
- PL031 RTC driver for reading system time
- Displays current time (HH:MM) in menu bar (top right)
- Auto-updates every minute
- Timezone support (default: CET/UTC+1, configurable in rtc.rs)
- Unix timestamp conversion to human-readable date/time

✅ **Networking (VirtIO-Net)**
- Full VirtIO 1.0 network device driver with modern virtio features
- Complete network protocol stack: Ethernet, ARP, IPv4, ICMP
- Ping support - test connectivity to external hosts (e.g., `ping 8.8.8.8`)
- ARP request/reply handling for MAC address resolution
- Packet transmission and reception working via QEMU user-mode networking
- Network commands: `ping <ip>`, `ifconfig`, `arp`
- Configuration: IP 10.0.2.15, Gateway 10.0.2.2, MAC 52:54:00:12:34:56

✅ **VirtIO Input**
- Keyboard and mouse input via VirtIO devices
- Events flow from QEMU window to OS
- Full keyboard support with modifiers (Ctrl, Shift, Alt)

✅ **VirtIO Block Storage**
- Sector read/write operations
- Persistent storage using modern VirtIO 1.0

✅ **VirtIO GPU with Hardware Cursor**
- Full VirtIO GPU driver (1280x800 resolution)
- Hardware-accelerated cursor with dedicated cursor queue
- Smooth cursor movement with accurate click detection

## Quick Start

### Build
```bash
cargo build --release --target aarch64-unknown-uefi --bin uefi_boot
```

### Create Persistent Disk (first time only)
```bash
qemu-img create -f raw test_disk.img 10M
```

### Run
```bash
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
  -nodefaults \
  -M virt \
  -cpu cortex-a57 \
  -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci \
  -display cocoa \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0,disable-legacy=on,disable-modern=off \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio
```

**IMPORTANT:**
- Click on QEMU graphical window (not terminal) for input
- Device order matters: test_disk.img before uefi_disk for persistence

## Using the GUI

**Menu Bar (top of screen):**
- **Terminal** - Opens a new terminal window with interactive shell
- **Editor** - Opens a new blank text editor
- **Files** - Opens file explorer to browse/manage files
- **About** - Shows OS information

**Window Controls:**
- Click window to focus
- Click red X button to close window
- Windows auto-tile (1-4 windows supported)

**File Explorer:**
- Click file to select (blue highlight)
- Double-click file to open in editor
- Arrow keys to navigate, Enter to open
- Toolbar buttons: Refresh, New File, Delete, Rename (when file selected)

**Text Editor:**
- Click and drag to select text
- Arrow keys to navigate
- Shift+Arrow keys to select
- Ctrl+S: Save, Ctrl+A: Select All
- Ctrl+C/X/V: Copy/Cut/Paste
- Ctrl+Z/Y: Undo/Redo

## Shell Commands (Terminal Window)

```
help                    - Show available commands
ls                      - List files
cat <filename>          - Show file contents
create <name> <size>    - Create a file (size in bytes)
write <file> <text>     - Write text to file
rm <filename>           - Delete a file
rename <old> <new>      - Rename a file (or use 'mv')
clear                   - Clear screen
ping <ip>               - Ping an IP address (e.g., ping 8.8.8.8)
ifconfig                - Show network configuration
arp                     - Show ARP cache
```

## Architecture Overview

### Memory Map
- **RAM:** 0x40000000+
- **UART:** 0x09000000
- **PL031 RTC:** 0x09010000
- **PCI ECAM:** 0x4010000000
- **PCI MMIO:** 0x10000000
- **VirtIO Virtqueues:** 0x50000000+

### VirtIO Device Layout
- **GPU (0:0:0):** BAR=0x10100000, Controlq=0x50000000, Cursorq=0x50010000
- **Keyboard (0:1:0):** BAR=0x10200000, Virtqueue=0x50020000
- **Mouse (0:2:0):** BAR=0x10300000, Virtqueue=0x50030000
- **Block (0:3:0):** BAR=0x10400000, Virtqueue=0x50040000
- **Network (0:4:0):** BAR=0x10500000, Receiveq=0x50050000, Transmitq=0x50060000

**Critical:** Each device needs unique BAR and virtqueue addresses to avoid conflicts.

### Filesystem Layout (SimpleFS)
- **Sector 0:** Superblock (magic: 0x524F5354 "ROST")
- **Sectors 1-2:** File table (32 entries × 20 bytes)
- **Sectors 3-10:** Reserved
- **Sectors 11+:** File data

## Core Files

### GUI & Window Management
- `src/kernel/window_manager.rs` - Tiling window manager with menu bar
- `src/kernel/editor.rs` - Full-featured text editor with mouse selection
- `src/kernel/file_explorer.rs` - Visual file browser with keyboard/mouse navigation
- `src/kernel/console.rs` - Multi-instance terminal/console support
- `src/kernel/framebuffer.rs` - Double-buffered rendering system

### VirtIO Drivers
- `src/kernel/virtio_gpu.rs` - VirtIO GPU driver with hardware cursor
- `src/kernel/virtio_input.rs` - Keyboard/mouse input driver (evdev codes)
- `src/kernel/virtio_blk.rs` - Block device driver
- `src/kernel/virtio_net.rs` - VirtIO network device driver (modern VirtIO 1.0)

### Networking
- `src/kernel/network.rs` - Network protocol stack (Ethernet, ARP, IPv4, ICMP)

### Filesystem & Storage
- `src/kernel/filesystem.rs` - SimpleFS implementation
- `src/kernel/shell.rs` - Interactive shell for terminal windows

### System
- `src/kernel/timer.rs` - ARM Generic Timer for hardware-independent timing
- `src/kernel/rtc.rs` - PL031 Real-Time Clock driver
- `src/kernel/usb_hid.rs` - Input event handling and routing
- `src/kernel/dtb.rs` - Device Tree parser
- `src/kernel/pci.rs` - PCI configuration
- `src/kernel/mod.rs` - Kernel init and main loop

## Critical Gotchas

### VirtIO Devices
1. **64-bit BARs:** Must program both BAR4 and BAR5 registers
2. **Memory conflicts:** Each device needs unique BAR and virtqueue addresses
3. **Modern only:** Use `disable-legacy=on` for block devices
4. **Device order:** Persistent disk must be first VirtIO block device

### Filesystem
1. **Stack overflow:** Never allocate 512-byte buffers on stack - use static buffers
2. **Packed structs:** Use `ptr::read_volatile()` and `ptr::write_volatile()` for all field access
3. **File table:** Spans 2 sectors (640 bytes > 512 bytes)
4. **Test sectors:** VirtIO tests must not write to sectors 0-10 (filesystem reserved)

### Shell
1. **Static storage:** Block devices stored in static `BLOCK_DEVICES` to avoid dangling pointers
2. **Device index:** Shell stores device index, not pointer

## Development History

### Major Milestones
1. **VirtIO Input** - 64-bit BAR programming enabled keyboard/mouse
2. **VirtIO Block** - Modern VirtIO device with 3-descriptor chain
3. **SimpleFS** - Custom filesystem with CRUD operations
4. **File Persistence** - Fixed sector collision (test was overwriting file table)
5. **Interactive Shell** - UART-based command interface
6. **GUI & Window Manager** - Tiling window manager with menu bar and multiple window types
7. **Text Editor** - Full-featured editor with mouse selection, undo/redo, clipboard
8. **File Explorer** - Visual file browser with keyboard/mouse navigation
9. **ARM Generic Timer** - Hardware-independent timing for double-click detection

### Key Fixes
- **File persistence:** VirtIO test was overwriting sector 1 (file table) - moved to sector 1000
- **File table overflow:** 640 bytes doesn't fit in 512-byte sector - now uses 2 sectors
- **Memory conflicts:** Block device initially used same addresses as keyboard - allocated unique addresses
- **Stack overflow:** 512-byte buffers on stack caused hangs - switched to static buffers
- **Legacy VirtIO:** Legacy devices (0x1001) hang - now only uses modern (0x1042)
- **Double-click timing:** Frame counter was hardware-dependent (7220 frames/sec!) - switched to ARM Generic Timer for consistent 500ms threshold

## Resources

### Documentation
- [VirtIO 1.0 Spec](https://docs.oasis-open.org/virtio/virtio/v1.0/virtio-v1.0.html)
- [Stephen Brennan's VirtIO Block Guide](https://brennan.io/2020/03/22/sos-block-device/)
- [OSDev Wiki](https://wiki.osdev.org/)

### Debugging Tips
When stuck:
1. Check CLAUDE.md for known issues
2. Search official docs (VirtIO spec, Linux kernel source)
3. Search OSDev forums, Stack Overflow
4. Don't spend hours guessing - search first!

---

**🚨 Keep this file updated after major changes! 🚨**

**🚨 Never credit claude when making a commit.🚨**

