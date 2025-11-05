# Known Issues and Architectural Limitations

## 1. Syscall Context Switch Stack Corruption (CRITICAL)

**Status:** Workaround in place, proper fix needed

**Symptom:**
- Calling `yield_now()` during syscall handlers causes crashes with `ELR_EL1 = 0xFF000000FF000000`
- System hangs when long operations don't yield (CPU starvation)
- Debug `print_debug()` calls accidentally "fix" hangs by adding synchronization points

**Root Cause:**

In `kernel/src/kernel/exception_vector.s`, the syscall handler:

1. **Entry:** Saves ExceptionContext to kernel stack at address `SP` (e.g., 0xK1000)
2. **Rust Handler:** Calls `handle_el0_syscall_rust(ctx)` which may call `yield_now()`
3. **Context Switch:** `yield_now()` → `context_switch()` → SP changes to new thread's kernel stack (e.g., 0xK2000)
4. **Return:** Tries to restore registers from `[sp + offsets]` - but SP now points to 0xK2000, NOT the original 0xK1000!
5. **Crash:** Restores garbage values → corrupted return address → fault at 0xFF000000FF000000

**Why Debug Prints "Fix" It:**

`print_debug()` calls syscall without `yield_now()` → adds synchronization/barriers → allows other threads to run via timer interrupts → prevents CPU starvation BUT doesn't trigger context switch bug.

**Current Workaround:**

- `yield_now()` disabled in window manager screen clear loop (line 590-594 in `userspace/window_manager/main.rs`)
- Debug prints kept throughout `redraw_all()` to provide implicit synchronization
- System works but long operations can starve other threads

**Proper Fix (TODO):**

Modify `kernel/src/kernel/exception_vector.s` to preserve ExceptionContext pointer across context switches:

```asm
handle_el0_syscall_entry:
    // Save all registers
    sub sp, sp, #272
    stp x0, x1, [sp, #16 * 0]
    ... (save all registers)

    // CRITICAL: Save context pointer in callee-saved register
    // x19-x28 are preserved across function calls, including context_switch()
    mov x19, sp  // Save ExceptionContext address

    // Call Rust handler (may context switch, changing SP)
    mov x0, sp
    bl handle_el0_syscall_rust

    // CRITICAL: Restore from saved context pointer (x19), NOT sp!
    // SP may have changed due to context switch
    ldp x0, x1, [x19, #16 * 16]    // Restore ELR/SPSR
    msr elr_el1, x0
    msr spsr_el1, x1

    ldp x0, x1, [x19, #16 * 0]     // Restore all registers from x19
    ... (restore all from x19)

    // Restore x19 last
    ldr x19, [x19, #16 * 9 + 8]

    eret
```

**Testing Plan:**

1. Implement fix in `exception_vector.s`
2. Re-enable `yield_now()` in window manager screen clear loop
3. Remove debug print workarounds
4. Test terminal spawning multiple times
5. Test with multiple windows open
6. Stress test with rapid input events

---

## 2. Terminal Text Rendering Not Implemented

**Status:** Stub implementation, needs completion

**Current State:**
- Terminal window opens with title bar
- Content area shows black background only
- No text rendering (see `userspace/terminal/src/main.rs:149-150`)

**TODO:**
- Implement bitmap font rendering in terminal
- Add input event handling
- Add command processing

---

## 3. CSV Viewer and Image Viewer Disabled

**Status:** Temporarily commented out in `kernel/src/kernel/embedded_apps.rs`

**Reason:** Build errors after `cargo clean` - missing allocator configuration

**TODO:**
- Fix allocator setup for these binaries
- Re-enable in embedded_apps.rs
- Test loading and rendering
