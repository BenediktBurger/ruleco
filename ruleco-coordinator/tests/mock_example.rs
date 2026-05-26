//! Example of how to test coordinator components with mocked dependencies
//!
//! This example shows how to create mock implementations of the ports used by the coordinator
//! and test the coordinator's behavior without needing real ZMQ sockets or other external dependencies.

use ruleco_coordinator::core::coordinator_core::CoordinatorCore;
use ruleco_coordinator::core::domain::{ComponentEntry, CoordinatorEntry};
use ruleco_coordinator::core::ports::message_port::Identity;
use ruleco_coordinator::core::ports::{ClockPort, DirectoryPort, RoutingPort};
use ruleco_core::errors::Error;
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use std::time::Instant;

// For simplicity, we'll create a simple mock directory that implements the trait directly
struct MockDirectory {
    namespace: Vec<u8>,
    local_components: std::collections::HashMap<FullName, ComponentEntry>,
    coordinators: std::collections::HashMap<Vec<u8>, CoordinatorEntry>,
    remote_components: std::collections::HashMap<Vec<u8>, Vec<FullName>>,
}

impl MockDirectory {
    fn new(namespace: Vec<u8>) -> Self {
        Self {
            namespace,
            local_components: std::collections::HashMap::new(),
            coordinators: std::collections::HashMap::new(),
            remote_components: std::collections::HashMap::new(),
        }
    }

    fn add_component(&mut self, component: ComponentEntry) {
        self.local_components
            .insert(component.name.clone(), component);
    }
}

impl DirectoryPort for MockDirectory {
    fn register_component(&mut self, name: FullName, identity: &[u8]) -> Result<(), Error> {
        let component = ComponentEntry {
            name,
            identity: identity.to_vec(),
            last_seen: std::time::Instant::now(),
        };
        self.local_components
            .insert(component.name.clone(), component);
        Ok(())
    }

    fn deregister_component(&mut self, name: FullName) -> Result<Option<ComponentEntry>, Error> {
        Ok(self.local_components.remove(&name))
    }

    fn get_component_identity(&self, name: &FullName) -> Result<Vec<u8>, Error> {
        self.local_components
            .get(name)
            .map(|c| c.identity.clone())
            .ok_or_else(Error::not_signed_in)
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
        dealer_identity: &[u8],
    ) -> Result<Option<CoordinatorEntry>, Error> {
        let namespace_to_remove = self
            .coordinators
            .iter()
            .find(|(_, entry)| entry.dealer_identity == dealer_identity)
            .map(|(ns, _)| ns.clone());
        if let Some(ns) = namespace_to_remove {
            Ok(self.coordinators.remove(&ns))
        } else {
            Ok(None)
        }
    }

    fn get_coordinator_dealer_identity(&self, namespace: &[u8]) -> Result<Vec<u8>, Error> {
        self.coordinators
            .get(namespace)
            .map(|c| c.dealer_identity.clone())
            .ok_or_else(Error::node_unknown)
    }

    fn is_coordinator_registered(&self, dealer_identity: &[u8]) -> bool {
        self.coordinators
            .values()
            .any(|c| c.dealer_identity == dealer_identity)
    }

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
            .ok_or_else(|| Error::node_unknown_with_data(serde_json::Value::Null))
    }

    fn remove_remote_components(&mut self, namespace: &[u8]) -> Result<(), Error> {
        self.remote_components.remove(namespace);
        Ok(())
    }

    fn get_all_global_components(
        &self,
    ) -> Result<std::collections::HashMap<Vec<u8>, Vec<FullName>>, Error> {
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
            Err(Error::node_unknown())
        }
    }

    fn get_all_coordinators_mut(&mut self) -> Vec<&mut CoordinatorEntry> {
        self.coordinators.values_mut().collect()
    }
}

// Mock implementation of the clock port
struct MockClock {
    now: Instant,
}

impl MockClock {
    fn new(now: Instant) -> Self {
        Self { now }
    }
}

impl ClockPort for MockClock {
    fn now(&self) -> Instant {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_with_mocked_ports() {
        // Create mocks
        let mut mock_directory = MockDirectory::new(b"test_ns".to_vec());
        let mock_clock = MockClock::new(Instant::now());

        // Add a component to the mock directory with the full name
        // This simulates what the coordinator would do when a component signs in with just "component1"
        mock_directory.add_component(ComponentEntry {
            name: FullName::from_slice(b"test_ns.component1").unwrap(),
            identity: b"component1_identity".to_vec(),
            last_seen: Instant::now(),
        });

        // Create coordinator with mocked dependencies
        let namespace = b"test_ns".to_vec();
        let core = CoordinatorCore::new(
            namespace,
            "tcp://127.0.0.1:12300".to_string(),
            mock_directory,
            mock_clock,
        );

        // Create a test message
        let message = MessageBuilder::new()
            .receiver(FullName::from_slice(b"test_ns.component1").unwrap())
            .sender(FullName::from_slice(b"test_ns.component1").unwrap())
            .message_type(1)
            .payload_single(b"test content".to_vec())
            .build()
            .unwrap();

        // Test
        let sender_identity = Identity::Component {
            identity: b"component1_identity".to_vec(),
        };
        let decision = core.route_message(&message.to_view().unwrap(), &sender_identity);

        // Assertions
        match decision {
            Ok(target_identity) => match target_identity {
                Identity::Component { identity } => {
                    assert_eq!(identity.as_slice(), b"component1_identity");
                }
                _ => panic!("Expected Identity::Component, got {:?}", target_identity),
            },
            Err(e) => panic!("Expected Ok(Identity), got Err: {:?}", e),
        }
    }
}
