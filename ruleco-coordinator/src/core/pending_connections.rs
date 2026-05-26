//! Module for tracking pending connections to remote coordinators

use ruleco_core::full_name::FullName;
use std::collections::HashMap;

/// Error type for pending connection operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConnectionNotFoundError;

impl std::fmt::Display for PendingConnectionNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pending connection not found")
    }
}

impl std::error::Error for PendingConnectionNotFoundError {}

/// Tracks pending connections to remote coordinators
///
/// This stores the mapping between dealer identities and the information
/// needed to complete the connection process.
pub struct PendingConnections {
    /// Map from dealer identity to connection information
    connections: HashMap<Vec<u8>, PendingConnectionInfo>,
}

/// Information about a pending connection
pub struct PendingConnectionInfo {
    /// The address of the remote coordinator
    pub address: String,
    /// The full name of the remote coordinator (once known)
    pub remote_name: Option<FullName>,
    /// Timestamp when the connection was initiated
    pub initiated_at: std::time::Instant,
}

impl Default for PendingConnections {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingConnections {
    /// Create a new empty pending connections tracker
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Add a new pending connection
    pub fn add_pending_connection(
        &mut self,
        dealer_identity: Vec<u8>,
        address: String,
        clock: &dyn crate::core::ports::ClockPort,
    ) -> &PendingConnectionInfo {
        let info = PendingConnectionInfo {
            address,
            remote_name: None,
            initiated_at: clock.now(),
        };
        self.connections.insert(dealer_identity.clone(), info);
        self.connections.get(&dealer_identity).unwrap()
    }

    /// Get information about a pending connection
    pub fn get_pending_connection(&self, dealer_identity: &[u8]) -> Option<&PendingConnectionInfo> {
        self.connections.get(dealer_identity)
    }

    /// Update the remote name for a pending connection
    pub fn update_remote_name(
        &mut self,
        dealer_identity: &[u8],
        remote_name: FullName,
    ) -> Result<(), PendingConnectionNotFoundError> {
        if let Some(info) = self.connections.get_mut(dealer_identity) {
            info.remote_name = Some(remote_name);
            Ok(())
        } else {
            Err(PendingConnectionNotFoundError)
        }
    }

    /// Remove a completed connection and return its information
    pub fn complete_connection(&mut self, dealer_identity: &[u8]) -> Option<PendingConnectionInfo> {
        self.connections.remove(dealer_identity)
    }

    /// Check for timed out pending connections
    pub fn check_timeouts(
        &mut self,
        timeout_duration: std::time::Duration,
        clock: &dyn crate::core::ports::ClockPort,
    ) -> Vec<Vec<u8>> {
        let now = clock.now();
        let timed_out: Vec<Vec<u8>> = self
            .connections
            .iter()
            .filter(|(_, info)| now.duration_since(info.initiated_at) > timeout_duration)
            .map(|(identity, _)| identity.clone())
            .collect();

        for identity in &timed_out {
            self.connections.remove(identity);
        }

        timed_out
    }

    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    pub fn len(&self) -> usize {
        self.connections.len()
    }
}
