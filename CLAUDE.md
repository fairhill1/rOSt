# rOSt - Rust ARM64 Operating System

**Last Updated:** 2025-10-31

## What Works

✅ **Full Persistent Filesystem with Interactive Shell**
- Create, read, write, delete files that survive reboots
- Interactive shell: `ls`, `cat`, `create`, `rm`, `write`, `clear`, `help`
- Custom SimpleFS filesystem (32 files max, up to 10MB disk)

✅ **VirtIO Input**
- Keyboard and mouse input via VirtIO devices
- Events flow from QEMU window to OS

✅ **VirtIO Block Storage**
- Sector read/write operations
- Persistent storage using modern VirtIO 1.0

✅ **Graphics & Display**
- UEFI GOP framebuffer (640x480)
- Mouse cursor rendering

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
  -device ramfb \
  -display cocoa \
  -device virtio-keyboard-pci \
  -device virtio-mouse-pci \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio
```

**IMPORTANT:**
- Click on QEMU graphical window (not terminal) for input
- Device order matters: test_disk.img before uefi_disk for persistence

## Shell Commands

```
help                    - Show available commands
ls                      - List files
cat <filename>          - Show file contents
create <name> <size>    - Create a file (size in bytes)
write <file> <text>     - Write text to file
rm <filename>           - Delete a file
clear                   - Clear screen
```

## Architecture Overview

### Memory Map
- **RAM:** 0x40000000+
- **PCI ECAM:** 0x4010000000
- **PCI MMIO:** 0x10000000
- **VirtIO Virtqueues:** 0x50000000+

### VirtIO Device Layout
- **Keyboard (0:1:0):** BAR=0x10100000, Virtqueue=0x50000000
- **Mouse (0:2:0):** BAR=0x10200000, Virtqueue=0x50010000
- **Block (0:3:0):** BAR=0x10300000, Virtqueue=0x50020000

**Critical:** Each device needs unique BAR and virtqueue addresses to avoid conflicts.

### Filesystem Layout (SimpleFS)
- **Sector 0:** Superblock (magic: 0x524F5354 "ROST")
- **Sectors 1-2:** File table (32 entries × 20 bytes)
- **Sectors 3-10:** Reserved
- **Sectors 11+:** File data

## Core Files

- `src/kernel/virtio_input.rs` - Keyboard/mouse input driver
- `src/kernel/virtio_blk.rs` - Block device driver
- `src/kernel/filesystem.rs` - SimpleFS implementation
- `src/kernel/shell.rs` - Interactive shell
- `src/kernel/dtb.rs` - Device Tree parser
- `src/kernel/pci.rs` - PCI configuration
- `src/kernel/mod.rs` - Kernel init

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

### Key Fixes
- **File persistence:** VirtIO test was overwriting sector 1 (file table) - moved to sector 1000
- **File table overflow:** 640 bytes doesn't fit in 512-byte sector - now uses 2 sectors
- **Memory conflicts:** Block device initially used same addresses as keyboard - allocated unique addresses
- **Stack overflow:** 512-byte buffers on stack caused hangs - switched to static buffers
- **Legacy VirtIO:** Legacy devices (0x1001) hang - now only uses modern (0x1042)

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

