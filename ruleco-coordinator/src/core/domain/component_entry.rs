use ruleco_core::full_name::FullName;
use std::time::Instant;

/// Represents a component registered with the coordinator
#[derive(Debug, Clone)]
pub struct ComponentEntry {
    /// The component's full name (unique within the node)
    pub name: FullName,
    /// The ZMQ identity for routing messages to this component
    pub identity: Vec<u8>,
    /// Last time we received a message from this component (for timeout tracking)
    pub last_seen: Instant,
}
