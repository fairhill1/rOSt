/// Round-robin scheduler for rOSt
/// Manages thread scheduling and context switching

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use crate::kernel::thread::{Thread, ThreadState, context_switch, jump_to_thread, Process, set_process_main_thread};

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

            crate::kernel::uart_write_string(&alloc::format!("Spawned kernel thread {} for process {}\r\n", thread_id, process_id));
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

            // Create the thread with both kernel and user stacks (no duplicate initialization!)
            let thread = Box::new(Thread::new_user(thread_id, process_id, entry_point, kernel_stack_top, user_stack_top));

            // Debug: get LR before moving the thread
            let debug_lr = thread.context.x30;

            // Link process and thread using safe function
            set_process_main_thread(process_id, thread_id);

            self.threads.push(thread);
            self.ready_queue.push_back(thread_id);

            crate::kernel::uart_write_string(&alloc::format!("Spawned user process {} as thread {}\r\n", process_id, thread_id));
            crate::kernel::uart_write_string(&alloc::format!("DEBUG: Thread {} LR (x30) = 0x{:x}\r\n", thread_id, debug_lr));

            // Note: User process created but not automatically started
            // This avoids the recursive scheduling issue that was causing hangs
            crate::kernel::uart_write_string("DEBUG: User process ready, will run on next scheduler cycle\r\n");

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

        self.ready_queue.pop_front()
    }

    /// Yield CPU to another thread (cooperative)
    pub fn yield_now(&mut self) -> Option<(*mut crate::kernel::thread::ThreadContext, *const crate::kernel::thread::ThreadContext, bool)> {
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
    /// Returns the next thread's context to switch to (without saving current)
    pub fn terminate_current_and_yield(&mut self) -> Option<*const crate::kernel::thread::ThreadContext> {
        if let Some(current_id) = self.current_thread {
            // Mark current thread as terminated (don't add back to ready queue)
            if let Some(thread) = self.threads.iter_mut().find(|t| t.id == current_id) {
                thread.state = ThreadState::Terminated;
                crate::kernel::uart_write_string(&alloc::format!(
                    "[SCHEDULER] Thread {} terminated\r\n", current_id
                ));
            }
            self.current_thread = None;
        }

        // Pick next thread to run
        let next_id = match self.pick_next() {
            Some(id) => id,
            None => {
                // No threads ready - check if we have a kernel context to return to
                if let Some(kernel_ctx) = self.kernel_context {
                    crate::kernel::uart_write_string("[SCHEDULER] No more threads, returning to kernel context (shell)\r\n");
                    return Some(kernel_ctx as *const _);
                }

                crate::kernel::uart_write_string("[SCHEDULER] No more threads and no kernel context\r\n");
                return None;
            }
        };

        // Get next thread and mark it as running
        let next_thread = self.threads.iter_mut().find(|t| t.id == next_id)?;
        next_thread.state = ThreadState::Running;
        self.current_thread = Some(next_id);

        crate::kernel::uart_write_string(&alloc::format!(
            "[SCHEDULER] Switching to thread {} (without saving current)\r\n", next_id
        ));

        // Return pointer to next thread's context (no old context to save)
        Some(&next_thread.context as *const _)
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
                    crate::kernel::uart_write_string("All threads finished, returning to kernel\r\n");

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

        // Debug: Print thread switch info
        crate::kernel::uart_write_string(&alloc::format!("DEBUG: Switching to thread {} (LR=0x{:x})\r\n", next_id, next_thread.context.x30));

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
