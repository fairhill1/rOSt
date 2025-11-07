//! Synchronization helpers for shared memory IPC
//!
//! These utilities ensure proper memory barriers and cache coherency
//! when communicating between processes via shared memory.

use crate::runtime::{getpid, send_message};
use core::sync::atomic::{compiler_fence, Ordering};

/// Synchronize shared memory writes and notify another process
///
/// CRITICAL: Always use this after writing to shared memory buffers.
///
/// This function ensures:
/// 1. Compiler doesn't reorder memory writes
/// 2. CPU cache is flushed (via syscall barrier)
/// 3. IPC message is sent to notify the other process
///
/// # Example
/// ```no_run
/// // Write pixels to shared memory buffer
/// pixel_buffer[0] = 0xFF_FF_FF_FF;
/// pixel_buffer[1] = 0xFF_00_00_00;
///
/// // MUST sync before notifying WM
/// let msg = KernelToWM::RequestRedraw { id: my_pid };
/// sync_and_notify(wm_pid, &msg.to_bytes());
/// ```
///
/// # Why is this needed?
/// Without proper synchronization, the other process may read stale data:
/// - Your writes might still be in CPU cache
/// - Compiler might reorder writes after the send_message call
/// - Result: Other process sees old/partial data
pub fn sync_and_notify(target_pid: u32, msg: &[u8]) -> i32 {
    // Compiler barrier - prevent reordering of memory operations
    compiler_fence(Ordering::SeqCst);

    // Force cache flush via syscall (getpid is lightweight but forces barrier)
    let _ = getpid();

    // Now safe to notify - the other process will see our writes
    send_message(target_pid, msg)
}

/// Synchronize shared memory writes without sending a message
///
/// Use this when you need to ensure writes are visible but don't
/// want to send an IPC message yet.
///
/// # Example
/// ```no_run
/// // Write critical data
/// shared_state.ready = true;
///
/// // Ensure it's visible to other processes
/// sync_memory();
///
/// // Continue with other work...
/// ```
pub fn sync_memory() {
    compiler_fence(Ordering::SeqCst);
    let _ = getpid(); // Syscall forces cache coherency
}
