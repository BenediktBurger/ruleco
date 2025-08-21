use jsonrpsee_types::Request;
use ruleco_coordinator::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use ruleco_coordinator::core::coordinator_core::CoordinatorCore;
use ruleco_coordinator::core::domain::{ComponentEntry, CoordinatorEntry, RoutingDecision};
use ruleco_coordinator::core::ports::message_receiver_port::Identity;
use ruleco_coordinator::core::ports::{DirectoryPort, RoutingPort};
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use ruleco_core::protocol_constants::MessageType;
use std::time::Instant;

#[test]
fn test_route_message_to_local_component() {
    // Setup
    let namespace = b"test_ns".to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add a local component to the directory
    let component_identity = b"component1_identity".to_vec();
    let component_name = FullName::from_slice(b"test_ns.component1").unwrap();
    let component = ComponentEntry {
        name: component_name.clone(),
        identity: component_identity.clone(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component).unwrap();
    let component2_identity = b"component2_identity".to_vec();
    let component2_name = FullName::from_slice(b"test_ns.component2").unwrap();
    let component2 = ComponentEntry {
        name: component2_name.clone(),
        identity: component2_identity.clone(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component2).unwrap();

    let core = CoordinatorCore::new(namespace, directory, clock);

    // Create a test message
    let sender_identity = b"component1_identity".to_vec();
    let message = MessageBuilder::new()
        .receiver(component2_name)
        .sender(component_name)
        .message_type(MessageType::Json.into())
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
            assert_eq!(target_identity, component2_identity);
        }
        _ => panic!("Expected Local routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_message_to_local_component_without_namespace_in_receiver() {
    // Setup
    let namespace = b"test_ns".to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add a local component to the directory
    let component_identity = b"component1_identity".to_vec();
    let component_name = FullName::from_slice(b"test_ns.component1").unwrap();
    let component = ComponentEntry {
        name: component_name.clone(),
        identity: component_identity.clone(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component).unwrap();
    let component2_identity = b"component2_identity".to_vec();
    let component2_name = FullName::from_slice(b"test_ns.component2").unwrap();
    let component2 = ComponentEntry {
        name: component2_name.clone(),
        identity: component2_identity.clone(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component2).unwrap();

    let core = CoordinatorCore::new(namespace, directory, clock);

    // Create a test message
    let sender_identity = b"component1_identity".to_vec();
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(component2_name.name()).unwrap())
        .sender(component_name)
        .message_type(MessageType::Json.into())
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
            assert_eq!(target_identity, component2_identity);
        }
        _ => panic!("Expected Local routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_sign_in_message_to_coordinator() {
    // Setup
    let namespace = b"test_ns".to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let core = CoordinatorCore::new(namespace, directory, clock);

    // Create a sign-in message addressed to the coordinator
    // For sign-in messages, the sender is just the component name, not the full name
    let sender_identity = b"component1_identity".to_vec();
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(b"COORDINATOR").unwrap())
        .sender(FullName::from_slice(b"component1").unwrap())
        .message_type(MessageType::Json.into())
        .payload_json(&Request::owned(
            "sign_in".into(),
            None,
            jsonrpsee_types::Id::Number(1),
        ))
        .unwrap()
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
        RoutingDecision::SelfTarget => {
            // This is expected for sign-in messages to the coordinator
        }
        _ => panic!("Expected SelfTarget routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_message_to_remote_component() {
    // Setup
    let namespace = b"test_ns".to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add sender component to directory (required for authorization)
    let sender_identity = b"component1_identity".to_vec();
    let sender_name = FullName::from_slice(b"test_ns.component1").unwrap();
    let component = ComponentEntry {
        name: sender_name.clone(),
        identity: sender_identity.clone(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component).unwrap();

    // Add a remote coordinator to the directory
    let remote_namespace = b"remote_ns".to_vec();
    let dealer_identity = b"dealer_identity".to_vec();
    let coordinator = CoordinatorEntry {
        namespace: remote_namespace.clone(),
        dealer_identity: dealer_identity.clone(),
        address: "tcp://localhost:5555".to_string(),
    };
    directory.add_coordinator(coordinator).unwrap();

    let core = CoordinatorCore::new(namespace, directory, clock);

    // Create a test message addressed to a remote component
    // The receiver should be the full name of the remote component
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(b"remote_ns.remote_component").unwrap())
        .sender(sender_name)
        .message_type(MessageType::Json.into())
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
        RoutingDecision::Remote {
            target_dealer_identity,
        } => {
            assert_eq!(target_dealer_identity, dealer_identity);
        }
        _ => panic!("Expected Remote routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_non_sign_in_message_from_unregistered_component() {
    // Setup
    let namespace = b"test_ns".to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let core = CoordinatorCore::new(namespace, directory, clock);

    // Create a non-sign-in message addressed to the coordinator from an unregistered component
    let sender_identity = b"component1_identity".to_vec();
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(b"COORDINATOR").unwrap())
        .sender(FullName::from_slice(b"test_ns.component1").unwrap())
        .message_type(MessageType::Json.into())
        .payload_single(br#"{"jsonrpc":"2.0","method":"other_method","id":1}"#.to_vec())
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
        RoutingDecision::Error { error, .. } => {
            // This is expected - unregistered components can't send non-sign-in messages
            match error {
                ruleco_core::errors::Error::Leco(leco_error) => {
                    match leco_error {
                        ruleco_core::errors::LecoError::NotSignedIn { .. } => {
                            // This is what we expect
                        }
                        _ => panic!("Expected NotSignedIn error, got {:?}", leco_error),
                    }
                }
                _ => panic!("Expected Leco error, got {:?}", error),
            }
        }
        _ => panic!("Expected Error routing decision, got {:?}", decision),
    }
}
