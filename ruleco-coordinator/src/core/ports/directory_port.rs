use crate::core::domain::{ComponentEntry, CoordinatorEntry};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use std::time::Instant;

/// Interface for directory management operations
///
/// The Directory manages the mapping between domain names (FullName)
/// and transport identities (ZMQ identities). This is a domain-level
/// concern - the adapter doesn't know about "N1.CA", only raw identities.
pub trait DirectoryPort {
    // Component registration with identity mapping

    /// Register a component with its ZMQ identity
    ///
    /// This is the domain-level registration. The Core validates
    /// the name before calling this method.
    fn register_component(&mut self, name: FullName, identity: &[u8]) -> Result<(), Error>;

    /// Deregister a component and return its entry
    fn deregister_component(&mut self, name: FullName) -> Result<Option<ComponentEntry>, Error>;

    /// Get the ZMQ identity for a registered component
    fn get_component_identity(&self, name: &FullName) -> Result<Vec<u8>, Error>;

    /// Check if a component is registered
    fn is_component_registered(&self, name: &FullName) -> bool;

    // Coordinator tracking

    /// Register a remote coordinator
    fn register_coordinator(&mut self, coordinator: CoordinatorEntry) -> Result<(), Error>;

    /// Deregister a remote coordinator and return its entry
    fn deregister_coordinator(
        &mut self,
        namespace: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error>;

    /// Get the dealer identity for a registered coordinator
    fn get_coordinator_dealer_identity(&self, namespace: &[u8]) -> Result<Vec<u8>, Error>;

    /// Check if a coordinator is registered
    fn is_coordinator_registered(&self, namespace: &[u8]) -> bool;

    // Query methods (for compatibility and inspection)

    /// Get a component entry from the local directory
    fn get_local_component(&self, name: &FullName) -> Option<&ComponentEntry>;

    /// Get a coordinator entry from the global directory
    fn get_coordinator(&self, namespace: &[u8]) -> Option<&CoordinatorEntry>;

    /// Get all local components
    fn get_all_local_components(&self) -> Vec<&ComponentEntry>;

    /// Get all coordinators
    fn get_all_coordinators(&self) -> Vec<&CoordinatorEntry>;

    /// Update the last seen time for a local component
    fn update_component_last_seen(
        &mut self,
        name: FullName,
        last_seen: Instant,
    ) -> Result<(), Error>;
}
