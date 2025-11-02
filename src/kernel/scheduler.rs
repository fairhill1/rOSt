/// Round-robin scheduler for rOSt
/// Manages thread scheduling and context switching

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;
use crate::kernel::thread::{Thread, ThreadState, context_switch, jump_to_thread};

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

    /// Spawn a new thread
    pub fn spawn(&mut self, entry_point: fn()) -> usize {
        let id = self.next_thread_id;
        self.next_thread_id += 1;

        let thread = Box::new(Thread::new(id, entry_point));
        self.threads.push(thread);
        self.ready_queue.push_back(id);

        crate::kernel::uart_write_string(&alloc::format!("Spawned thread {}\r\n", id));
        id
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
        if current_id == Some(next_id) {
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
