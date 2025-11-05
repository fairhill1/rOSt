# Microkernel Migration Plan

**Goal:** Migrate from hybrid architecture to full microkernel architecture where all GUI code runs at EL0.

## Current Architecture (Hybrid)

```
┌─────────────────────────────────────────────┐
│ Kernel (EL1) - ~8000 lines                  │
│ ├─ VirtIO drivers                           │
│ ├─ Scheduler                                │
│ ├─ IPC                                      │
│ ├─ window_manager.rs (manages windows)     │ ← REMOVE
│ ├─ widgets/ (console, editor, browser)     │ ← REMOVE
│ └─ Framebuffer rendering                   │
└─────────────────────────────────────────────┘
         ↓ IPC
┌──────────────┐
│ WM (EL0)     │ ← Only routes input, doesn't own windows
└──────────────┘
```

**Problems:**
- Two window managers (kernel + userspace) with separate state
- Apps are kernel code, not isolated processes
- Close button doesn't work (ID mismatch)
- Can't kill misbehaving apps

## Target Architecture (Full Microkernel)

```
┌─────────────────────────────────────────────┐
│ Kernel (EL1) - MINIMAL (~2000 lines)        │
│ ├─ VirtIO drivers (hardware access only)   │
│ ├─ Scheduler (thread switching)             │
│ ├─ IPC (message passing, shared memory)    │
│ ├─ MMU management (page tables)             │
│ ├─ Syscall dispatcher                       │
│ └─ Input polling → forward to WM            │
│                                             │
│ NO window_manager.rs                        │
│ NO widget code                              │
│ NO application logic                        │
└─────────────────────────────────────────────┘
         ↓ IPC          ↓ IPC         ↓ IPC
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ WM (EL0)     │  │ Terminal     │  │ Text Editor  │
│ - Window list│  │ (EL0)        │  │ (EL0)        │
│ - Layout     │  │ - Own process│  │ - Own process│
│ - Chrome     │  │ - Own buffer │  │ - Syntax     │
│ - Routing    │  │ - Shell logic│  │   highlight  │
│ - Compositing│  │              │  │              │
└──────────────┘  └──────────────┘  └──────────────┘
         ↓ IPC          ↓ IPC         ↓ IPC
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Browser      │  │ File Explorer│  │ Snake Game   │
│ (EL0)        │  │ (EL0)        │  │ (EL0)        │
│ - HTML parse │  │ - FS access  │  │ - Game loop  │
│ - Render DOM │  │ - List files │  │ - Physics    │
└──────────────┘  └──────────────┘  └──────────────┘
```

## Syscalls

### Existing
- `sys_exit(code)`
- `sys_print_debug(msg)`
- `sys_getpid()`
- `sys_open/read/write/close()`
- `sys_fb_info()`, `sys_fb_map()`
- `sys_poll_event()`
- `sys_shm_create/map/unmap()`
- `sys_send_message/recv_message()`

### New Syscalls Needed
```rust
sys_spawn_elf(path: &str) → u64         // Spawn ELF, return PID
sys_kill(pid: u64) → i64                // Kill process
sys_fb_flush(x: u32, y: u32, w: u32, h: u32) → i64  // Flush FB region
```

## IPC Protocol

### Kernel → WM
```rust
enum KernelToWM {
    InputEvent {
        sender_pid: u32,  // Kernel's PID for responses
        mouse_x: i32,
        mouse_y: i32,
        event: InputEvent,
    }
}
```

### WM → Kernel
```rust
enum WMToKernel {
    RouteInput {
        app_pid: u32,      // Changed from window_id to app_pid
        event: InputEvent,
    },
    NoAction,              // WM handled it (menu click, close button, etc)
}
```

### WM → App
```rust
enum WMToApp {
    InputEvent { event: InputEvent },
    Resize { width: u32, height: u32 },
    Focus { focused: bool },
    Redraw { },           // Window needs redraw (e.g., exposed)
    Close { },            // User clicked close button
}
```

### App → WM
```rust
enum AppToWM {
    CreateWindow {
        title: [u8; 64],
        title_len: usize,
        width: u32,
        height: u32,
        shmem_id: u64,    // Shared memory for rendering
    },
    UpdateBuffer { },     // New frame ready in shared memory
    Exit { },             // App closing gracefully
}
```

## Data Flow

