# rOSt - Rust ARM64 Operating System

Production-grade ARM64 OS written in Rust. **Last Updated:** 2025-11-02

## Features

**GUI:** Tiling window manager, text editor (syntax highlighting, undo/redo), file explorer, web browser (HTML, BMP images, smoltcp TCP/IP), image viewer
**Filesystem:** SimpleFS (32 files, 10MB, persistent across reboots)
**Hardware:** VirtIO GPU/Input/Block/Net, ARM Generic Timer, PL031 RTC
**Networking:** smoltcp 0.12 stack, DNS, HTTP/1.0 client, ping, download command
**Shell:** Interactive terminal with filesystem and network commands

## Quick Start

```bash
# Build
cargo build --release --target aarch64-unknown-uefi --bin uefi_boot

# Create disk (first time)
qemu-img create -f raw test_disk.img 10M

# Run
cp target/aarch64-unknown-uefi/release/uefi_boot.efi uefi_disk/EFI/BOOT/BOOTAA64.EFI && \
qemu-system-aarch64 \
  -nodefaults -M virt -cpu cortex-a57 -m 1G \
  -bios /opt/homebrew/share/qemu/edk2-aarch64-code.fd \
  -device virtio-gpu-pci -display cocoa \
  -device virtio-keyboard-pci -device virtio-mouse-pci \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0,disable-legacy=on,disable-modern=off \
  -drive file=test_disk.img,if=none,format=raw,id=hd0 \
  -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off \
  -drive format=raw,file=fat:rw:uefi_disk \
  -serial stdio
```

**Note:** Click QEMU window (not terminal) for input. Device order matters: test_disk.img must come before uefi_disk.

## Shell Commands

```
ls, cat <file>, create <name> <size>, write <file> <text>, rm <file>, rename <old> <new>
ping <ip>, nslookup <domain>, http <url>, download <url>, ifconfig, arp
```

## Architecture

### Memory Map
- RAM: 0x40000000+, UART: 0x09000000, RTC: 0x09010000
- PCI ECAM: 0x4010000000, PCI MMIO: 0x10000000, VirtIO queues: 0x50000000+

### VirtIO Layout
- GPU (0:0:0): BAR 0x10100000, queues 0x50000000/0x50010000
- Keyboard/Mouse/Block/Net: BARs 0x102/0x103/0x104/0x105, queues 0x50020000/30/40/50+60

### Filesystem (SimpleFS)
- Sector 0: Superblock (magic 0x524F5354)
- Sectors 1-2: File table (32 entries × 20 bytes)
- Sectors 11+: File data

### Codebase Structure
```
src/kernel/          - Core (mod, memory, dtb)
  drivers/           - PCI, timer, RTC, input_events
    virtio/          - gpu, input, blk, net
src/system/
  fs/                - filesystem (SimpleFS)
  net/               - network, dns, tcp (deprecated, using smoltcp)
src/gui/             - framebuffer, window_manager, html_parser, bmp_decoder, clipboard
  widgets/           - browser, editor, console, file_explorer, text_input, image_viewer
src/apps/            - shell, snake
```

## Critical Gotchas

**VirtIO:**
- 64-bit BARs need both BAR4 and BAR5 programmed
- Each device needs unique BAR + virtqueue addresses
- Use `disable-legacy=on` for modern devices
- Persistent disk must be first VirtIO block device

**Filesystem:**
- Never allocate 512-byte buffers on stack (use static)
- Packed structs need `ptr::read_volatile()`/`write_volatile()`
- File table spans 2 sectors (640 bytes)
- Don't write to sectors 0-10 (filesystem reserved)

**Networking:**
- QEMU user-mode: 10.0.2.x range (guest: 10.0.2.15, gateway: 10.0.2.2)
- HTTP responses may arrive in multiple TCP segments
- Always replenish RX buffers after packet processing (buffer exhaustion!)
- Use ARM timer for delays (not nop loops)

**Module Paths:**
- After reorganization: `crate::kernel::drivers::virtio::net::*` (not `crate::kernel::virtio_net::*`)

**Colors:**
- BMP decoder outputs 0xAABBGGRR format
- Browser writes pixels directly, image viewer swaps R/B channels

## Key Wins

- **smoltcp migration:** Production TCP/IP stack, 91% code reduction (834→76 lines)
- **Buffer exhaustion fix:** Auto-replenish RX buffers after each packet
- **ARM Generic Timer:** Hardware-independent timing (no CPU-dependent delays)
- **Modular architecture:** Clean separation (drivers/system/gui/apps)

---

**🚨 Update after major changes! 🚨**
