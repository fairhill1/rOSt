/// Round-robin scheduler for rOSt
/// Manages thread scheduling and context switching

/// Print a number to UART without heap allocations
fn print_number(mut n: usize) {
    if n == 0 {
        crate::kernel::uart_write_string("0");
        return;
    }

    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }

    // Print in reverse (we built it backwards)
    while i > 0 {
        i -= 1;
        let ch = [buf[i]];
        if let Ok(s) = core::str::from_utf8(&ch) {
            crate::kernel::uart_write_string(s);
        }
    }
}

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use crate::kernel::thread::{Thread, ThreadState, ThreadType, context_switch, jump_to_thread, Process, set_process_main_thread};

pub struct Scheduler {
    pub threads: Vec<Box<Thread>>,
    pub ready_queue: VecDeque<usize>, // Thread IDs ready to run
    pub current_thread: Option<usize>,
    next_thread_id: usize,
    kernel_context: Option<*mut crate::kernel::thread::ThreadContext>, // Context to return to when all threads done
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            threads: Vec::new(),
            ready_queue: VecDeque::new(),
            current_thread: None,
            next_thread_id: 1, // 0 reserved for idle thread if needed
            kernel_context: None,
        }
    }

    /// Set the kernel context to return to when all threads finish
    pub fn set_kernel_context(&mut self, ctx: *mut crate::kernel::thread::ThreadContext) {
        self.kernel_context = Some(ctx);
    }

    /// Spawn a new kernel thread (creates a kernel process automatically)
    pub fn spawn(&mut self, entry_point: fn()) -> usize {
        use crate::kernel::thread::{create_kernel_process, set_process_main_thread};

        let process_id = create_kernel_process();
        let thread_id = self.next_thread_id;
        self.next_thread_id += 1;

        // Get the process stack information using the safe API
        if let Some((stack_top, _)) = crate::kernel::thread::get_process_stack_info(process_id) {
            // Create the thread with proper references to process memory
            let thread = Box::new(Thread::new_kernel(thread_id, process_id, entry_point, stack_top));

            // Link process and thread using safe API
            set_process_main_thread(process_id, thread_id);

            self.threads.push(thread);
            self.ready_queue.push_back(thread_id);

            // CRITICAL: No heap allocations during ELF loading (interrupts disabled)
            crate::kernel::uart_write_string("[SCHEDULER] Spawned kernel thread\r\n");
            thread_id
        } else {
            crate::kernel::uart_write_string("[SCHEDULER] Failed to get process for kernel thread\r\n");
            0
        }
    }

    /// Spawn a new user process and add its main thread to the scheduler
    pub fn spawn_user_process(&mut self, entry_point: extern "C" fn() -> !) -> usize {
        use crate::kernel::thread::{create_user_process, set_process_main_thread};

        let process_id = create_user_process();
        let thread_id = self.next_thread_id;
        self.next_thread_id += 1;

        // Get the process stack information using the safe API
        if let Some((kernel_stack_top, user_stack_opt)) = crate::kernel::thread::get_process_stack_info(process_id) {
            let user_stack_top = user_stack_opt.unwrap_or(0);

            // CRITICAL: No heap allocations during ELF loading (interrupts disabled)

            // Create the thread with both kernel and user stacks (no duplicate initialization!)
            let thread = Box::new(Thread::new_user(thread_id, process_id, entry_point, kernel_stack_top, user_stack_top));

            // Link process and thread using safe function
            set_process_main_thread(process_id, thread_id);

            self.threads.push(thread);
            self.ready_queue.push_back(thread_id);

            crate::kernel::uart_write_string("[SCHEDULER] Spawned user process\r\n");

            // Note: User process created but not automatically started
            // This avoids the recursive scheduling issue that was causing hangs
            crate::kernel::uart_write_string("[SCHEDULER] User process ready, will run on next scheduler cycle\r\n");

            process_id
        } else {
            crate::kernel::uart_write_string("[SCHEDULER] Failed to get process for user thread\r\n");
            0
        }
    }

    /// Round-robin: pick next thread from ready queue
    fn pick_next(&mut self) -> Option<usize> {
        // Remove terminated threads from ready queue
        self.ready_queue.retain(|&id| {
            self.threads.iter().any(|t| t.id == id && t.state != ThreadState::Terminated)
        });

        // DEBUG: Print ready queue contents (DISABLED - too spammy)
        // crate::kernel::uart_write_string("[SCHEDULER] Ready queue before pick: [");
        // for (i, &id) in self.ready_queue.iter().enumerate() {
        //     if i > 0 { crate::kernel::uart_write_string(", "); }
        //     // Print the actual thread ID number
        //     print_number(id);
        // }
        // crate::kernel::uart_write_string("]\r\n");

        self.ready_queue.pop_front()
    }

    /// Yield CPU to another thread (cooperative)
    pub fn yield_now(&mut self) -> Option<(*mut crate::kernel::thread::ThreadContext, *const crate::kernel::thread::ThreadContext, bool)> {
        // CRITICAL: No debug output in hot path - causes excessive noise

        if let Some(current_id) = self.current_thread {
            // Mark current thread as ready and add to back of queue
            if let Some(thread) = self.threads.iter_mut().find(|t| t.id == current_id) {
                if thread.state == ThreadState::Running {
                    thread.state = ThreadState::Ready;
                    self.ready_queue.push_back(current_id);
                }
            }
        }

        // Schedule next thread and return pointers for context switch
        self.schedule()
    }

    /// Terminate current thread and yield to next thread
    /// Used when a thread exits - doesn't save current thread context
    /// Returns (next_context, process_id_to_cleanup) tuple
    /// CRITICAL: Caller must clean up the process AFTER releasing scheduler lock!
    pub fn terminate_current_and_yield(&mut self) -> (Option<*const crate::kernel::thread::ThreadContext>, Option<usize>) {
        let process_to_cleanup = if let Some(current_id) = self.current_thread {
            // Get process ID before terminating the thread
            let process_id = self.threads.iter()
                .find(|t| t.id == current_id)
                .map(|t| t.process_id);

            // Mark current thread as terminated (don't add back to ready queue)
            if let Some(thread) = self.threads.iter_mut().find(|t| t.id == current_id) {
                thread.state = ThreadState::Terminated;
                // NOTE: No heap allocations in scheduler path!
            }
            self.current_thread = None;

            process_id
        } else {
            None
        };

        // Pick next thread to run
        let next_id = match self.pick_next() {
            Some(id) => id,
            None => {
                // No threads ready - check if we have a kernel context to return to
                if let Some(kernel_ctx) = self.kernel_context {
                    crate::kernel::uart_write_string("[SCHEDULER] No more threads, returning to kernel context (shell)\r\n");
                    return (Some(kernel_ctx as *const _), process_to_cleanup);
                }

                crate::kernel::uart_write_string("[SCHEDULER] No more threads and no kernel context\r\n");
                return (None, process_to_cleanup);
            }
        };

        // Get next thread and mark it as running
        let next_thread = match self.threads.iter_mut().find(|t| t.id == next_id) {
            Some(t) => t,
            None => return (None, process_to_cleanup),
        };
        next_thread.state = ThreadState::Running;
        self.current_thread = Some(next_id);

        // DEBUG: Print what thread we're switching to
        crate::kernel::uart_write_string("[SCHEDULER] Switching to thread ID ");
        print_number(next_id);
        crate::kernel::uart_write_string(", PID ");
        print_number(next_thread.process_id);
        crate::kernel::uart_write_string("\r\n");

        // Return pointer to next thread's context and process ID to clean up
        (Some(&next_thread.context as *const _), process_to_cleanup)
    }

    /// Preempt current thread (called by timer interrupt)
    pub fn preempt(&mut self) -> Option<(*mut crate::kernel::thread::ThreadContext, *const crate::kernel::thread::ThreadContext, bool)> {
        // Same as yield for round-robin
        self.yield_now()
    }

    /// Core scheduler logic - switch to next thread
    /// Returns pointers for context switch that caller must execute OUTSIDE the lock
    pub fn schedule(&mut self) -> Option<(*mut crate::kernel::thread::ThreadContext, *const crate::kernel::thread::ThreadContext, bool)> {
        let next_id = match self.pick_next() {
            Some(id) => id,
            None => {
                // No threads ready - return to kernel if we have a kernel context
                if let Some(kernel_ctx) = self.kernel_context {
                    // Get current thread context if any
                    let current_ptr = if let Some(id) = self.current_thread {
                        self.threads
                            .iter_mut()
                            .find(|t| t.id == id)
                            .map(|t| &mut t.context as *mut _)
                    } else {
                        None
                    };

                    self.current_thread = None;
                    return Some((current_ptr.unwrap_or(core::ptr::null_mut()), kernel_ctx as *const _, false));
                }

                // No kernel context, nothing to do
                return None;
            }
        };

        let current_id = self.current_thread;

        // Don't switch if already running this thread
        // UNLESS we have a kernel context to yield to (cooperative multitasking)
        if current_id == Some(next_id) {
            // Same thread wants to yield - return to kernel if available
            if let Some(kernel_ctx) = self.kernel_context {
                let current_ptr = self.threads
                    .iter_mut()
                    .find(|t| t.id == next_id)
                    .map(|t| &mut t.context as *mut _)
                    .unwrap_or(core::ptr::null_mut());

                // Mark thread as ready and ADD BACK TO QUEUE (it was removed by pick_next)
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == next_id) {
                    thread.state = ThreadState::Ready;
                }
                self.ready_queue.push_back(next_id); // Critical: re-add to queue!

                self.current_thread = None;
                return Some((current_ptr, kernel_ctx as *const _, false));
            }
            return None;
        }

        // Find current and next threads
        let current_ptr = if let Some(id) = current_id {
            self.threads
                .iter_mut()
                .find(|t| t.id == id)
                .map(|t| &mut t.context as *mut _)
        } else {
            None
        };

        let next_thread = match self.threads.iter_mut().find(|t| t.id == next_id) {
            Some(t) => t,
            None => {
                crate::kernel::uart_write_string("ERROR: Next thread not found!\r\n");
                return None;
            }
        };

        next_thread.state = ThreadState::Running;
        let next_ptr = &next_thread.context as *const _;

        self.current_thread = Some(next_id);

        // Return pointers for context switch (to be done outside lock)
        let is_first_switch = current_ptr.is_none();
        Some((current_ptr.unwrap_or(core::ptr::null_mut()), next_ptr, is_first_switch))
    }

}

