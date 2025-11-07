## START HERE 

The goal is to build a modern, robust, modular Production-grade ARM64 Desktop OS written in Rust. no_std environment.

# rOSt - Rust ARM64 Operating System

**Last Updated:** 2025-11-03

## Features

**GUI:** Tiling window manager, text editor (syntax highlighting, undo/redo), file explorer, web browser (async HTTP, progressive image loading, HTML/BMP/PNG), image viewer
**Browser:** Event-driven async I/O, image caching, auto-reflow, viewport clipping, width/height attribute parsing
**Filesystem:** SimpleFS (32 files, 10MB, persistent across reboots)
**Hardware:** VirtIO GPU/Input/Block/Net, ARM Generic Timer, PL031 RTC
**Networking:** smoltcp 0.12 stack, DNS, HTTP/1.0 client, ping, download command
**Kernel:** Preemptive multitasking, round-robin scheduler, ARM64 context switching, EL0/EL1 privilege separation with syscalls
**Memory:** Higher-half kernel (0xFFFF...), dual page tables (TTBR0/TTBR1), MMU-based memory protection
**Userspace:** Production ELF loader, standalone userspace binaries, microkernel architecture
**IPC:** Shared memory (cross-process, 16MB regions), message passing (256-byte messages, ring buffers), ready for microkernel migration
**Shell:** Interactive terminal with filesystem and network commands

## Quick Start

```bash
# Build kernel
cargo build --release --target aarch64-unknown-uefi -p rust_os

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
exec shell - Load and run userspace shell ELF at EL0
```

## Architecture

### Virtual Memory Layout
**TTBR0 (User space):** 0x0000_0000_0000_0000 - 0x0000_FFFF_FFFF_FFFF
- User programs execute at EL0 with access to low-half addresses only
- Isolated per-process page tables for memory protection

**TTBR1 (Kernel space):** 0xFFFF_FF00_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF
- Kernel executes at EL1 in higher-half (L0 page table index 510)
- Shared kernel mapping across all processes
- Physical 0x40000000-0x1_0000_0000 mapped to virtual 0xFFFF_FF00_4000_0000+
- **⚠️ CRITICAL for DMA:** VirtIO devices require PHYSICAL addresses, use `memory::virt_to_phys()` for all DMA buffers (stack, heap, etc.)

**Physical Memory Map:**
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
kernel/              - Main OS kernel (EL1)
  src/kernel/        - Core (mod, memory, dtb, scheduler, thread, interrupts, elf_loader)
    drivers/         - PCI, timer, RTC, input_events
      virtio/        - gpu, input, blk, net
  src/system/
    fs/              - filesystem (SimpleFS)
    net/             - network, dns, helpers (smoltcp-based)
  src/gui/           - framebuffer, window_manager, html_parser, bmp_decoder, png_decoder
    widgets/         - browser (async), editor, console, file_explorer, text_input, image_viewer
  src/apps/          - shell (kernel), snake

