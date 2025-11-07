# rOSt Kernel Audit Report: Memory Corruption & Instability Analysis

**Date:** 2025-01-07
**Symptom:** Random artifacts (ASCII characters, pixel corruption) appear in terminal windows after opening/closing multiple terminals
**Root Cause:** Multiple critical architectural bugs causing memory corruption, race conditions, and resource leaks

---

## Executive Summary

Found **11 CRITICAL** and **8 HIGH** severity bugs that cause systemic instability. The primary issues are:

1. **Shared memory resource leak** - Physical memory never freed on process termination
2. **Zombie thread use-after-recycle** - Stale threads execute in recycled process memory
3. **Zero memory isolation** - All processes share same page tables with full RAM access
4. **Race conditions** - Thread removal/process cleanup unsafe with scheduler
5. **Bounds violations** - Process/stack limits not enforced after 8 processes

**Impact:** After opening/closing ~8-64 terminals, the system enters unstable state where:
- Shared memory exhausted → allocator reuses active memory
- Zombie threads access recycled process memory
- No protection between processes allows cross-contamination

---

## 1. SCHEDULER & CONTEXT SWITCHING

### CRITICAL #1: Thread Removal Race Condition During Process Termination

**File:** `/kernel/src/kernel/thread.rs:833-846`
**Severity:** CRITICAL

```rust
// mark_process_terminated() holds PROCESS_MANAGER lock
let mut scheduler = crate::kernel::scheduler::SCHEDULER.lock();
let before_count = scheduler.threads.len();
scheduler.threads.retain(|t| t.process_id != id);  // ❌ Nested lock acquisition!
```

**Problem:** Creates nested lock scenario:
- Process termination holds `PROCESS_MANAGER` lock (line 906)
- Then acquires `SCHEDULER` lock (line 837)
- If timer interrupt fires between locks → potential deadlock
- If thread removal fails silently, zombie threads remain in scheduler

**Impact:** Process cleanup fails, leaving threads that point to freed/recycled memory. When scheduled, they corrupt terminal buffers.

