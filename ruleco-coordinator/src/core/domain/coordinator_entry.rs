use std::time::Instant;

/// Represents another coordinator in the network
#[derive(Debug, Clone)]
pub struct CoordinatorEntry {
    /// The namespace of the remote coordinator
    pub namespace: Vec<u8>,
    /// The ZMQ dealer socket identity for communicating with this coordinator
    pub dealer_identity: Vec<u8>,
    /// The address of the remote coordinator
    pub address: String,
    /// Last time we received a message from this coordinator (for timeout tracking)
    pub last_seen: Instant,
}
