/// Process and Thread management for rOSt
/// Implements preemptive multitasking with kernel processes and EL0 user processes

use alloc::boxed::Box;
use alloc::vec;
use core::arch::asm;

/// Process states
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ProcessState {
    Created,
    Ready,
    Running,
    Blocked,
    Terminated,
}

/// Process type
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ProcessType {
    Kernel,  // Runs in EL1, kernel space
    User,    // Runs in EL0, user space with MMU protection
}

/// IPC message queue for inter-process communication
pub struct MessageQueue {
    messages: [Option<crate::kernel::syscall::IpcMessage>; crate::kernel::syscall::MAX_MESSAGES_PER_PROCESS],
    read_idx: usize,
    write_idx: usize,
    count: usize,
}

impl MessageQueue {
    pub const fn new() -> Self {
        Self {
            messages: [const { None }; crate::kernel::syscall::MAX_MESSAGES_PER_PROCESS],
            read_idx: 0,
            write_idx: 0,
            count: 0,
        }
    }

    pub fn push(&mut self, msg: crate::kernel::syscall::IpcMessage) -> bool {
        if self.count >= crate::kernel::syscall::MAX_MESSAGES_PER_PROCESS {
            return false; // Queue full
        }
        self.messages[self.write_idx] = Some(msg);
        self.write_idx = (self.write_idx + 1) % crate::kernel::syscall::MAX_MESSAGES_PER_PROCESS;
        self.count += 1;
        true
    }