### Window Creation
```
1. User clicks "Terminal" in menu
2. WM handles menu click
3. WM calls sys_spawn_elf("/bin/terminal") → PID 5
4. Terminal app starts, calls sys_shm_create(640*480*4) → shmem_id
5. Terminal → WM: CreateWindow { title: "Terminal", shmem_id, ... }
6. WM adds window to its list, associates with PID 5
7. WM composites and flushes framebuffer
```

### Input Routing
```
1. Kernel polls VirtIO devices
2. Kernel → WM: InputEvent { mouse_x, mouse_y, event }
3. WM determines target:
   - Menu click? WM spawns app
   - Close button? WM sends Close to app, removes window
   - Window content? WM → App: InputEvent
4. If routed: WM → Kernel: RouteInput { app_pid: 5, event }
5. Kernel forwards: Kernel → App(PID 5): InputEvent
```

### Rendering
```
1. App renders to its shared memory buffer
2. App → WM: UpdateBuffer { }
3. WM:
   - Maps app's shared memory (sys_shm_map)
   - Composites app buffer + chrome to framebuffer
   - Calls sys_fb_flush(x, y, w, h)
```

### Window Close
```
1. User clicks close button
2. WM detects close button click
3. WM → App: Close { }
4. App cleans up, exits
5. App → WM: Exit { } (or process dies)
6. WM removes window from list
7. WM redraws remaining windows
```

## Migration Steps

### Phase 1: Syscalls (1-2 hours)
- [ ] Add `sys_spawn_elf(path)` syscall
  - Parse path, load ELF, spawn process
  - Return PID to caller
- [ ] Add `sys_kill(pid)` syscall
  - Terminate process, clean up resources
- [ ] Add `sys_fb_flush(x, y, w, h)` syscall
  - Currently fb_flush() has no parameters
  - Add dirty region tracking

### Phase 2: WM Ownership (2-3 hours)
- [ ] Move window list to userspace WM
  - Port `WindowState` struct to userspace
  - Port tiling layout logic
  - Port menu bar logic
- [ ] Update WM to handle `CreateWindow` IPC from apps
- [ ] Update WM to composite app buffers to FB
- [ ] Update WM to spawn apps via `sys_spawn_elf`

### Phase 3: Convert Terminal (3-4 hours)
- [ ] Create `userspace/terminal/` crate
- [ ] Port console widget code
- [ ] Port shell logic
- [ ] Implement shared memory rendering
- [ ] Implement `AppToWM` IPC protocol
- [ ] Test end-to-end: spawn, input, render, close

### Phase 4: Convert Other Apps (6-8 hours)
- [ ] Convert Editor to `userspace/editor/`
- [ ] Convert Browser to `userspace/browser/`
- [ ] Convert File Explorer to `userspace/file_explorer/`
- [ ] Convert Snake to `userspace/snake/`

### Phase 5: Cleanup (1-2 hours)
- [ ] Delete `kernel/src/gui/window_manager.rs`
- [ ] Delete `kernel/src/gui/widgets/`
- [ ] Delete `kernel/src/apps/shell.rs`
- [ ] Remove unused imports/dependencies
- [ ] Update kernel size metrics

## Success Criteria

- ✅ Kernel is <2500 lines
- ✅ All GUI code runs at EL0
- ✅ Apps are isolated processes with own PIDs
- ✅ Can kill misbehaving apps without kernel panic
- ✅ Close button works
- ✅ Window focus works
- ✅ Tiling layout works
- ✅ Menu spawns apps correctly
- ✅ All 5 apps functional (Terminal, Editor, Browser, Files, Snake)

## Risks & Mitigations

**Risk:** IPC overhead kills performance
- *Mitigation:* Use shared memory for large data (framebuffers), not message passing

**Risk:** Breaking existing functionality during migration
- *Mitigation:* Keep kernel WM in place until userspace WM fully working, then delete

**Risk:** Complex synchronization bugs between processes
- *Mitigation:* Start with Terminal (simplest), validate architecture before converting complex apps

**Risk:** Running out of context during implementation
- *Mitigation:* Work incrementally, commit after each phase

## Notes

- Keep `embedded_apps.rs` pattern for now (embed ELFs in kernel image)
- Later can add VirtIO-FS to load apps from disk
- WM is "just another app" with special privileges (FB access, spawn apps)
- Apps don't need kernel changes - pure userspace code