// Safe to send between threads - we use Mutex for synchronization
unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

// Global scheduler instance
pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

/// Run pending threads cooperatively (called from main loop)
/// Gives ready threads a time slice and then returns to caller
pub fn run_pending_threads() {
    // Check if there are any ready threads
    let has_ready = {
        let sched = SCHEDULER.lock();
        !sched.ready_queue.is_empty()
    };

    if !has_ready {
        return; // No threads to run
    }

    // Create a context for the main loop so threads can yield back
    let mut main_context = crate::kernel::thread::ThreadContext {
        x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0,
        x25: 0, x26: 0, x27: 0, x28: 0, x29: 0, x30: 0, sp: 0,
    };

    // Register main context so threads can yield back to us
    {
        let mut sched = SCHEDULER.lock();
        sched.set_kernel_context(&mut main_context as *mut _);
    }

    // Get next thread to run
    let switch_info = {
        let mut sched = SCHEDULER.lock();
        sched.schedule()
    };

    // Switch to thread (will save our context and can return here)
    if let Some((current_ptr, next_ptr, is_first)) = switch_info {
        unsafe {
            if is_first || current_ptr.is_null() {
                // Use context_switch anyway to save main_context
                crate::kernel::thread::context_switch(&mut main_context as *mut _, next_ptr);
            } else {
                crate::kernel::thread::context_switch(current_ptr, next_ptr);
            }
        }
    }

    // Thread yielded back to us, clear kernel context
    {
        let mut sched = SCHEDULER.lock();
        sched.kernel_context = None;
    }
}

/// Start the scheduler (free function that handles lock properly)
pub fn start_scheduler() {
    crate::kernel::uart_write_string("Starting scheduler...\r\n");

    // Create a context for the kernel main thread so we can return here
    let mut kernel_context = crate::kernel::thread::ThreadContext {
        x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0,
        x25: 0, x26: 0, x27: 0, x28: 0, x29: 0, x30: 0, sp: 0,
    };

    // Register the kernel context with the scheduler
    {
        let mut sched = SCHEDULER.lock();
        sched.set_kernel_context(&mut kernel_context as *mut _);
    }

    // Get context switch info while holding lock
    let switch_info = {
        let mut sched = SCHEDULER.lock();
        sched.schedule()
    }; // Lock dropped here!

    // Perform first context switch outside the lock
    // Always use context_switch (not jump) so we can return here when done
    if let Some((_current_ptr, next_ptr, _is_first)) = switch_info {
        unsafe {
            // Context switch from kernel to first thread
            // This will save kernel context and when all threads finish, return here
            context_switch(&mut kernel_context as *mut _, next_ptr);
        }
    }

    crate::kernel::uart_write_string("Returned to kernel from scheduler\r\n");
}
