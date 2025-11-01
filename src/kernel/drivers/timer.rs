// ARM Generic Timer for hardware-independent timing

use core::arch::asm;

/// Get current time in microseconds using ARM Generic Timer
pub fn get_time_us() -> u64 {
    let counter: u64;
    let frequency: u64;

    unsafe {
        // Read the counter value (CNTPCT_EL0)
        asm!("mrs {}, cntpct_el0", out(reg) counter);

        // Read the counter frequency (CNTFRQ_EL0)
        asm!("mrs {}, cntfrq_el0", out(reg) frequency);
    }

    // Convert counter ticks to microseconds
    // Formula: (counter * 1_000_000) / frequency
    // To avoid overflow, we can do: counter / (frequency / 1_000_000)
    if frequency > 0 {
        (counter * 1_000_000) / frequency
    } else {
        0
    }
}

/// Get current time in milliseconds
pub fn get_time_ms() -> u64 {
    get_time_us() / 1000
}

/// Simple delay in microseconds (busy wait)
pub fn delay_us(us: u64) {
    let start = get_time_us();
    while get_time_us() - start < us {
        // Busy wait
        unsafe {
            asm!("nop");
        }
    }
}

/// Simple delay in milliseconds (busy wait)
pub fn delay_ms(ms: u64) {
    delay_us(ms * 1000);
}
