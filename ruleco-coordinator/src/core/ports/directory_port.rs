use crate::core::domain::{ComponentEntry, CoordinatorEntry};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use std::collections::HashMap;
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

    // Global directory methods

    /// Add components from a remote namespace to the global directory
    fn add_remote_components(
        &mut self,
        namespace: Vec<u8>,
        components: Vec<FullName>,
    ) -> Result<(), Error>;

    /// Get all components from a remote namespace
    fn get_remote_components(&self, namespace: &[u8]) -> Result<Vec<FullName>, Error>;

    /// Remove all components from a remote namespace
    fn remove_remote_components(&mut self, namespace: &[u8]) -> Result<(), Error>;

    /// Get all global components across all remote namespaces
    fn get_all_global_components(&self) -> Result<HashMap<Vec<u8>, Vec<FullName>>, Error>;

    /// Update the last seen time for a remote coordinator
    fn update_coordinator_last_seen(
        &mut self,
        namespace: &[u8],
        last_seen: Instant,
    ) -> Result<(), Error>;

    /// Get all coordinators (mutable for updating last_seen)
    fn get_all_coordinators_mut(&mut self) -> Vec<&mut CoordinatorEntry>;
}
