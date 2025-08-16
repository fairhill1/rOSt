use alloc::{vec, vec::Vec};
use core::arch::asm;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

#[repr(C)]
pub struct TaskContext {
    // General purpose registers x0-x30
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
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
    pub x30: u64, // Link register
    
    // Stack pointer and program counter
    pub sp: u64,
    pub pc: u64,
    
    // Processor state
    pub spsr: u64,
}

impl TaskContext {
    pub fn new() -> Self {
        TaskContext {
            x0: 0, x1: 0, x2: 0, x3: 0, x4: 0, x5: 0, x6: 0, x7: 0,
            x8: 0, x9: 0, x10: 0, x11: 0, x12: 0, x13: 0, x14: 0, x15: 0,
            x16: 0, x17: 0, x18: 0, x19: 0, x20: 0, x21: 0, x22: 0, x23: 0,
            x24: 0, x25: 0, x26: 0, x27: 0, x28: 0, x29: 0, x30: 0,
            sp: 0,
            pc: 0,
            spsr: 0x3c5, // EL1h, interrupts disabled
        }
    }
}

pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub state: TaskState,
    pub context: TaskContext,
    pub stack: Vec<u8>,
    pub entry_point: fn(),
}

impl Task {
    pub fn new(id: usize, name: &'static str, entry_point: fn(), stack_size: usize) -> Self {
        let mut stack = vec![0u8; stack_size];
        let stack_top = stack.as_ptr() as u64 + stack_size as u64;
        
        let mut context = TaskContext::new();
        context.sp = stack_top;
        context.pc = entry_point as *const fn() as u64;
        
        Task {
            id,
            name,
            state: TaskState::Ready,
            context,
            stack,
            entry_point,
        }
    }
}

pub struct Scheduler {
    tasks: Vec<Task>,
    current_task: Option<usize>,
    next_task_id: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            tasks: Vec::new(),
            current_task: None,
            next_task_id: 0,
        }
    }
    
    pub fn add_task(&mut self, name: &'static str, entry_point: fn(), stack_size: usize) -> usize {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        
        let task = Task::new(task_id, name, entry_point, stack_size);
        self.tasks.push(task);
        
        crate::uart::Uart::new(0x0900_0000).puts("Task added: ");
        crate::uart::Uart::new(0x0900_0000).puts(name);
        crate::uart::Uart::new(0x0900_0000).puts("\n");
        
        task_id
    }
    
    pub fn schedule(&mut self) -> bool {
        // Simple round-robin scheduler
        if self.tasks.is_empty() {
            return false;
        }
        
        // Find next ready task
        let start_idx = self.current_task.map(|id| {
            self.tasks.iter().position(|t| t.id == id).unwrap_or(0)
        }).unwrap_or(0);
        
        for i in 0..self.tasks.len() {
            let idx = (start_idx + i + 1) % self.tasks.len();
            if self.tasks[idx].state == TaskState::Ready {
                // Mark current task as ready (if it exists and is running)
                if let Some(current_id) = self.current_task {
                    if let Some(current_task) = self.tasks.iter_mut().find(|t| t.id == current_id) {
                        if current_task.state == TaskState::Running {
                            current_task.state = TaskState::Ready;
                        }
                    }
                }
                
                // Switch to new task
                self.tasks[idx].state = TaskState::Running;
                self.current_task = Some(self.tasks[idx].id);
                return true;
            }
        }
        
        false
    }
    
    pub fn get_current_task(&self) -> Option<&Task> {
        if let Some(id) = self.current_task {
            self.tasks.iter().find(|t| t.id == id)
        } else {
            None
        }
    }
    
    pub fn yield_current_task(&mut self) {
        if let Some(id) = self.current_task {
            if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                task.state = TaskState::Ready;
            }
        }
    }
    
    pub fn terminate_current_task(&mut self) {
        if let Some(id) = self.current_task {
            if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
                task.state = TaskState::Terminated;
            }
            self.current_task = None;
        }
    }
    
    pub fn list_tasks(&self) {
        let uart = crate::uart::Uart::new(0x0900_0000);
        uart.puts("Tasks:\n");
        for task in &self.tasks {
            uart.puts("  ");
            uart.put_hex(task.id as u64);
            uart.puts(": ");
            uart.puts(task.name);
            uart.puts(" [");
            match task.state {
                TaskState::Ready => uart.puts("READY"),
                TaskState::Running => uart.puts("RUNNING"),
                TaskState::Blocked => uart.puts("BLOCKED"),
                TaskState::Terminated => uart.puts("TERMINATED"),
            }
            uart.puts("]\n");
        }
    }
}

static mut SCHEDULER: Scheduler = Scheduler::new();

pub fn init() {
    crate::uart::Uart::new(0x0900_0000).puts("Scheduler initialized\n");
}

pub fn add_task(name: &'static str, entry_point: fn(), stack_size: usize) -> usize {
    unsafe {
        SCHEDULER.add_task(name, entry_point, stack_size)
    }
}

pub fn schedule() {
    unsafe {
        SCHEDULER.schedule();
    }
}

pub fn yield_task() {
    unsafe {
        SCHEDULER.yield_current_task();
    }
}

pub fn list_tasks() {
    unsafe {
        SCHEDULER.list_tasks();
    }
}

// Sample tasks for demonstration
pub fn task1() {
    let uart = crate::uart::Uart::new(0x0900_0000);
    let mut counter = 0;
    
    loop {
        uart.puts("Task 1 running (");
        uart.put_hex(counter);
        uart.puts(")\n");
        counter += 1;
        
        // Simulate some work
        for _ in 0..1000000 {
            unsafe { asm!("nop") };
        }
        
        // Cooperatively yield
        yield_task();
        
        if counter > 5 {
            uart.puts("Task 1 terminating\n");
            unsafe { SCHEDULER.terminate_current_task(); }
            break;
        }
    }
}

pub fn task2() {
    let uart = crate::uart::Uart::new(0x0900_0000);
    let mut counter = 0;
    
    loop {
        uart.puts("Task 2 running (");
        uart.put_hex(counter);
        uart.puts(")\n");
        counter += 1;
        
        // Simulate different work pattern
        for _ in 0..500000 {
            unsafe { asm!("nop") };
        }
        
        yield_task();
        
        if counter > 8 {
            uart.puts("Task 2 terminating\n");
            unsafe { SCHEDULER.terminate_current_task(); }
            break;
        }
    }
}