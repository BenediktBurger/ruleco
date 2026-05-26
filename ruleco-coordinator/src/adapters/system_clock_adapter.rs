use crate::core::ports::ClockPort;
use std::time::Instant;

/// System clock implementation of the clock port
pub struct SystemClockAdapter;

impl Default for SystemClockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClockAdapter {
    /// Create a new system clock adapter
    pub fn new() -> Self {
        Self
    }
}

impl ClockPort for SystemClockAdapter {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