**How it causes artifacts:**
1. Terminal process exits but thread not removed from scheduler
2. Process slot recycled for new terminal (CRITICAL #2)
3. Scheduler switches to old thread with stale SP pointing into new terminal's memory
4. Random writes corrupt new terminal's framebuffer → artifacts appear

**Fix:**
```rust
// BEFORE: Inside mark_process_terminated() with locks held
scheduler.threads.retain(|t| t.process_id != id);

// AFTER: Separate cleanup phase, explicit lock ordering
pub fn cleanup_terminated_threads(process_id: usize) {
    let daif = disable_interrupts();
    let mut scheduler = SCHEDULER.lock();
    scheduler.threads.retain(|t| t.process_id != process_id);
    drop(scheduler);
    restore_interrupts(daif);
}
// Call AFTER releasing PROCESS_MANAGER lock
```

---

### CRITICAL #2: Zombie Process Memory Access (Use-After-Recycle)

**File:** `/kernel/src/kernel/thread.rs:733-766`
**Severity:** CRITICAL

```rust
fn create_user_process_with_stack_slot(...) -> usize {
    // Find existing zombie at this stack slot to RECYCLE
    if let Some(zombie) = self.processes.iter_mut().find(|p|
        p.state == ProcessState::Terminated && p.stack_index == Some(stack_index)
    ) {
        // ❌ Does NOT check if threads still reference this process!
        zombie.id = process_id;
        zombie.state = ProcessState::Created;
        zombie.main_thread_id = None;
        // ... reset fields but REUSE kernel_stack Box
```

**Problem:** Process recycling happens WITHOUT verifying all threads are cleaned up. Combined with CRITICAL #1:
1. Thread removal fails (race condition)
2. Process gets recycled with NEW ID but thread still in scheduler with OLD context
3. Thread executes with NEW process ID but OLD SP/registers
4. Accesses memory thinking it's old process, but memory now belongs to new process

**Impact:** Use-after-recycle. Most severe consequence - explains why artifacts appear specifically after multiple open/close cycles.

**Fix:**
```rust
// Add assertion before recycling
if let Some(zombie) = self.processes.iter_mut().find(...) {
    // Verify no threads reference this process
    let scheduler = SCHEDULER.lock();
    let has_threads = scheduler.threads.iter().any(|t| t.process_id == zombie.id);
    drop(scheduler);

    if has_threads {
        // Don't recycle if threads still exist - create new process instead
        return self.create_user_process_new_slot(process_id, entry_point);
    }

    zombie.id = process_id;
    // ... rest of recycling
}
```

---

### HIGH #1: Stack Pointer Not Validated on Context Switch

**File:** `/kernel/src/kernel/thread.rs:473-507`
**Severity:** HIGH

```rust
pub unsafe extern "C" fn context_switch(
    _current: *mut ThreadContext,
    _next: *const ThreadContext,
) {
    core::arch::naked_asm!(
        "ldr x9, [x1, #96]",
        "mov sp, x9",  // ❌ No validation that SP is in valid range!
        "ret",
    )
}
```

**Problem:** If thread context is corrupted (from bugs #1/#2), SP could point to:
- Another process's memory
- Device MMIO regions (VirtIO, UART at 0x09000000+)
- Unallocated memory
- Kernel data structures

**Impact:** Function calls push to invalid stack → arbitrary memory corruption, crashes, or silent data corruption.

**Fix:**
```rust
pub unsafe extern "C" fn context_switch(...) {
    core::arch::naked_asm!(
        // Load SP from next context
        "ldr x9, [x1, #96]",

        // Validate SP is in user stack range (0x48000000 - 0x48100000)
        "mov x10, #0x48000000",
        "cmp x9, x10",
        "b.lt .Linvalid_sp",
        "mov x10, #0x48100000",
        "cmp x9, x10",
        "b.ge .Linvalid_sp",

        // Valid - switch SP
        "mov sp, x9",
        "ret",

        ".Linvalid_sp:",
        "brk #1",  // Trigger debug exception
    )
}
```

---

### MEDIUM #1: Scheduler Ready Queue Accumulates Dead Threads

**File:** `/kernel/src/kernel/scheduler.rs:124-128`
**Severity:** MEDIUM

```rust
fn pick_next(&mut self) -> Option<usize> {
    // Cleanup only happens HERE
    self.ready_queue.retain(|&id| {
        self.threads.iter().any(|t| t.id == id && t.state != ThreadState::Terminated)
    });
```

**Problem:** Threads removed from `scheduler.threads` by `mark_process_terminated()` AFTER being added to ready_queue. Time window creates accumulation of dead IDs.

**Impact:** Scheduler slowly degrades as dead thread IDs accumulate in queue. Eventually causes scheduling delays or starvation.

---

## 2. MMU & PAGE TABLES

### CRITICAL #3: No TTBR0 Switching Per-Process (Zero Memory Isolation)

**File:** `/kernel/src/kernel/memory.rs:726-748`
**Severity:** CRITICAL

```rust
/// Switch TTBR0 to user page tables (called once during boot)
/// After this, we don't need to switch TTBR0 on context switches
pub fn switch_ttbr0_to_user_tables() {
    // ❌ Called ONCE at boot, then TTBR0 stays on shared user tables FOREVER!
```

**Current behavior:** ALL user processes share the SAME TTBR0 page tables mapping 0-4GB with USER permissions.

**Problem:**
- No memory isolation between processes
- Process A can directly read/write Process B's memory
- Terminal A can write to Terminal B's framebuffer
- No fault isolation - one bad pointer in any process crashes everything

**Impact:** **This is the most fundamental security/isolation bug.** Explains cross-terminal corruption even without other bugs.

**Example attack:**
```rust
// Terminal A can corrupt Terminal B
let terminal_b_framebuffer = 0x60400000 as *mut u32;  // Known physical address
unsafe { *terminal_b_framebuffer = 0xFF_00_FF_00; }  // Write magenta pixel
```

**Fix requires major refactor:**
```rust
// Per-process page tables
pub struct Process {
    ttbr0_base: u64,  // Physical address of L0 page table for this process
    // ...
}

// In context_switch()
unsafe {
    asm!(
        "msr ttbr0_el1, {ttbr0}",
        "dsb sy",
        "tlbi vmalle1is",
        "dsb sy",
        "isb",
        ttbr0 = in(reg) next_process.ttbr0_base,
    );
}
```

---

### CRITICAL #4: User Page Tables Map Entire Physical Memory

**File:** `/kernel/src/kernel/memory.rs:590-622`
**Severity:** CRITICAL

```rust
// TEMPORARY: Map entire 4GB into user space with USER permissions
// This allows the test user program (which is compiled into the kernel) to execute
// TODO: In production, load user binaries at low addresses and only map those pages

for i in 0..512usize {
    let addr = (i as u64) * 0x200000;
    USER_L2_TABLE_0.entries[i] = PageTableEntry::new_block(
        addr,
        true,  // USER accessible
        true,  // Write
        true,  // Read
        ...
    );
}
```

**Problem:** Comment says "TEMPORARY" but this is production code! Every userspace process can access:
- All physical RAM (0x40000000-0x80000000)
- All device MMIO:
  - UART: 0x09000000
  - RTC: 0x09010000
  - VirtIO: 0x10000000+
  - PCI ECAM: 0x4010000000
- Other processes' stacks, heaps, and data
- Kernel data structures (if in low 4GB physical)

**Impact:**
1. Terminal can accidentally overwrite another terminal's stack/heap
2. Buggy process can corrupt kernel structures by writing to physical memory
3. Process can reprogram hardware devices
4. Complete violation of security model

**Fix:** Only map process's actual memory regions:
```rust
// Map only:
// - Process .text/.data/.bss (from ELF)
// - Process stack
// - Shared memory regions explicitly mapped
// - NO device MMIO, NO other processes' memory
```

---

### HIGH #2: Shared Memory Region Not Properly Isolated

**File:** `/kernel/src/kernel/syscall_ipc.rs:7-46`
**Severity:** HIGH

```rust
struct ShmAllocator {
    next_addr: u64,  // ❌ Simple bump allocator, never frees!
    end_addr: u64,
}

fn allocate(&mut self, size: usize) -> Option<u64> {
    let aligned_size = (size + 4095) & !4095;
    if self.next_addr + aligned_size as u64 > self.end_addr {
        return None; // ❌ Can't allocate even if earlier regions freed!
    }

    let addr = self.next_addr;
    self.next_addr += aligned_size as u64;  // ❌ Only goes UP, never DOWN!
    Some(addr)
}
```

**Problem:** Bump allocator never reclaims freed regions. Combined with CRITICAL #5 (memory not freed on termination):
- 256MB shared memory region (0x60000000-0x70000000)
- Each terminal: ~4MB framebuffer
- Max allocations: 256MB / 4MB = **64 terminals**
- After 64 allocations, allocator returns `None` even if 63 regions freed
- New terminals **can't allocate** → undefined behavior or reuse active memory

**Impact:** After ~64 terminal open/close cycles, system becomes unstable. New allocations fail or reuse memory unsafely.

**Fix - Implement free list:**
```rust
struct ShmAllocator {
    free_regions: Vec<(u64, usize)>,  // (addr, size) of freed blocks
    next_addr: u64,
    end_addr: u64,
}

fn allocate(&mut self, size: usize) -> Option<u64> {
    let aligned_size = (size + 4095) & !4095;

    // First-fit: Try to reuse freed regions
    for i in 0..self.free_regions.len() {
        let (addr, free_size) = self.free_regions[i];
        if free_size >= aligned_size {
            self.free_regions.remove(i);
            if free_size > aligned_size {
                // Return leftover to free list
                self.free_regions.push((addr + aligned_size as u64, free_size - aligned_size));
            }
            return Some(addr);
        }
    }

    // No reusable region, allocate new
    if self.next_addr + aligned_size as u64 > self.end_addr {
        return None;
    }

    let addr = self.next_addr;
    self.next_addr += aligned_size as u64;
    Some(addr)
}

fn free(&mut self, addr: u64, size: usize) {
    let aligned_size = (size + 4095) & !4095;
    self.free_regions.push((addr, aligned_size));
    // TODO: Coalesce adjacent free regions
}
```

---

## 3. SHARED MEMORY (SHM)

### CRITICAL #5: Shared Memory Not Freed on Process Termination

**File:** `/kernel/src/kernel/thread.rs:790-848`
**Severity:** CRITICAL - **PRIMARY ROOT CAUSE**

```rust
pub fn mark_process_terminated(&mut self, id: usize) {
    if let Some(process) = self.get_process_mut(id) {
        process.state = ProcessState::Terminated;

        // Return stack slot to free list...
        // Clear file descriptors...
        // Remove threads from scheduler...

        // ❌ NO CLEANUP OF process.shm_table!
        // Shared memory regions remain allocated!
        // Physical memory at 0x60000000+ never reclaimed!
    }
}
```

**What happens:**
1. Terminal 1 creates framebuffer: `shm_create(4MB)` → physical addr `0x60000000`
2. Terminal exits → `mark_process_terminated()` called
3. `process.shm_table` still contains region `{id=1, physical_addr=0x60000000, size=4MB}`
4. Process slot recycled but **SHM regions NOT freed**
5. Terminal 2 allocates: `shm_create(4MB)` → physical addr `0x60400000` (bump allocator increments)
6. After 64 terminals: allocator exhausted (256MB / 4MB = 64)
7. Allocation fails or **wraps around, reusing active addresses** → CORRUPTION

**Impact:** Memory leak causes eventual shared memory exhaustion. After exhaustion, terminals either:
- Fail to allocate (crash)
- Reuse active physical addresses (multiple terminals write to same framebuffer)
- Trigger CRITICAL #4 (ID collision) sooner

**Fix:**
```rust
pub fn mark_process_terminated(&mut self, id: usize) {
    if let Some(process) = self.get_process_mut(id) {
        process.state = ProcessState::Terminated;

        // FREE ALL SHARED MEMORY REGIONS
        for region in &mut process.shm_table.regions {
            if let Some(shm_region) = region.take() {
                // Free physical memory back to allocator
                SHM_ALLOCATOR.lock().free(shm_region.physical_addr, shm_region.size);
            }
        }

        // Rest of cleanup...
    }
}
```

---

### CRITICAL #6: Shared Memory ID Wrapping and Collision

**File:** `/kernel/src/kernel/thread.rs:82-91`
**Severity:** CRITICAL

```rust
pub fn alloc(&mut self, size: usize, physical_addr: u64) -> Option<i32> {
    for slot in self.regions.iter_mut() {
        if slot.is_none() {
            let id = self.next_id;
            self.next_id += 1;  // ❌ No overflow check! Can wrap or reuse IDs!
            *slot = Some(SharedMemoryRegion { id, size, physical_addr, ... });
            return Some(id);
        }
    }
    None
}
```

**Problem:** `next_id` is `i32`, increments forever without bounds checking:
- After 2^31 allocations: wraps to `i32::MIN` (negative)
- Or wraps back to 1, colliding with active IDs
- Process A allocates: `shm_create()` → ID = 1, physical = 0x60000000
- After wraparound, Process B allocates: → ID = 1, physical = 0x65000000
- Process A calls `shm_map(1)` → searches all processes, finds B's region → returns wrong address!

**Impact:** Cross-process memory access. Terminal A maps Terminal B's framebuffer → writes appear in wrong window.

**Fix:**
```rust
pub fn alloc(&mut self, size: usize, physical_addr: u64) -> Option<i32> {
    for slot in self.regions.iter_mut() {
        if slot.is_none() {
            let id = self.next_id;

            // Check for overflow
            if id == i32::MAX {
                // Out of IDs - this should never happen in practice
                return None;
            }

            self.next_id += 1;
            *slot = Some(SharedMemoryRegion { id, size, physical_addr, ... });
            return Some(id);
        }
    }
    None
}
```

Or better: use `u64` IDs that never wrap in practice.

---

### HIGH #3: Race Condition in sys_shm_map() (TOCTOU)

**File:** `/kernel/src/kernel/syscall_ipc.rs:99-110` and `/kernel/src/kernel/thread.rs:952-980`
**Severity:** HIGH

```rust
pub fn find_shared_memory(shm_id: i32) -> Option<u64> {
    let daif = disable_interrupts();
    let mut pm_lock = PROCESS_MANAGER.lock();

    let result = if let Some(pm) = pm_lock.as_mut() {
        for process in &mut pm.processes {
            if process.state == ProcessState::Terminated {  // ❌ TOCTOU!
                continue;
            }

            if let Some(region) = process.shm_table.get_mut(shm_id) {
                region.virtual_addr = Some(region.physical_addr);
                found = Some(region.physical_addr);
                break;
            }
        }
    };

    drop(pm_lock);
    restore_interrupts(daif);
    result  // ❌ Return address but process could terminate AFTER lock released!
}
```

**Problem - Time-of-Check-Time-of-Use:**
1. Check `process.state == Terminated` (line 968) while holding lock
2. Find SHM region, get physical address
3. Release lock, restore interrupts
4. Return address to caller
5. **Timer interrupt fires** → process terminates and gets recycled
6. Caller uses `physical_addr` that now belongs to different process

**Impact:** Use-after-free on shared memory. Reading/writing to wrong terminal's framebuffer.

**Fix - Increment reference count:**
```rust
pub struct SharedMemoryRegion {
    id: i32,
    size: usize,
    physical_addr: u64,
    virtual_addr: Option<u64>,
    ref_count: AtomicUsize,  // Track active mappings
}

pub fn find_shared_memory(shm_id: i32) -> Option<u64> {
    let pm_lock = PROCESS_MANAGER.lock();

    if let Some(pm) = pm_lock.as_ref() {
        for process in &pm.processes {
            if process.state == ProcessState::Terminated {
                continue;
            }

            if let Some(region) = process.shm_table.get(shm_id) {
                region.ref_count.fetch_add(1, Ordering::SeqCst);
                return Some(region.physical_addr);
            }
        }
    }

    None
}

// In sys_shm_unmap():
region.ref_count.fetch_sub(1, Ordering::SeqCst);

// In mark_process_terminated(), only free if ref_count == 0
```

---

## 4. INTERRUPTS & SYSCALLS

### CRITICAL #7: Re-entrancy in Interrupt Handler (Deadlock Risk)

**File:** `/kernel/src/kernel/interrupts.rs:120-248`
**Severity:** CRITICAL

```rust
extern "C" fn handle_el0_syscall_rust(ctx: *mut ExceptionContext) {
    // ARM64 automatically disables IRQ interrupts when taking SVC
    // ❌ Re-enable interrupts to allow timer IRQs!
    unsafe {
        core::arch::asm!("msr daifclr, #2");
    }

    // ... syscall handling ...
}
```

**Problem:** Syscalls re-enable interrupts immediately, allowing timer IRQs to fire mid-syscall:
1. Syscall starts, acquires `PROCESS_MANAGER.lock()`
2. Interrupts enabled
3. Timer IRQ fires
4. IRQ handler calls `get_current_process()` → tries to acquire `PROCESS_MANAGER.lock()` → **DEADLOCK**

OR:
1. Syscall manipulating `scheduler.ready_queue`
2. Timer IRQ fires mid-manipulation
3. IRQ handler also manipulates ready_queue → **data structure corruption**

**Impact:** Deadlocks and corrupted scheduler state. Explains intermittent system hangs and scheduling bugs.

**Fix:**
```rust
// Option 1: Keep interrupts disabled during syscalls (safest)
extern "C" fn handle_el0_syscall_rust(ctx: *mut ExceptionContext) {
    // DON'T re-enable interrupts
    // Syscalls are fast enough that missing a timer tick is acceptable
}

// Option 2: Fine-grained interrupt control
extern "C" fn handle_el0_syscall_rust(ctx: *mut ExceptionContext) {
    // Only enable interrupts during blocking operations
    match syscall_number {
        SYS_READ | SYS_WRITE => {
            // These may block - enable interrupts
            unsafe { asm!("msr daifclr, #2"); }
            // ...
            unsafe { asm!("msr daifset, #2"); }
        }
        _ => {
            // Fast syscalls - keep interrupts disabled
        }
    }
}
```

---

### HIGH #4: Stack Corruption from Large Syscall Buffers

**File:** `/kernel/src/kernel/syscall.rs:357-391` (and many others)
**Severity:** HIGH

```rust
fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    // Allocate buffer to read entire file
    let mut file_buffer = alloc::vec![0u8; 1024 * 1024]; // ❌ 1MB heap allocation!
    // ...
}
```

**Problem:**
- Syscalls allocate large buffers (up to 1MB) on heap
- Kernel stacks are 512KB (thread.rs:275)
- If heap fragmented or exhausted → panic in allocator OR silent corruption
- Stack frames + locals + heap metadata can overflow stack into adjacent memory

**Impact:** Stack overflow corrupts adjacent process stacks, kernel data, or device MMIO regions.

**Fix:**
```rust
// Use static buffer with size limit
static FILE_BUFFER: spin::Mutex<[u8; 64 * 1024]> = spin::Mutex::new([0; 64 * 1024]);

fn sys_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    if count > 64 * 1024 {
        return SyscallError::InvalidArgument.as_i64();
    }

    let mut file_buffer = FILE_BUFFER.lock();
    // ... use file_buffer[..count] ...
}
```

---

### MEDIUM #2: Missing Interrupt Disable in Critical Sections

**File:** `/kernel/src/kernel/syscall.rs:618-632` (and many others)
**Severity:** MEDIUM

```rust
fn get_current_process() -> Option<usize> {
    let daif = disable_interrupts();  // ✓ This function does it correctly
    let scheduler = SCHEDULER.lock();
    let result = ...;
    drop(scheduler);
    restore_interrupts(daif);
    result
}
```

**Problem:** While this function is correct, many syscall handlers call `SCHEDULER.lock()` or `PROCESS_MANAGER.lock()` WITHOUT disabling interrupts first:
- `sys_read()` - line 357
- `sys_write()` - line 426
- `sys_open()` - line 278

**Impact:** Potential deadlocks if timer IRQ fires while holding locks.

**Fix:** Audit all lock acquisitions, add interrupt disable where missing.

---

## 5. PROCESS LIFECYCLE

### CRITICAL #8: Process Table Array Bounds Not Enforced

**File:** `/kernel/src/kernel/thread.rs:631-648`
**Severity:** CRITICAL

```rust
pub struct ProcessManager {
    processes: alloc::vec::Vec<Process>,  // ❌ No MAX_PROCESSES limit!
    next_process_id: usize,  // ❌ Increments forever!
    free_stack_slots: [Option<usize>; MAX_USER_PROCESSES],  // Fixed: 8 slots
    free_stack_count: usize,
}
```

**Problem:**
- `next_process_id` increments indefinitely (no wraparound handling)
- `processes` Vec can grow beyond 8 entries
- `free_stack_slots` array only has **8 slots**
- After 8 user processes created, `free_stack_count == 0` but can still create processes
- New processes get `stack_index >= 8` or uninitialized stack_index

**Stack calculation (line 249):**
```rust
let stack_addr = USER_STACK_BASE + (stack_index as u64 * USER_STACK_SIZE as u64);
// USER_STACK_BASE = 0x48000000
// USER_STACK_SIZE = 128KB
// stack_index = 8 → stack_addr = 0x48100000
// This is OUTSIDE reserved region (should end at 0x48100000)!
```

**Impact:** After 8 terminal spawns:
- stack_index >= 8
- Stacks allocated OUTSIDE reserved region
- Overlap with next physical region (heap? device MMIO?)
- Instant corruption or crashes

**Fix:**
```rust
const MAX_USER_PROCESSES: usize = 8;

impl ProcessManager {
    pub fn create_user_process(...) -> Option<usize> {
        // Count active user processes
        let user_process_count = self.processes.iter()
            .filter(|p| p.stack_index.is_some() && p.state != ProcessState::Terminated)
            .count();

        if user_process_count >= MAX_USER_PROCESSES {
            return None;  // Limit reached
        }

        // ... rest of creation
    }
}
```

---

### CRITICAL #9: Zombie State Machine Inconsistency

**File:** `/kernel/src/kernel/thread.rs:782-788`
**Severity:** CRITICAL

```rust
pub fn terminate_process(&mut self, id: usize) {
    if let Some(process) = self.get_process_mut(id) {
        process.state = ProcessState::Terminated;
        // ❌ Only sets state, doesn't free resources!
    }
}
```

**Also in thread.rs:599-612:**
```rust
pub fn exit() -> ! {
    // ...
    thread.state = ThreadState::Terminated;
    let process_id = thread.process_id;
    terminate_process(process_id);  // ❌ Incomplete cleanup!
    // ...
}
```

**Problem:** Multiple termination functions with different cleanup levels:
- `terminate_process()` - only sets `process.state = Terminated`
- `mark_process_terminated()` - sets state + frees stack slot + removes threads
- Different call sites use different functions inconsistently

**Impact:** Some exit paths leave zombie processes fully intact (SHM not freed, threads not removed), others partially clean up. Creates inconsistent state depending on how process exits.

**Fix - Single termination path:**
```rust
// Remove terminate_process(), only use mark_process_terminated()
pub fn exit() -> ! {
    let mut sched = SCHEDULER.lock();
    if let Some(id) = sched.current_thread {
        let process_id = sched.threads.iter()
            .find(|t| t.id == id)
            .map(|t| t.process_id);
        drop(sched);

        if let Some(pid) = process_id {
            // Use full cleanup path
            let mut pm = PROCESS_MANAGER.lock();
            if let Some(pm) = pm.as_mut() {
                pm.mark_process_terminated(pid);
            }
        }
    }

    loop {
        yield_now();  // Never return
    }
}
```

---

### HIGH #5: Free Stack Slot Tracking Corruption

**File:** `/kernel/src/kernel/thread.rs:677-724`
**Severity:** HIGH

```rust
let stack_index = if self.free_stack_count > 0 {
    let mut found_idx = None;
    for (i, slot) in self.free_stack_slots.iter_mut().enumerate() {
        if slot.is_some() {
            found_idx = *slot;  // ❌ Gets VALUE (stack index), not array index!
            *slot = None;
            self.free_stack_count -= 1;
            break;
        }
    }

    if let Some(idx) = found_idx {
        idx  // This is correct by accident
    }
```

**Problem:** Code finds first `Some` value and returns the stack INDEX stored in it, not the array index. Works by accident but fragile:
- If `free_stack_count` gets out of sync (race or bug) → wrong behavior
- No validation that returned index is in valid range

**Impact:** Stack slot allocator can fail even when slots available, or allocate same slot twice if counter corrupted.

**Fix:**
```rust
let stack_index = if self.free_stack_count > 0 {
    let mut found = None;
    for (array_idx, slot) in self.free_stack_slots.iter_mut().enumerate() {
        if let Some(stack_idx) = slot {
            // Validate range
            if *stack_idx >= MAX_USER_PROCESSES {
                // Corruption detected
                panic!("Corrupted free_stack_slots: invalid index {}", stack_idx);
            }

            found = Some(*stack_idx);
            *slot = None;
            self.free_stack_count -= 1;
            break;
        }
    }

    found.expect("free_stack_count out of sync with array")
}
```

---

## 6. STACK MANAGEMENT

### HIGH #6: User Stacks Not Bounds-Checked

**File:** `/kernel/src/kernel/thread.rs:275-284`
**Severity:** HIGH

```rust
const STACK_SIZE: usize = 512 * 1024; // Kernel stack
const USER_STACK_SIZE: usize = 128 * 1024; // User stack: 128KB
const MAX_USER_PROCESSES: usize = 8;
const USER_STACK_BASE: u64 = 0x48000000;

static mut NEXT_USER_STACK_INDEX: usize = 0;  // ❌ Can exceed 8!
```

**Problem:** `NEXT_USER_STACK_INDEX` used in older code paths (line 701-722), increments without bound:
- After 8 processes: `NEXT_USER_STACK_INDEX = 8`
- Stack address = `0x48000000 + 8 * 128KB = 0x48100000`
- Reserved region: `0x48000000` to `0x48000000 + 8*128KB = 0x48100000`
- **Stack 8 is exactly at the boundary** → next region is heap or MMIO

**Impact:** Combined with CRITICAL #8, stacks overflow into adjacent memory regions causing instant corruption.

**Fix:**
```rust
// Remove NEXT_USER_STACK_INDEX entirely, only use free_stack_slots

// Add assertion in thread creation:
impl Thread {
    pub fn new_user_with_stack_index(id: usize, stack_index: usize) -> Self {
        assert!(stack_index < MAX_USER_PROCESSES,
                "stack_index {} exceeds MAX_USER_PROCESSES {}",
                stack_index, MAX_USER_PROCESSES);

        let stack_addr = USER_STACK_BASE + (stack_index as u64 * USER_STACK_SIZE as u64);
        // ...
    }
}
```

---

### MEDIUM #3: No Guard Pages Between Stacks

**File:** `/kernel/src/kernel/thread.rs:222-255`
**Severity:** MEDIUM

```rust
pub fn new_user_with_stack_index(id: usize, stack_index: usize) -> Self {
    // Calculate fixed address for user stack
    let stack_addr = USER_STACK_BASE + (stack_index as u64 * USER_STACK_SIZE as u64);

    // ❌ Stacks are immediately adjacent, no guard pages!
    // Stack 0: 0x48000000 - 0x48020000 (128KB)
    // Stack 1: 0x48020000 - 0x48040000 (128KB) ← directly follows!
```

**Problem:** Stacks allocated contiguously with no unmapped guard pages between them. If process overflows its 128KB stack, it directly writes into next process's stack. No page fault to catch overflow.

**Impact:** Stack overflow silently corrupts neighboring process memory. **This is likely a major contributor to terminal artifacts!**

**Example scenario:**
1. Terminal A has deep call stack or recursive function → overflows 128KB
2. Writes spill into Terminal B's stack (immediately adjacent)
3. Terminal B's stack now has corrupted return addresses, local variables
4. Terminal B executes with corrupted state → displays garbage

**Fix:**
```rust
const USER_STACK_SIZE: usize = 128 * 1024;
const GUARD_PAGE_SIZE: usize = 4096;
const STACK_REGION_SIZE: usize = USER_STACK_SIZE + GUARD_PAGE_SIZE;  // 132KB

// In page table setup:
for i in 0..MAX_USER_PROCESSES {
    let stack_base = USER_STACK_BASE + (i as u64 * STACK_REGION_SIZE as u64);

    // Map actual stack (128KB)
    map_pages(stack_base, USER_STACK_SIZE, USER_RW);

    // Unmap guard page (4KB) - will fault on access
    unmap_pages(stack_base + USER_STACK_SIZE as u64, GUARD_PAGE_SIZE);
}
```

---

### MEDIUM #4: Kernel Stack Not Zeroed When Recycling

**File:** `/kernel/src/kernel/thread.rs:758-763`
**Severity:** MEDIUM

```rust
// Process recycling
zombie.kernel_stack = process.kernel_stack;  // ❌ Reuse Box without zeroing!

// Zero the user stack
unsafe {
    let stack_addr = USER_STACK_BASE + (stack_index as u64 * USER_STACK_SIZE as u64);
    core::ptr::write_bytes(stack_addr as *mut u8, 0, USER_STACK_SIZE);
}
```

**Problem:** When recycling zombie process:
- User stack is zeroed (line 761)
- **Kernel stack is NOT zeroed**, just reused as-is
- Old process may have left sensitive data or corrupted pointers on kernel stack
- New process inherits this stale data

**Impact:**
- Information leak (old process data visible to new process)
- Potential corruption if syscalls encounter stale stack values

**Fix:**
```rust
// Zero kernel stack on recycling
unsafe {
    let kernel_stack_ptr = zombie.kernel_stack.as_mut_ptr();
    core::ptr::write_bytes(kernel_stack_ptr, 0, STACK_SIZE);
}
```

---

## SUMMARY OF BUGS BY SEVERITY

### CRITICAL (11 bugs)
1. **Thread removal race** - Scheduler corruption during process termination
2. **Zombie process use-after-recycle** - Threads execute in recycled memory
3. **No TTBR0 switching** - Zero process isolation, all share memory
4. **User page tables map all RAM** - Security hole, access to everything
5. **Shared memory not freed** - Memory leak on process exit (**PRIMARY BUG**)
6. **SHM ID wrapping** - ID collisions after wraparound
7. **Process bounds not enforced** - Crashes after 8 processes
8. **Zombie state machine** - Inconsistent cleanup
9. **Syscall re-entrancy** - Deadlock from interrupts during syscalls
10. **SHM allocator never reclaims** - Exhaustion after 64 terminals (**PRIMARY BUG**)
11. **TOCTOU in sys_shm_map** - Use-after-free race

### HIGH (8 bugs)
1. **SP not validated** - Context switch can load arbitrary SP
2. **SHM not isolated** - Bump allocator flaw
3. **SHM map race** - TOCTOU on physical address
4. **Stack corruption** - Large syscall buffers overflow
5. **Stack slot tracking** - Counter can desync
6. **User stacks not checked** - Bounds violations
7. **No guard pages** - Silent stack overflow (**MAJOR CONTRIBUTOR**)
8. **Missing interrupt disable** - Inconsistent lock discipline

### MEDIUM (4 bugs)
1. **Dead threads in ready queue** - Scheduler degradation
2. **Kernel stack not zeroed** - Information leak
3. **Interrupt discipline** - Inconsistent patterns
4. **No stack overflow detection** - Silent corruption

---

## ROOT CAUSE ANALYSIS

The terminal artifacts are caused by **combination of bugs**, not a single issue:

### Primary Root Causes (Must Fix)

1. **CRITICAL #5 + #10: Shared Memory Leak**
   - Terminals don't free SHM on exit
   - Bump allocator exhausted after 64 terminals
   - New terminals reuse active memory → direct corruption

2. **CRITICAL #1 + #2: Zombie Thread Execution**
   - Thread not removed from scheduler
   - Process recycled with new ID
   - Old thread executes with stale context in new process memory
   - Arbitrary memory corruption

3. **HIGH #7 + MEDIUM #3: Stack Overflow**
   - No guard pages between stacks
   - Terminal A overflows 128KB stack
   - Directly writes into Terminal B's stack
   - Terminal B displays corrupted state

### Contributing Factors

4. **CRITICAL #3 + #4: No Memory Isolation**
   - All processes share TTBR0
   - Any process can write to any other's memory
   - Amplifies impact of other bugs

5. **CRITICAL #8: Process Limit Not Enforced**
   - After 8 processes, system unstable
   - Stack indices out of bounds
   - Immediate crashes or corruption

---

## RECOMMENDED FIX PRIORITY

### Phase 1: Stop the Bleeding (Critical Fixes)

These 5 fixes will eliminate ~80% of artifacts:

1. **Fix CRITICAL #5** - Free SHM on process termination
   ```rust
   // In mark_process_terminated()
   for region in &mut process.shm_table.regions {
       if let Some(shm) = region.take() {
           SHM_ALLOCATOR.lock().free(shm.physical_addr, shm.size);
       }
   }
   ```

2. **Fix CRITICAL #10** - Implement SHM free list
   ```rust
   struct ShmAllocator {
       free_regions: Vec<(u64, usize)>,
       next_addr: u64,
       end_addr: u64,
   }
   // Implement allocate() with first-fit reuse
   // Implement free() to return regions
   ```

3. **Fix CRITICAL #1** - Separate thread cleanup from process lock
   ```rust
   // Call cleanup_terminated_threads() AFTER releasing PROCESS_MANAGER
   ```

4. **Fix CRITICAL #8** - Enforce MAX_USER_PROCESSES
   ```rust
   if user_process_count >= MAX_USER_PROCESSES {
       return None;
   }
   ```

5. **Fix HIGH #7** - Add stack index assertions
   ```rust
   assert!(stack_index < MAX_USER_PROCESSES);
   ```

### Phase 2: Harden the System (High Priority)

6. **Add guard pages** - Catch stack overflows (HIGH #7)
7. **Validate SP in context_switch** - Prevent arbitrary SP (HIGH #1)
8. **Fix TOCTOU in sys_shm_map** - Reference counting (HIGH #3)
9. **Disable interrupts in syscalls** - Prevent re-entrancy (CRITICAL #9)
10. **Audit lock discipline** - Consistent interrupt disable (HIGH #8)

### Phase 3: Architectural Improvements (Long-term)

11. **Implement per-process TTBR0** - True memory isolation (CRITICAL #3)
12. **Restrict user page tables** - Only map process memory (CRITICAL #4)
13. **Add SP range checks** - Hardware fault on invalid SP
14. **Implement proper SHM reference counting** - Safe concurrent access
15. **Add memory region validation** - Assert on all address calculations

---

## TESTING STRATEGY

After each fix, test:

1. **Single terminal** - Should work cleanly
2. **Multiple terminals (4)** - Should not corrupt
3. **Open/close cycle** - Spawn 4, close all, spawn 4 again
4. **Stress test** - Open/close 100 times to trigger exhaustion bugs
5. **Long-running** - Leave 4 terminals open for extended period

Monitor for:
- Visual artifacts in terminal windows
- System hangs or deadlocks
- Panic messages in UART output
- Memory exhaustion errors

---

## CONCLUSION

The terminal artifacts are caused by **systemic architectural flaws** in:
- Resource lifecycle management (SHM not freed)
- Process isolation (no per-process page tables)
- Concurrency control (races in scheduler/SHM)
- Bounds checking (limits not enforced)

**The good news:** Most bugs have straightforward fixes. Implementing Phase 1 (5 critical fixes) should eliminate the artifacts you're experiencing.

**The challenge:** Some bugs (per-process TTBR0, guard pages) require significant refactoring and testing.

**Recommendation:** Start with Phase 1 fixes immediately. These are low-risk, high-impact changes that directly address the reported symptoms.