    pub fn pop(&mut self) -> Option<crate::kernel::syscall::IpcMessage> {
        if self.count == 0 {
            return None; // Queue empty
        }
        let msg = self.messages[self.read_idx];
        self.messages[self.read_idx] = None;
        self.read_idx = (self.read_idx + 1) % crate::kernel::syscall::MAX_MESSAGES_PER_PROCESS;
        self.count -= 1;
        msg
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Shared memory region descriptor
#[derive(Clone, Copy)]
pub struct SharedMemoryRegion {
    pub id: i32,
    pub size: usize,
    pub physical_addr: u64,
    pub virtual_addr: Option<u64>, // Mapped address (if mapped)
    pub ref_count: usize,           // Number of processes using this region
}

/// Shared memory table for a process
pub struct SharedMemoryTable {
    regions: [Option<SharedMemoryRegion>; crate::kernel::syscall::MAX_SHM_REGIONS],
    next_id: i32,
}

impl SharedMemoryTable {
    pub const fn new() -> Self {
        Self {
            regions: [const { None }; crate::kernel::syscall::MAX_SHM_REGIONS],
            next_id: 1,
        }
    }

    pub fn alloc(&mut self, size: usize, physical_addr: u64) -> Option<i32> {
        for slot in self.regions.iter_mut() {
            if slot.is_none() {
                let id = self.next_id;
                self.next_id += 1;
                *slot = Some(SharedMemoryRegion {
                    id,
                    size,
                    physical_addr,
                    virtual_addr: None,
                    ref_count: 1,
                });
                return Some(id);
            }
        }
        None
    }

    pub fn get(&self, id: i32) -> Option<&SharedMemoryRegion> {
        self.regions.iter()
            .filter_map(|r| r.as_ref())
            .find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut SharedMemoryRegion> {
        self.regions.iter_mut()
            .filter_map(|r| r.as_mut())
            .find(|r| r.id == id)
    }

    pub fn remove(&mut self, id: i32) -> bool {
        for slot in self.regions.iter_mut() {
            if let Some(region) = slot {
                if region.id == id {
                    *slot = None;
                    return true;
                }
            }
        }
        false
    }
}

/// Process structure - owns all memory and threads
pub struct Process {
    pub id: usize,
    pub state: ProcessState,
    pub process_type: ProcessType,
    pub user_stack: Option<Box<[u8]>>, // User stack for EL0 processes
    pub kernel_stack: Box<[u8]>,      // Kernel stack for syscalls
    pub main_thread_id: Option<usize>, // Reference to main thread by ID
    pub file_descriptors: crate::kernel::filedesc::FileDescriptorTable, // Open files
    pub socket_descriptors: crate::kernel::syscall::SocketDescriptorTable, // Network sockets
    pub message_queue: MessageQueue,   // IPC message queue
    pub shm_table: SharedMemoryTable,  // Shared memory regions
}

impl Process {
    /// Create a new kernel process
    pub fn new_kernel(id: usize) -> Self {
        // Allocate kernel stack
        let kernel_stack = vec![0u8; STACK_SIZE].into_boxed_slice();

        // DEBUG: Print kernel stack allocation
        // REMOVED: alloc::format!() causes deadlock in syscall context
        // let kernel_stack_ptr = kernel_stack.as_ptr();
        // crate::kernel::uart_write_string(&alloc::format!(
        //     "[PROCESS] Kernel stack for kernel process {}: {:#018x} - {:#018x} ({} KB)\r\n",
        //     id,
        //     kernel_stack_ptr as u64,
        //     kernel_stack_ptr as u64 + STACK_SIZE as u64,
        //     STACK_SIZE / 1024
        // ));
        crate::kernel::uart_write_string("[PROCESS] Kernel stack for kernel process allocated\r\n");

        Process {
            id,
            state: ProcessState::Created,
            process_type: ProcessType::Kernel,
            user_stack: None,
            kernel_stack,
            main_thread_id: None,
            file_descriptors: crate::kernel::filedesc::FileDescriptorTable::new(),
            socket_descriptors: crate::kernel::syscall::SocketDescriptorTable::new(),
            message_queue: MessageQueue::new(),
            shm_table: SharedMemoryTable::new(),
        }
    }

    /// Create a new user process
    pub fn new_user(id: usize) -> Self {
        // Allocate user stack at fixed low physical address (identity-mapped in user page tables)
        // This is critical because user stacks must be accessible with user page tables
        let user_stack = unsafe {
            let idx = NEXT_USER_STACK_INDEX;
            if idx >= MAX_USER_PROCESSES {
                panic!("Too many user processes!");
            }
            NEXT_USER_STACK_INDEX += 1;

            // Calculate address for this process's stack
            let stack_addr = (USER_STACK_BASE + (idx as u64 * USER_STACK_SIZE as u64)) as *mut u8;

            // DEBUG: Print stack allocation
            // REMOVED: alloc::format!() causes deadlock in syscall context
            crate::kernel::uart_write_string("[PROCESS] User stack for process allocated\r\n");

            // Zero the stack memory
            core::ptr::write_bytes(stack_addr, 0, USER_STACK_SIZE);

            // Create a Box from this fixed address (never actually freed)
            Box::from_raw(core::slice::from_raw_parts_mut(stack_addr, USER_STACK_SIZE))
        };

        // Allocate kernel stack for syscalls (from heap is fine - always accessed at EL1)
        let kernel_stack = vec![0u8; STACK_SIZE].into_boxed_slice();

        // DEBUG: Print kernel stack allocation
        // REMOVED: alloc::format!() causes deadlock in syscall context
        crate::kernel::uart_write_string("[PROCESS] Kernel stack for user process allocated\r\n");

        Process {
            id,
            state: ProcessState::Created,
            process_type: ProcessType::User,
            user_stack: Some(user_stack),
            kernel_stack,
            main_thread_id: None,
            file_descriptors: crate::kernel::filedesc::FileDescriptorTable::new(),
            socket_descriptors: crate::kernel::syscall::SocketDescriptorTable::new(),
            message_queue: MessageQueue::new(),
            shm_table: SharedMemoryTable::new(),
        }
    }

    /// Get kernel stack top address
    pub fn get_kernel_stack_top(&self) -> u64 {
        let stack_top = self.kernel_stack.as_ptr() as u64 + self.kernel_stack.len() as u64;
        stack_top & !0xF // 16-byte alignment
    }

    /// Get user stack top address (for EL0 processes)
    /// CRITICAL: Converts kernel virtual address to user-accessible address
    /// by masking off the high-half bits (0xFFFF... → 0x0000...)
    pub fn get_user_stack_top(&self) -> Option<u64> {
        self.user_stack.as_ref().map(|stack| {
            let kernel_virt_addr = stack.as_ptr() as u64 + stack.len() as u64;
            // Convert kernel virtual address (0xFFFF_FF00_xxxx_xxxx) to user physical address
            // by masking to only keep low 48 bits (same as entry point conversion)
            let user_addr = kernel_virt_addr & 0x0000_FFFF_FFFF_FFFF;
            user_addr & !0xF // 16-byte alignment
        })
    }
}

const STACK_SIZE: usize = 512 * 1024; // 512KB per thread (increased for deep GUI call stacks)
const USER_STACK_SIZE: usize = 128 * 1024; // 128KB for user stacks
const MAX_USER_PROCESSES: usize = 8; // Maximum 8 user processes

// User stacks are allocated at fixed low physical address (identity-mapped in page tables)
// CRITICAL: MUST be within actual RAM! QEMU gives us 1GB starting at 0x40000000
// Physical RAM: 0x40000000 to 0x80000000 (1GB)
// IMPORTANT: Avoid 0x50000000 - VirtIO allocates descriptor tables there!
// Use 0x48000000 (128MB into RAM, well within the 1GB limit, doesn't conflict with VirtIO)
const USER_STACK_BASE: u64 = 0x48000000;

static mut NEXT_USER_STACK_INDEX: usize = 0;

/// Thread context - saved/restored during context switch
/// Contains callee-saved registers per ARM64 calling convention
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ThreadContext {
    // Callee-saved registers (x19-x29)
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // Frame pointer
    pub x30: u64, // Link register (saved as part of callee-saved)

    // Stack pointer (saved separately)
    pub sp: u64,  // Stack pointer
}

impl ThreadContext {
    /// Create a new context for a thread entry point
    pub fn new(entry_point: fn(), stack_top: u64) -> Self {
        ThreadContext {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: entry_point as u64, // Thread starts here
            sp: stack_top,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ThreadState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[derive(Debug, Clone, Copy)]
pub enum ThreadType {
    Kernel,
    User,
}

/// Thread execution context - no owned memory, only execution state
pub struct Thread {
    pub id: usize,
    pub process_id: usize,              // Which process owns this thread
    pub thread_type: ThreadType,        // Kernel or User thread
    pub context: ThreadContext,         // Execution context for context switching
    pub state: ThreadState,             // Current thread state
    pub kernel_entry_point: Option<fn()>,   // For kernel threads
    pub user_entry_point: Option<extern "C" fn() -> !>, // For user threads
}

impl Thread {
    /// Create a new kernel thread
    pub fn new_kernel(id: usize, process_id: usize, entry_point: fn(), stack_top: u64) -> Self {
        Thread {
            id,
            process_id,
            thread_type: ThreadType::Kernel,
            context: ThreadContext::new_kernel(entry_point, stack_top),
            state: ThreadState::Ready,
            kernel_entry_point: Some(entry_point),
            user_entry_point: None,
        }
    }

    /// Create a new user thread
    pub fn new_user(id: usize, process_id: usize, entry_point: extern "C" fn() -> !, kernel_stack_top: u64, user_stack_top: u64) -> Self {
        Thread {
            id,
            process_id,
            thread_type: ThreadType::User,
            context: ThreadContext::new_user(entry_point, kernel_stack_top, user_stack_top),
            state: ThreadState::Ready,
            kernel_entry_point: None,
            user_entry_point: Some(entry_point),
        }
    }
}

impl ThreadContext {
    /// Create a new context for a kernel thread entry point
    pub fn new_kernel(entry_point: fn(), stack_top: u64) -> Self {
        ThreadContext {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: entry_point as u64, // Thread starts here
            sp: stack_top,
        }
    }

    /// Create a new context for a user thread entry point
    /// This sets up a fake ExceptionContext that seamlessly transitions to EL0
    pub fn new_user(entry_point: extern "C" fn() -> !, kernel_stack_top: u64, user_stack_top: u64) -> Self {
        // Allocate space for ExceptionContext on the kernel stack
        let exception_context_addr = kernel_stack_top - core::mem::size_of::<crate::kernel::interrupts::ExceptionContext>() as u64;

        // CRITICAL: Convert higher-half kernel address to low-half physical address
        // After higher-half transition, entry_point is like 0xFFFF_FF00_7C6A_5840
        // But EL0 uses TTBR0 with identity mappings: virt 0x7C6A_5840 → phys 0x7C6A_5840
        // So we need to strip off the KERNEL_BASE to get the physical offset
        // KERNEL_BASE = 0xFFFF_FF00_0000_0000, so we keep only bits [39:0]
        let entry_point_high = entry_point as u64;
        let entry_point_low = entry_point_high & 0x0000_00FF_FFFF_FFFF; // Keep only lower 40 bits

        // REMOVED: alloc::format!() causes deadlock in syscall context
        crate::kernel::uart_write_string("[USER-THREAD] Converting entry point\r\n");

        // Create the ExceptionContext that will transition to EL0
        let exception_context = crate::kernel::interrupts::ExceptionContext {
            // General purpose registers start as 0 for security
            x0: 0, x1: 0, x2: 0, x3: 0, x4: 0, x5: 0, x6: 0, x7: 0,
            x8: 0, x9: 0, x10: 0, x11: 0, x12: 0, x13: 0, x14: 0, x15: 0,
            x16: 0, x17: 0, x18: 0, x19: 0, x20: 0, x21: 0, x22: 0, x23: 0,
            x24: 0, x25: 0, x26: 0, x27: 0, x28: 0, x29: user_stack_top, x30: 0, // Store user stack in x29

            // Padding to match assembly layout (8-byte gap after x30)
            _padding: 0,

            // ELR_EL1 = user entry point in low-half (TTBR0-accessible address)
            elr_el1: entry_point_low,

            // SPSR_EL1 = EL0t with interrupts enabled (0x0)
            // Bits: [3:0]=0000 (EL0t), [6]=0 (FIQ enabled), [7]=0 (IRQ enabled), [8]=0 (SError enabled)
            spsr_el1: 0x0,
        };

        // Write the ExceptionContext to the kernel stack
        unsafe {
            let context_ptr = exception_context_addr as *mut crate::kernel::interrupts::ExceptionContext;
            context_ptr.write_volatile(exception_context);
        }

              // Use the higher-half kernel trampoline approach
        // el0_syscall_entry_return must be reachable from high-half kernel addresses
        // CRITICAL: x30 must point to the assembly trampoline, NOT the user entry point!
        // The trampoline will ERET to the user program using the ExceptionContext we created.

        extern "C" {
            fn el0_syscall_entry_return() -> !;
        }

        ThreadContext {
            x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0,
            x25: 0, x26: 0, x27: 0, x28: 0, x29: user_stack_top, // User stack in x29
            x30: el0_syscall_entry_return as u64, // Point to assembly trampoline (higher-half kernel code)
            sp: exception_context_addr, // Point to the ExceptionContext we just created
        }
    }
}

/// Switch from current thread to next thread
///
/// # Safety
/// This function directly manipulates CPU registers and stack pointers.
/// Must only be called by the scheduler with interrupts disabled.
#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(
    _current: *mut ThreadContext,
    _next: *const ThreadContext,
) {
    core::arch::naked_asm!(
        // Save current thread context (callee-saved registers)
        "stp x19, x20, [x0, #0]",
        "stp x21, x22, [x0, #16]",
        "stp x23, x24, [x0, #32]",
        "stp x25, x26, [x0, #48]",
        "stp x27, x28, [x0, #64]",
        "stp x29, x30, [x0, #80]",  // x29 = FP, x30 = LR
        "mov x9, sp",
        "str x9, [x0, #96]",         // Save SP

        // Restore next thread context
        "ldp x19, x20, [x1, #0]",
        "ldp x21, x22, [x1, #16]",
        "ldp x23, x24, [x1, #32]",
        "ldp x25, x26, [x1, #48]",
        "ldp x27, x28, [x1, #64]",
        "ldp x29, x30, [x1, #80]",   // x29 = FP, x30 = LR
        "ldr x9, [x1, #96]",
        "mov sp, x9",                 // Restore SP

        // Return to next thread (jumps to LR)
        "ret",
    )
}

/// Jump to a thread for the first time (no previous context to save)
///
/// # Safety
/// This function directly manipulates CPU registers and stack pointers.
#[unsafe(naked)]
pub unsafe extern "C" fn jump_to_thread(_context: *const ThreadContext) {
    core::arch::naked_asm!(
        // Restore thread context
        "ldp x19, x20, [x0, #0]",
        "ldp x21, x22, [x0, #16]",
        "ldp x23, x24, [x0, #32]",
        "ldp x25, x26, [x0, #48]",
        "ldp x27, x28, [x0, #64]",
        "ldp x29, x30, [x0, #80]",
        "ldr x9, [x0, #96]",
        "mov sp, x9",
        "ret", // Jump to LR (entry point)
    )
}

/// Public API for thread management
use crate::kernel::scheduler::SCHEDULER;

/// Spawn a new kernel thread
pub fn spawn(entry_point: fn()) -> usize {
    SCHEDULER.lock().spawn(entry_point)
}

/// Static context for kernel/shell when yielding from non-thread context
static mut KERNEL_YIELD_CONTEXT: ThreadContext = ThreadContext {
    x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0,
    x25: 0, x26: 0, x27: 0, x28: 0, x29: 0, x30: 0, sp: 0,
};

/// Yield CPU to another thread (cooperative scheduling)
pub fn yield_now() {
    // Get context switch info while holding the lock
    let switch_info = {
        let mut sched = SCHEDULER.lock();

        // If we're not in a thread context (i.e., shell/kernel), set up kernel context
        if sched.current_thread.is_none() {
            unsafe {
                sched.set_kernel_context(&mut KERNEL_YIELD_CONTEXT as *mut _);
            }
        }

        sched.yield_now()
    }; // Lock is dropped here!

    // Now perform context switch outside the lock
    if let Some((current_ptr, next_ptr, is_first)) = switch_info {
        unsafe {
            // If current_ptr is null, use our kernel context
            let actual_current_ptr = if current_ptr.is_null() {
                &mut KERNEL_YIELD_CONTEXT as *mut _
            } else {
                current_ptr
            };

            if is_first {
                // First time running this thread - jump to it (saves our context first)
                context_switch(actual_current_ptr, next_ptr);
            } else {
                context_switch(actual_current_ptr, next_ptr);
            }
        }
    }
}

/// Exit current thread
pub fn exit() -> ! {
    // Mark thread and its process as terminated, then get next switch
    let switch_info = {
        let mut sched = SCHEDULER.lock();
        if let Some(id) = sched.current_thread {
            if let Some(thread) = sched.threads.iter_mut().find(|t| t.id == id) {
                thread.state = ThreadState::Terminated;
                // REMOVED: alloc::format!() causes deadlock in syscall context
                crate::kernel::uart_write_string("Thread exited\r\n");

                // Also terminate the associated process
                let process_id = thread.process_id;
                terminate_process(process_id);
            }
        }
        sched.schedule()
    }; // Lock is dropped here!

    // Perform context switch outside the lock
    if let Some((current_ptr, next_ptr, is_first)) = switch_info {
        unsafe {
            if is_first {
                jump_to_thread(next_ptr);
            } else {
                context_switch(current_ptr, next_ptr);
            }
        }
    }

    // Should never reach here
    loop {
        unsafe { asm!("wfi") }
    }
}

/// Process Manager - handles creation and management of processes
/// ProcessManager owns processes, scheduler owns threads
pub struct ProcessManager {
    processes: alloc::vec::Vec<Process>,
    next_process_id: usize,
}

impl ProcessManager {
    pub fn new() -> Self {
        ProcessManager {
            processes: alloc::vec::Vec::new(),
            next_process_id: 0,
        }
    }

    /// Create a new kernel process
    pub fn create_kernel_process(&mut self) -> usize {
        let process_id = self.next_process_id;
        self.next_process_id += 1;

        let process = Process::new_kernel(process_id);
        self.processes.push(process);

        // REMOVED: alloc::format!() causes deadlock in syscall context
        crate::kernel::uart_write_string("[PROCESS] Created kernel process\r\n");
        process_id
    }

    /// Create a new user process
    pub fn create_user_process(&mut self) -> usize {
        let process_id = self.next_process_id;
        self.next_process_id += 1;

        let process = Process::new_user(process_id);
        self.processes.push(process);

        // REMOVED: alloc::format!() causes deadlock in syscall context
        crate::kernel::uart_write_string("[PROCESS] Created user process\r\n");
        process_id
    }

    /// Get a mutable reference to a process
    pub fn get_process_mut(&mut self, id: usize) -> Option<&mut Process> {
        self.processes.iter_mut().find(|p| p.id == id)
    }

    /// Mark a process as terminated
    pub fn terminate_process(&mut self, id: usize) {
        if let Some(process) = self.get_process_mut(id) {
            process.state = ProcessState::Terminated;
            // REMOVED: alloc::format!() causes deadlock in syscall context
            crate::kernel::uart_write_string("[PROCESS] Process terminated\r\n");
        }
    }

    /// Mark a process as terminated and ready for cleanup
    /// NOTE: We don't actually remove the process from the table here because
    /// the calling thread is still executing on the process's kernel stack!
    /// The process will be cleaned up later by a background task or left as "zombie"
    pub fn mark_process_terminated(&mut self, id: usize) {
        if let Some(process) = self.get_process_mut(id) {
            process.state = ProcessState::Terminated;

            // Log termination
            crate::kernel::uart_write_string("[PROCESS] Process ");
            if id < 10 {
                unsafe {
                    core::ptr::write_volatile(0x09000000 as *mut u8, b'0' + id as u8);
                }
            }
            crate::kernel::uart_write_string(" marked as terminated (zombie)\r\n");

            // TODO: Implement actual cleanup in a background reaper thread
            // For now, the process becomes a "zombie" - marked terminated but not freed
            // This is safe because:
            // 1. The thread is marked Terminated so scheduler won't run it
            // 2. The process is marked Terminated so syscalls won't access it
            // 3. Memory is not freed (small leak but prevents use-after-free)
        }
    }

    /// Set the main thread ID for a process
    pub fn set_process_main_thread(&mut self, process_id: usize, thread_id: usize) {
        if let Some(process) = self.get_process_mut(process_id) {
            process.main_thread_id = Some(thread_id);
            process.state = ProcessState::Ready;
            // REMOVED: alloc::format!() causes deadlock in syscall context
            crate::kernel::uart_write_string("[PROCESS] Process main thread set\r\n");
        }
    }
}

// Global process manager (protected by mutex for memory safety)
use spin::Mutex;
static PROCESS_MANAGER: Mutex<Option<ProcessManager>> = Mutex::new(None);

/// Initialize the process manager
pub fn init_process_manager() {
    *PROCESS_MANAGER.lock() = Some(ProcessManager::new());
    crate::kernel::uart_write_string("[PROCESS] Process manager initialized\r\n");
}

/// Create a user process and return its ID
pub fn create_user_process() -> usize {
    PROCESS_MANAGER.lock()
        .as_mut()
        .map(|pm| pm.create_user_process())
        .unwrap_or(0)
}

/// Create a kernel process and return its ID
pub fn create_kernel_process() -> usize {
    PROCESS_MANAGER.lock()
        .as_mut()
        .map(|pm| pm.create_kernel_process())
        .unwrap_or(0)
}

/// Mark a process as terminated (becomes zombie until reaped)
pub fn mark_process_terminated(id: usize) {
    PROCESS_MANAGER.lock()
        .as_mut()
        .map(|pm| pm.mark_process_terminated(id));
}

/// Get process stack information (safe - returns owned data)
pub fn get_process_stack_info(id: usize) -> Option<(u64, Option<u64>)> {
    PROCESS_MANAGER.lock()
        .as_ref()
        .and_then(|pm| pm.processes.iter().find(|p| p.id == id))
        .map(|p| (p.get_kernel_stack_top(), p.get_user_stack_top()))
}

/// Execute a closure with mutable access to a process (safe - no aliasing)
pub fn with_process_mut<F, R>(id: usize, f: F) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    // CRITICAL: No heap allocations in syscall path!
    let mut guard = PROCESS_MANAGER.lock();

    let result = guard
        .as_mut()
        .and_then(|pm| pm.get_process_mut(id))
        .map(f);

    result
}

/// Search all processes for a shared memory region by ID
/// Returns the physical address if found, None otherwise
/// NOTE: Skips terminated (zombie) processes to avoid reusing freed resources
pub fn find_shared_memory(shm_id: i32) -> Option<u64> {
    let mut pm_lock = PROCESS_MANAGER.lock();
    if let Some(pm) = pm_lock.as_mut() {
        for process in &mut pm.processes {
            // Skip terminated processes (zombies)
            if process.state == ProcessState::Terminated {
                continue;
            }

            if let Some(region) = process.shm_table.get_mut(shm_id) {
                // Found it! Mark as mapped and return physical address
                region.virtual_addr = Some(region.physical_addr);
                return Some(region.physical_addr);
            }
        }
    }
    None
}

/// Set the main thread for a process
pub fn set_process_main_thread(process_id: usize, thread_id: usize) {
    if let Some(pm) = PROCESS_MANAGER.lock().as_mut() {
        pm.set_process_main_thread(process_id, thread_id);
    }
}

/// Terminate a process
pub fn terminate_process(id: usize) {
    if let Some(pm) = PROCESS_MANAGER.lock().as_mut() {
        pm.terminate_process(id);
    }
}
