use crate::core::domain::{ComponentEntry, CoordinatorEntry};
use crate::core::ports::DirectoryPort;
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use std::collections::HashMap;
use std::time::Instant;

/// In-memory implementation of the directory port
pub struct InMemoryDirectoryAdapter {
    /// Namespace of the coordinator this adapter belongs to
    namespace: Vec<u8>,
    /// Local components registered with this coordinator
    local_components: HashMap<FullName, ComponentEntry>,
    /// Other coordinators in the network
    coordinators: HashMap<Vec<u8>, CoordinatorEntry>,
}

impl InMemoryDirectoryAdapter {
    /// Create a new in-memory directory adapter
    pub fn new(namespace: Vec<u8>) -> Self {
        Self {
            namespace,
            local_components: HashMap::new(),
            coordinators: HashMap::new(),
        }
    }
}

impl DirectoryPort for InMemoryDirectoryAdapter {
    fn add_local_component(&mut self, component: ComponentEntry) -> Result<(), Error> {
        // Check if component name is already taken
        if self.local_components.contains_key(&component.name) {
            return Err(Error::duplicate_name());
        }

        self.local_components
            .insert(component.name.clone(), component);
        Ok(())
    }

    fn remove_local_component(&mut self, name: FullName) -> Result<Option<ComponentEntry>, Error> {
        Ok(self.local_components.remove(&name))
    }

    fn get_local_component(&self, name: &FullName) -> Option<&ComponentEntry> {
        if name.has_namespace() {
            self.local_components.get(name)
        } else {
            let full_name = FullName::new(self.namespace.clone(), name.name().to_vec());
            self.local_components.get(&full_name)
        }
    }

    fn update_component_last_seen(
        &mut self,
        name: FullName,
        last_seen: Instant,
    ) -> Result<(), Error> {
        if let Some(component) = self.local_components.get_mut(&name) {
            component.last_seen = last_seen;
            Ok(())
        } else {
            Err(Error::not_signed_in())
        }
    }

    fn add_coordinator(&mut self, coordinator: CoordinatorEntry) -> Result<(), Error> {
        self.coordinators
            .insert(coordinator.namespace.clone(), coordinator);
        Ok(())
    }

    fn remove_coordinator(&mut self, namespace: &[u8]) -> Result<Option<CoordinatorEntry>, Error> {
        Ok(self.coordinators.remove(namespace))
    }

    fn get_coordinator(&self, namespace: &[u8]) -> Option<&CoordinatorEntry> {
        self.coordinators.get(namespace)
    }

    fn get_all_local_components(&self) -> Vec<&ComponentEntry> {
        self.local_components.values().collect()
    }

    fn get_all_coordinators(&self) -> Vec<&CoordinatorEntry> {
        self.coordinators.values().collect()
    }
}