librost/             - Userspace runtime library (#![no_std])
  src/runtime/       - Syscall wrappers (exit, print_debug, open, read, write, socket, etc.)

userspace/           - Userspace applications (EL0)
  shell/             - Standalone shell ELF binary
```

### Userspace Architecture

**Cargo Workspace:**
- 3 crates: `kernel` (EL1), `librost` (runtime), `userspace/*` (EL0 apps)
- Kernel builds to `aarch64-unknown-uefi` (UEFI bootable)
- Userspace builds to `aarch64-unknown-none` (freestanding ELF)

**ELF Loading:**
- Parse with `xmas-elf` crate
- Load LOAD segments into memory
- Calculate entry points (ELF vaddr → physical offset)
- Spawn as isolated process via scheduler

**Syscall Interface (`librost`):**
- Process: `exit(code)`, `getpid()`
- File I/O: `open(path, flags)`, `read(fd, buf)`, `write(fd, buf)`, `close(fd)`
- Time: `get_time()`
- Debug: `print_debug(msg)`
- Framebuffer: `fb_info()`, `fb_map()`, `fb_flush()`
- Input: `poll_event()`
- Network: `socket()`, `connect()`, `send()`, `recv()`
- IPC: `shm_create(size)`, `shm_map(id)`, `shm_unmap(id)`, `send_message(pid, data)`, `recv_message(buf, timeout)`

**IPC Architecture:**
- **Shared Memory:** Up to 16MB per region, 32 regions per process, globally accessible across ALL processes
- **Message Passing:** 256-byte messages, 16 messages per process, ring buffer implementation
- **Cross-Process Access:** `shm_map()` searches all processes for shared memory ID (not just current process)
- **Test Programs:** `ipc_sender` and `ipc_receiver` demonstrate end-to-end communication
- **Microkernel Ready:** Foundation for migrating window manager and GUI apps to userspace

**Testing:**
```
> exec shell
=== Loading Userspace Shell ELF ===
[ELF] Loaded 234 bytes at 0x7a68bb48
[USER] === rOSt Userspace Shell ===
[USER] Running at EL0 with privilege separation
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

## Debugging Methodology (MANDATORY for AI Agents)

**When debugging low-level code (MMU, interrupts, assembly), follow this checklist:**

### 1. Verify All Assumptions First
- **Never trust comments or variable names** - verify the actual values
- For address calculations: Calculate both directions (index→address AND address→index)
- Add verification math in comments: `// Verify: (0xFFFF_FF00_0000_0000 >> 39) & 0x1FF = 510 ✓`

### 2. Binary Search for the Bug Layer
When something doesn't work, verify each layer systematically:
```
Hardware configured? → Page tables set up? → Addresses calculated correctly? →
Barriers in right places? → Cache coherency maintained? → Permissions correct?
```
Don't skip layers! Simple bugs often hide in "obvious" places.

### 3. Stop After 3 Failed Attempts
If you try 3 different fixes and none work:
1. STOP adding complexity
2. Go back to first principles
3. Verify ALL assumptions from scratch
4. Check if the symptom matches a fundamentally different root cause

### 4. Add Diagnostic Output for Calculations
Don't just print values - print the *expected* vs *actual*:
```rust
uart_write_string(&format!("[DEBUG] KERNEL_BASE = {:#018x}\r\n", KERNEL_BASE));
uart_write_string(&format!("[DEBUG] → Calculated L0 index = {}\r\n", (KERNEL_BASE >> 39) & 0x1FF));
uart_write_string(&format!("[DEBUG] → Expected L0 index = 510\r\n"));
```
Mismatches jump out immediately.

### 5. Prefer Simple Explanations
**Occam's Razor for kernel debugging:**
- Wrong constant value (common) vs. obscure hardware interaction (rare)
- Arithmetic error (common) vs. cache coherency issue (less common)
- Missing permission bit (common) vs. ARM64 errata (very rare)

Try the simple explanation first.

### 6. Recognize Compiler Optimization Symptoms
**Pattern: "Works with debug output, breaks without debug output"**

This is a **compiler optimization issue**, NOT a logic bug. The debug syscalls/prints act as memory barriers.

**Immediate fixes to try (in order):**
1. `AtomicUsize`/`AtomicBool` with `Ordering::SeqCst` for shared state
2. `core::sync::atomic::compiler_fence(Ordering::SeqCst)`
3. `volatile_store()`/`volatile_load()` for memory-mapped I/O

**DON'T:**
- Add more debug output (confirms symptom, doesn't fix root cause)
- Assume the data isn't being written (it is, but compiler optimizes it away)
- Spend time debugging parsing/logic when the pattern is clear

**Example from CSV viewer:**
```rust
// ❌ BROKEN - Compiler optimizes away "dead stores" in release mode
struct CsvData {
    rows: usize,
    cols: usize,
}
fn set_cell(&mut self, row: usize, col: usize) {
    if row >= self.rows { self.rows = row + 1; }  // Optimized away!
}

// ✅ FIXED - Atomic forces actual memory writes
struct CsvData {
    rows: AtomicUsize,
    cols: AtomicUsize,
}
fn set_cell(&mut self, row: usize, col: usize) {
    let current = self.rows.load(Ordering::SeqCst);
    if row >= current { self.rows.store(row + 1, Ordering::SeqCst); }
}
```

**Why this happens:**
- Release mode (`--release`) enables aggressive optimizations
- Compiler sees `self.rows = X` but `self.rows` is never read in the same function
- Assumes it's a dead store and eliminates it
- Debug syscalls act as opaque function calls that could read memory, preventing optimization

## Critical Gotchas

**Heap Allocations in Critical Paths:**
- **NEVER use `alloc::format!()` or any heap allocations in syscall handlers, interrupt handlers, or scheduler code**
- Heap allocations can cause re-entrancy issues when the allocator is called from kernel critical paths
- Symptoms: Synchronous exceptions (`!X`), system freezes when userspace threads make blocking syscalls
- Solution: Use static strings for debug output in critical paths:
```rust
// ❌ BROKEN - crashes in syscall/scheduler context
uart_write_string(&alloc::format!("[SCHED] yield_now #{}\r\n", count));

// ✅ FIXED - no heap allocations in critical path
uart_write_string("[SCHED] yield_now entered\r\n");
```
- If you need formatted output for debugging, add it OUTSIDE the critical path (e.g., before syscall entry, after return to userspace)
- This applies to: `format!()`, `String::from()`, `vec![]`, `Box::new()`, etc. - anything that allocates

**MMU/Address Arithmetic:**
- **ALWAYS verify address calculations with explicit bit math in comments**
- L0 page table index = bits [47:39] of virtual address (9 bits = 0-511)
- Example: L0 index 510 = 0x1FE = 0b1_1111_1110
  - When placed at bits [47:39]: bit 39 must be 0, bits [47:40] = 0xFF
  - This gives 0xFFFF_FF00_0000_0000 (NOT 0xFFFF_FE80_0000_0000!)
- **Verify both directions:** Calculate address from index, then extract index from address
- Add compile-time assertions for critical addresses: `const _: () = assert!((ADDR >> 39) & 0x1FF == EXPECTED_INDEX);`
- Bit 39 determines L0 index parity: 0 = even (510, 512...), 1 = odd (509, 511...)
- TTBR0 uses low addresses (bit 55 = 0), TTBR1 uses high addresses (bit 55 = 1, canonical form = all upper bits 1)

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

**IPC:**
- Shell commands that spawn processes will freeze (shell isn't a scheduled thread)
- To test IPC: uncomment auto-spawn section in kernel/src/kernel/mod.rs and rebuild

---

**🚨 Update after major changes! 🚨**
