use std::time::Instant;

/// Interface for time-related operations
pub trait ClockPort {
    /// Get the current time
    fn now(&self) -> Instant;
}
