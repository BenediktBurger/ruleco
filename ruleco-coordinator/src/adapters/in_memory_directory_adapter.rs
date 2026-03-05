use crate::core::domain::{ComponentEntry, CoordinatorEntry};
use crate::core::ports::DirectoryPort;
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

/// In-memory implementation of the directory port
pub struct InMemoryDirectoryAdapter {
    /// Namespace of the coordinator this adapter belongs to
    namespace: Vec<u8>,
    /// Local components registered with this coordinator
    local_components: HashMap<FullName, ComponentEntry>,
    /// Other coordinators in the network
    coordinators: HashMap<Vec<u8>, CoordinatorEntry>,
    /// Remote components from other namespaces
    remote_components: HashMap<Vec<u8>, Vec<FullName>>,
}

impl InMemoryDirectoryAdapter {
    /// Create a new in-memory directory adapter
    pub fn new(namespace: Vec<u8>) -> Self {
        Self {
            namespace,
            local_components: HashMap::new(),
            coordinators: HashMap::new(),
            remote_components: HashMap::new(),
        }
    }
}

impl DirectoryPort for InMemoryDirectoryAdapter {
    fn register_component(&mut self, name: FullName, identity: &[u8]) -> Result<(), Error> {
        if self.local_components.contains_key(&name) {
            return Err(Error::duplicate_name_with_data(Value::String(
                name.to_string(),
            )));
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
        if self.coordinators.contains_key(&coordinator.namespace) {
            return Err(Error::duplicate_name_with_data(serde_json::Value::String(
                String::from_utf8_lossy(&coordinator.namespace).to_string(),
            )));
        }

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

    fn add_remote_components(
        &mut self,
        namespace: Vec<u8>,
        components: Vec<FullName>,
    ) -> Result<(), Error> {
        self.remote_components.insert(namespace, components);
        Ok(())
    }

    fn get_remote_components(&self, namespace: &[u8]) -> Result<Vec<FullName>, Error> {
        self.remote_components
            .get(namespace)
            .cloned()
            .ok_or_else(|| Error::node_unknown_with_data(Value::Null))
    }

    fn remove_remote_components(&mut self, namespace: &[u8]) -> Result<(), Error> {
        self.remote_components
            .remove(namespace)
            .ok_or_else(|| Error::node_unknown_with_data(Value::Null))?;
        Ok(())
    }

    fn get_all_global_components(&self) -> Result<HashMap<Vec<u8>, Vec<FullName>>, Error> {
        Ok(self.remote_components.clone())
    }

    fn update_coordinator_last_seen(
        &mut self,
        namespace: &[u8],
        last_seen: Instant,
    ) -> Result<(), Error> {
        if let Some(coordinator) = self.coordinators.get_mut(namespace) {
            coordinator.last_seen = last_seen;
            Ok(())
        } else {
            Err(Error::node_unknown_with_data(Value::Null))
        }
    }

    fn get_all_coordinators_mut(&mut self) -> Vec<&mut CoordinatorEntry> {
        self.coordinators.values_mut().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruleco_core::full_name::FullName;

    #[test]
    fn test_add_remote_components() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace = b"remote_namespace".to_vec();
        let components = vec![
            FullName::new(b"remote_namespace".to_vec(), b"CA".to_vec()),
            FullName::new(b"remote_namespace".to_vec(), b"CB".to_vec()),
        ];

        assert!(adapter
            .add_remote_components(namespace.clone(), components)
            .is_ok());
    }

    #[test]
    fn test_get_remote_components() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace = b"remote_namespace".to_vec();
        let components = vec![
            FullName::new(b"remote_namespace".to_vec(), b"CA".to_vec()),
            FullName::new(b"remote_namespace".to_vec(), b"CB".to_vec()),
        ];

        adapter
            .add_remote_components(namespace.clone(), components.clone())
            .unwrap();

        let retrieved = adapter.get_remote_components(&namespace).unwrap();
        assert_eq!(retrieved.len(), 2);
    }

    #[test]
    fn test_get_remote_components_not_found() {
        let adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let result = adapter.get_remote_components(b"non_existent_namespace");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_remote_components() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace = b"remote_namespace".to_vec();
        let components = vec![FullName::new(b"remote_namespace".to_vec(), b"CA".to_vec())];

        adapter
            .add_remote_components(namespace.clone(), components)
            .unwrap();

        assert!(adapter.remove_remote_components(&namespace).is_ok());
        assert!(adapter.get_remote_components(&namespace).is_err());
    }

    #[test]
    fn test_remove_remote_components_not_found() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let result = adapter.remove_remote_components(b"non_existent_namespace");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_global_components() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace1 = b"remote_namespace1".to_vec();
        let components1 = vec![FullName::new(namespace1.clone(), b"CA".to_vec())];

        let namespace2 = b"remote_namespace2".to_vec();
        let components2 = vec![
            FullName::new(namespace2.clone(), b"CA".to_vec()),
            FullName::new(namespace2.clone(), b"CB".to_vec()),
        ];

        adapter
            .add_remote_components(namespace1.clone(), components1)
            .unwrap();
        adapter
            .add_remote_components(namespace2.clone(), components2)
            .unwrap();

        let all = adapter.get_all_global_components().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key(&namespace1));
        assert!(all.contains_key(&namespace2));
    }

    #[test]
    fn test_overwrite_remote_components() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace = b"remote_namespace".to_vec();
        let components1 = vec![FullName::new(namespace.clone(), b"CA".to_vec())];

        adapter
            .add_remote_components(namespace.clone(), components1)
            .unwrap();

        let components2 = vec![
            FullName::new(namespace.clone(), b"CB".to_vec()),
            FullName::new(namespace.clone(), b"CC".to_vec()),
        ];

        adapter
            .add_remote_components(namespace.clone(), components2.clone())
            .unwrap();

        let retrieved = adapter.get_remote_components(&namespace).unwrap();
        assert_eq!(retrieved.len(), 2);
        assert!(retrieved.iter().any(|c| c.name() == b"CB"));
        assert!(retrieved.iter().any(|c| c.name() == b"CC"));
    }

    #[test]
    fn test_duplicate_coordinator() {
        let mut adapter = InMemoryDirectoryAdapter::new(b"local_namespace".to_vec());

        let namespace = b"coordinator_namespace".to_vec();
        let dealer_identity = b"dealer_id_1".to_vec();

        let coordinator1 = CoordinatorEntry {
            namespace: namespace.clone(),
            dealer_identity: dealer_identity.clone(),
            address: String::new(),
            last_seen: std::time::Instant::now(),
        };

        adapter.register_coordinator(coordinator1).unwrap();

        let coordinator2 = CoordinatorEntry {
            namespace: namespace.clone(),
            dealer_identity: b"dealer_id_2".to_vec(),
            address: String::new(),
            last_seen: std::time::Instant::now(),
        };

        let result = adapter.register_coordinator(coordinator2);
        assert!(result.is_err());
    }
}
