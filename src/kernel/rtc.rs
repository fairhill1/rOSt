// PL031 Real Time Clock Driver
// Based on ARM PrimeCell Real Time Clock (PL031) Technical Reference Manual

extern crate alloc;
use core::ptr;

/// PL031 RTC register offsets
const RTC_DR: usize = 0x000;   // Data Register (current time in seconds since epoch)
const RTC_MR: usize = 0x004;   // Match Register (for alarms)
const RTC_LR: usize = 0x008;   // Load Register (to set time)
const RTC_CR: usize = 0x00C;   // Control Register (enable/disable)

/// PL031 RTC base address on ARM virt machine
const PL031_BASE: usize = 0x09010000;

/// Timezone offset in hours from UTC (CET = UTC+1, no DST)
const TIMEZONE_OFFSET_HOURS: i32 = 1;

/// Read current time from RTC (seconds since Unix epoch)
pub fn read_time() -> u32 {
    unsafe {
        let rtc_base = PL031_BASE as *const u32;
        ptr::read_volatile(rtc_base.add(RTC_DR / 4))
    }
}

/// Set RTC time (seconds since Unix epoch)
#[allow(dead_code)]
pub fn set_time(seconds: u32) {
    unsafe {
        let rtc_base = PL031_BASE as *mut u32;
        ptr::write_volatile(rtc_base.add(RTC_LR / 4), seconds);
    }
}

/// Initialize RTC (enable it)
pub fn init() {
    unsafe {
        let rtc_base = PL031_BASE as *mut u32;
        // Enable RTC by writing 1 to control register
        ptr::write_volatile(rtc_base.add(RTC_CR / 4), 1);
    }
}

/// Simple time structure
#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateTime {
    /// Format as HH:MM string (for display)
    pub fn format_time(&self) -> alloc::string::String {
        alloc::format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// Format as YYYY-MM-DD HH:MM:SS string
    #[allow(dead_code)]
    pub fn format_datetime(&self) -> alloc::string::String {
        alloc::format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day,
            self.hour, self.minute, self.second
        )
    }
}

/// Convert Unix timestamp to DateTime
/// This is a simplified implementation without leap second handling
pub fn timestamp_to_datetime(timestamp: u32) -> DateTime {
    const SECONDS_PER_DAY: u32 = 86400;
    const SECONDS_PER_HOUR: u32 = 3600;
    const SECONDS_PER_MINUTE: u32 = 60;

    // Days since epoch (Jan 1, 1970)
    let mut days = timestamp / SECONDS_PER_DAY;
    let remaining_seconds = timestamp % SECONDS_PER_DAY;

    // Calculate time of day
    let hour = (remaining_seconds / SECONDS_PER_HOUR) as u8;
    let minute = ((remaining_seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE) as u8;
    let second = (remaining_seconds % SECONDS_PER_MINUTE) as u8;

    // Calculate year
    let mut year = 1970u16;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    // Calculate month and day
    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u8;
    for &days_in_month in &days_in_months {
        if days < days_in_month {
            break;
        }
        days -= days_in_month;
        month += 1;
    }

    let day = (days + 1) as u8; // Days are 1-indexed

    DateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get current time as DateTime (with timezone offset applied)
pub fn get_datetime() -> DateTime {
    let timestamp = read_time();
    // Apply timezone offset
    let offset_seconds = TIMEZONE_OFFSET_HOURS * 3600;
    let local_timestamp = if offset_seconds >= 0 {
        timestamp.saturating_add(offset_seconds as u32)
    } else {
        timestamp.saturating_sub((-offset_seconds) as u32)
    };
    timestamp_to_datetime(local_timestamp)
}
