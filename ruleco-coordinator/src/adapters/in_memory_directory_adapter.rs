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
    fn register_component(&mut self, name: FullName, identity: &[u8]) -> Result<(), Error> {
        if self.local_components.contains_key(&name) {
            return Err(Error::duplicate_name());
        }

        self.local_components.insert(
            name.clone(),
            ComponentEntry {
                name,
                identity: identity.to_vec(),
                last_seen: std::time::Instant::now(),
            },
        );
        Ok(())
    }

    fn deregister_component(&mut self, name: FullName) -> Result<Option<ComponentEntry>, Error> {
        Ok(self.local_components.remove(&name))
    }

    fn get_component_identity(&self, name: &FullName) -> Result<Vec<u8>, Error> {
        match self.get_local_component(name) {
            Some(entry) => Ok(entry.identity.clone()),
            None => Err(Error::not_signed_in()),
        }
    }

    fn is_component_registered(&self, name: &FullName) -> bool {
        self.local_components.contains_key(name)
    }

    fn register_coordinator(&mut self, coordinator: CoordinatorEntry) -> Result<(), Error> {
        self.coordinators
            .insert(coordinator.namespace.clone(), coordinator);
        Ok(())
    }

    fn deregister_coordinator(
        &mut self,
        namespace: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error> {
        Ok(self.coordinators.remove(namespace))
    }

    fn get_coordinator_dealer_identity(&self, namespace: &[u8]) -> Result<Vec<u8>, Error> {
        match self.get_coordinator(namespace) {
            Some(entry) => Ok(entry.dealer_identity.clone()),
            None => Err(Error::node_unknown_with_data(serde_json::Value::Null)),
        }
    }

    fn is_coordinator_registered(&self, namespace: &[u8]) -> bool {
        self.coordinators.contains_key(namespace)
    }

    // Query methods

    fn get_local_component(&self, name: &FullName) -> Option<&ComponentEntry> {
        if name.has_namespace() {
            self.local_components.get(name)
        } else {
            let full_name = FullName::new(self.namespace.clone(), name.name().to_vec());
            self.local_components.get(&full_name)
        }
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
}
