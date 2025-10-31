# rOSt Development Roadmap

## What Works Now ✅

**Boot & Core**
- UEFI bootloader with ExitBootServices
- ARM64 exception vectors and handlers
- GIC interrupt controller
- Timer system
- Physical memory allocator
- UART serial debugging

**Graphics & Input**
- VirtIO GPU driver (1280x800 resolution)
- Hardware-accelerated cursor
- VirtIO keyboard and mouse input
- Full bitmap font rendering

**Storage & Filesystem**
- VirtIO block device driver
- SimpleFS persistent filesystem (32 files, 10MB)
- File operations: create, read, write, delete

**User Interface**
- Full GUI window manager with tiling layout
- Draggable windows with title bars and close buttons
- Menu bar (Terminal, Editor, About)
- Multiple terminal instances with interactive shell
- Text editor with mouse selection and file save/load
- Shell commands: ls, cat, create, write, rm, clear, help

## Next Major Features

### Option 1: Multitasking & Processes
**Impact:** High - Foundational for running multiple programs
- Process control blocks (PCB)
- Context switching (save/restore registers)
- Cooperative or preemptive scheduler
- Basic inter-process communication

### Option 2: Networking
**Impact:** High - Very visible, enables communication
- VirtIO-Net driver
- Simple TCP/IP stack (or lwIP integration)
- Socket API basics
- Ping, basic HTTP client/server

### Option 3: Virtual Memory
**Impact:** Medium - Needed for proper isolation
- Custom ARM64 page tables (not UEFI's)
- Memory mapping and protection
- Kernel/user space separation
- Page fault handling

### Option 4: Better Filesystem
**Impact:** Medium - More usable storage
- Directory support (folders)
- Larger file sizes
- FAT32 or ext2 driver
- VFS abstraction layer

## Future Ideas

**User Space**
- Syscall interface (SVC instruction on ARM64)
- User/kernel privilege separation
- ELF program loader
- Basic libc implementation

**Advanced Features**
- SMP (multi-core support)
- USB drivers (if moving off VirtIO)
- Audio support
- More window manager features (resize, minimize)

## Architecture Decisions

Currently: **Monolithic kernel** (everything in kernel mode)
- Simple and fast for development
- Can evolve to microkernel later if needed

Memory: **Basic physical allocator**
- Works for now with page-based allocation
- May need buddy/slab allocator for efficiency

## Learning Resources
- [OSDev Wiki](https://wiki.osdev.org/) - Essential reference
- [Writing an OS in Rust](https://os.phil-opp.com/) - Rust-specific guidance
- [VirtIO 1.0 Spec](https://docs.oasis-open.org/virtio/virtio/v1.0/) - Device drivers
- ARM Architecture Reference Manual - For ARM64 details

## Testing
- Primary: QEMU aarch64 with VirtIO devices
- Target: Eventually Raspberry Pi 4 or similar ARM64 hardware

---

**Last Updated:** 2025-10-31
**Status:** Fully functional GUI OS with persistent storage - ready for next major feature!
