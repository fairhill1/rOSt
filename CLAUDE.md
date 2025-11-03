# rOSt - Rust ARM64 Operating System

Production-grade ARM64 OS written in Rust. **Last Updated:** 2025-11-03

## Features

**GUI:** Tiling window manager, text editor (syntax highlighting, undo/redo), file explorer, web browser (async HTTP, progressive image loading, HTML/BMP/PNG), image viewer
**Browser:** Event-driven async I/O, image caching, auto-reflow, viewport clipping, width/height attribute parsing
**Filesystem:** SimpleFS (32 files, 10MB, persistent across reboots)
**Hardware:** VirtIO GPU/Input/Block/Net, ARM Generic Timer, PL031 RTC
**Networking:** smoltcp 0.12 stack, DNS, HTTP/1.0 client, ping, download command
**Kernel:** Preemptive multitasking, round-robin scheduler, ARM64 context switching
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
src/kernel/          - Core (mod, memory, dtb, scheduler, thread, interrupts)
  drivers/           - PCI, timer, RTC, input_events
    virtio/          - gpu, input, blk, net
src/system/
  fs/                - filesystem (SimpleFS)
  net/               - network, dns, helpers (smoltcp-based)
src/gui/             - framebuffer, window_manager, html_parser, bmp_decoder, png_decoder, clipboard
  widgets/           - browser (async), editor, console, file_explorer, text_input, image_viewer
src/apps/            - shell, snake
```

## Code Quality Standards (MANDATORY)

**BEFORE writing ANY code, you MUST follow these rules:**

### 1. No Magic Numbers
Define constants for ANY numeric value that isn't obviously 0, 1, or a direct API requirement.

```rust
// ❌ BAD
current_y += 6;
height: 25,
if screen_y < 4 { ... }

// ✅ GOOD
const BLOCK_BOTTOM_SPACING: usize = 6;
const PAGE_BOTTOM_PADDING: usize = 25;
const MIN_VISIBLE_PIXELS: isize = 4;

current_y += BLOCK_BOTTOM_SPACING;
height: PAGE_BOTTOM_PADDING,
if screen_y < MIN_VISIBLE_PIXELS { ... }
```

### 2. No Code Duplication
If you write the same logic twice, extract a shared function IMMEDIATELY.

- Three reflow paths? ONE shared function.
- Two copies of get_font_size_px? ONE shared location.
- Same calculation in multiple places? Extract it.

### 3. Minimize Indirection
Don't convert values back and forth unless necessary for the domain.

```rust
// ❌ BAD - unnecessary conversions
css: 48px → divide by 8 → level 6 → multiply by 8 → 48px

// ✅ GOOD - store what you need
css: 48px → store 48px → render 48px
```

### 4. Document Non-Obvious Decisions
Add comments explaining WHY, not WHAT.

```rust
// ✅ GOOD
// Layout boxes use signed arithmetic for viewport clipping
// This allows negative positions when elements are scrolled off-screen
let y_signed = layout_box.y as isize - scroll_offset as isize;

// ❌ BAD
// Convert y to signed  (doesn't explain why)
let y_signed = layout_box.y as isize - scroll_offset as isize;
```

### 5. Self-Review Before Committing
Before every commit, review your changes:
- "Would I understand this in 6 months without context?"
- Any magic numbers? Add constants.
- Any duplication? Extract functions.
- Any confusing indirection? Simplify.

### 6. Proactive Tech Debt Management
When you notice accumulated issues (magic numbers, duplication, unclear logic):
**STOP and ask the user:** "I notice the code has [specific issue]. Should I clean this up now or continue with the current task?"

Don't let technical debt accumulate silently.

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

**Browser/Async:**
- Async I/O uses state machines, not threads (HttpState, ImageLoadState)
- Images default to 0x0 when no width/height attributes specified
- Reflow triggered when image dimensions change from 0x0 → actual size
- Image cache keyed by full URL, reused on layout recalculation
- Viewport clipping uses signed arithmetic (isize) to handle negative positions
- Text hidden when partially off-screen, images clip pixel-by-pixel

## Key Wins

- **Async browser:** Event-driven HTTP/image loading, stays responsive during network I/O (no blocking)
- **Image caching & reflow:** Smart layout recalculation when dimensions change, prevents duplicate downloads
- **Viewport clipping:** Pixel-perfect image clipping at viewport edges using signed arithmetic
- **Preemptive multitasking:** ARM64 threading infrastructure with round-robin scheduler (10ms time slices)
- **smoltcp migration:** Production TCP/IP stack, 91% code reduction (834→76 lines)
- **Buffer exhaustion fix:** Auto-replenish RX buffers after each packet
- **ARM Generic Timer:** Hardware-independent timing (no CPU-dependent delays)
- **Modular architecture:** Clean separation (drivers/system/gui/apps)

---

**🚨 Update after major changes! 🚨**
