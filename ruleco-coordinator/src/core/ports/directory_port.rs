use crate::core::domain::{ComponentEntry, CoordinatorEntry};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use std::time::Instant;

/// Interface for directory management operations
pub trait DirectoryPort {
    /// Add a component to the local directory
    fn add_local_component(&mut self, component: ComponentEntry) -> Result<(), Error>;

    /// Remove a component from the local directory
    fn remove_local_component(&mut self, name: FullName) -> Result<Option<ComponentEntry>, Error>;

    /// Get a component from the local directory
    fn get_local_component(&self, name: &FullName) -> Option<&ComponentEntry>;

    /// Update the last seen time for a local component
    fn update_component_last_seen(
        &mut self,
        name: FullName,
        last_seen: Instant,
    ) -> Result<(), Error>;

    /// Add or update a coordinator in the global directory
    fn add_coordinator(&mut self, coordinator: CoordinatorEntry) -> Result<(), Error>;

    /// Remove a coordinator from the global directory
    fn remove_coordinator(&mut self, namespace: &[u8]) -> Result<Option<CoordinatorEntry>, Error>;

    /// Get a coordinator from the global directory
    fn get_coordinator(&self, namespace: &[u8]) -> Option<&CoordinatorEntry>;

    /// Get all local components
    fn get_all_local_components(&self) -> Vec<&ComponentEntry>;

    /// Get all coordinators
    fn get_all_coordinators(&self) -> Vec<&CoordinatorEntry>;
}
