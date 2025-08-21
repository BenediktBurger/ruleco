//! Example of how to test coordinator components with mocked dependencies
//!
//! This example shows how to create mock implementations of the ports used by the coordinator
//! and test the coordinator's behavior without needing real ZMQ sockets or other external dependencies.

use ruleco_coordinator::core::coordinator_core::CoordinatorCore;
use ruleco_coordinator::core::domain::{ComponentEntry, CoordinatorEntry, RoutingDecision};
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
}

impl MockDirectory {
    fn new(namespace: Vec<u8>) -> Self {
        Self {
            namespace,
            local_components: std::collections::HashMap::new(),
            coordinators: std::collections::HashMap::new(),
        }
    }

    fn add_component(&mut self, component: ComponentEntry) {
        self.local_components
            .insert(component.name.clone(), component);
    }
}

impl DirectoryPort for MockDirectory {
    fn add_local_component(&mut self, component: ComponentEntry) -> Result<(), Error> {
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
            // Resolve name without namespace to our namespace + name
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
    use ruleco_coordinator::core::ports::message_receiver_port::Identity;

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
        let core = CoordinatorCore::new(namespace, mock_directory, mock_clock);

        // Create a test message
        let sender_identity = b"component1_identity".to_vec();
        let message = MessageBuilder::new()
            .receiver(FullName::from_slice(b"component1").unwrap())
            .sender(FullName::from_slice(b"test_ns.component1").unwrap())
            .message_type(1)
            .payload_single(b"test content".to_vec())
            .build()
            .unwrap();

        // Test
        let decision = core.route_message(
            &message.to_view().unwrap(),
            &Identity::Local {
                identity: sender_identity,
            },
        );

        // Assertions
        match decision {
            RoutingDecision::Local { target_identity } => {
                assert_eq!(target_identity, b"component1_identity".to_vec());
            }
            _ => panic!("Expected Local routing decision, got {:?}", decision),
        }
    }
}
