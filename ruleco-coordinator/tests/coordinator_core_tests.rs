use jsonrpsee_types::Request;
use rstest::rstest;
use ruleco_coordinator::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use ruleco_coordinator::core::coordinator_core::CoordinatorCore;
use ruleco_coordinator::core::domain::{ComponentEntry, CoordinatorEntry, RoutingDecision};
use ruleco_coordinator::core::ports::message_receiver_port::Identity;
use ruleco_coordinator::core::ports::{DirectoryPort, RoutingPort};
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use ruleco_core::protocol_constants::MessageType;
use std::time::Instant;

static NAMESPACE: &str = "test_namespace";
static REMOTE_NAMESPACE: &str = "remote_namespace";
static COMPONENT1_IDENTITY: &[u8] = b"com1";
static COMPONENT2_IDENTITY: &[u8] = b"com2";
static DEALER_IDENTITY: &[u8] = b"deal";
static UNREGISTERED_IDENTITY: &[u8] = b"unregistered";

fn self_name() -> FullName {
    FullName::from_str(&format!("{}.{}", NAMESPACE, "COORDINATOR")).unwrap()
}

fn component1_name() -> FullName {
    FullName::new(NAMESPACE.as_bytes().to_vec(), b"component1".to_vec())
}

fn component2_name() -> FullName {
    FullName::new(NAMESPACE.as_bytes().to_vec(), b"component2".to_vec())
}

fn remote_component_name() -> FullName {
    FullName::new(
        REMOTE_NAMESPACE.as_bytes().to_vec(),
        b"rem_component".to_vec(),
    )
}

fn _remote_coordinator_name() -> FullName {
    FullName::new(
        REMOTE_NAMESPACE.as_bytes().to_vec(),
        b"COORDINATOR".to_vec(),
    )
}

/// Create a default core with reused configuration
fn create_default_core() -> CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter> {
    let namespace = NAMESPACE.as_bytes().to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add local components to the directory
    let component = ComponentEntry {
        name: component1_name(),
        identity: COMPONENT1_IDENTITY.to_vec(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component).unwrap();
    let component2 = ComponentEntry {
        name: component2_name(),
        identity: COMPONENT2_IDENTITY.to_vec(),
        last_seen: Instant::now(),
    };
    directory.add_local_component(component2).unwrap();

    // Add a remote coordinator to the directory
    let coordinator = CoordinatorEntry {
        namespace: REMOTE_NAMESPACE.as_bytes().to_vec(),
        dealer_identity: DEALER_IDENTITY.to_vec(),
        address: "tcp://localhost:5555".to_string(),
    };
    directory.add_coordinator(coordinator).unwrap();

    let core = CoordinatorCore::new(namespace, directory, clock);
    core
}

#[test]
fn test_route_message_from_local_to_local_component() {
    // Setup
    let core = create_default_core();

    let message = MessageBuilder::new()
        .receiver(component2_name())
        .sender(component1_name())
        .build()
        .unwrap();

    // Test
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &Identity::Local {
            identity: COMPONENT1_IDENTITY.to_vec(),
        },
    );

    // Assertions
    match decision {
        RoutingDecision::Local { target_identity } => {
            assert_eq!(target_identity, COMPONENT2_IDENTITY);
        }
        _ => panic!("Expected Local routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_message_to_local_component_without_namespace_in_receiver() {
    let core = create_default_core();

    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(component2_name().name()).unwrap())
        .sender(component1_name())
        .build()
        .unwrap();

    // Test
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &Identity::Local {
            identity: COMPONENT1_IDENTITY.to_vec(),
        },
    );

    // Assertions
    match decision {
        RoutingDecision::Local { target_identity } => {
            assert_eq!(target_identity, COMPONENT2_IDENTITY);
        }
        _ => panic!("Expected Local routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_sign_in_message_to_coordinator() {
    // Setup
    let core = create_default_core();

    // Create a sign-in message addressed to the coordinator
    // For sign-in messages, the sender is just the component name, not the full name
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(b"COORDINATOR").unwrap())
        .sender(FullName::from_slice(b"new_component").unwrap())
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
            identity: UNREGISTERED_IDENTITY.to_vec(),
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
    let core = create_default_core();

    let message = MessageBuilder::new()
        .receiver(remote_component_name())
        .sender(component1_name())
        .build()
        .unwrap();

    // Test
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &Identity::Local {
            identity: COMPONENT1_IDENTITY.to_vec(),
        },
    );

    // Assertions
    match decision {
        RoutingDecision::Remote {
            target_dealer_identity,
        } => {
            assert_eq!(target_dealer_identity, DEALER_IDENTITY);
        }
        _ => panic!("Expected Remote routing decision, got {:?}", decision),
    }
}

#[test]
fn test_route_message_from_remote_component() {
    //Setup
    let core = create_default_core();

    let message = MessageBuilder::new()
        .sender(remote_component_name())
        .receiver(component1_name())
        .build()
        .unwrap();

    // Test
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &Identity::Remote {
            identity: DEALER_IDENTITY.to_vec(),
        },
    );

    // Assertions
    match decision {
        RoutingDecision::Local { target_identity } => {
            assert_eq!(target_identity, COMPONENT1_IDENTITY)
        }
        _ => panic!("Expected Remote routing decision, got {:?}", decision),
    }
}

#[rstest]
fn test_route_non_sign_in_message_from_unregistered_component_to_coordinator(
    #[values(&self_name().to_vec(), &component1_name().to_vec(), b"COORDINATOR")] receiver: &[u8],
    #[values(
        b"not_registered",
        b"test_ns.not_registered",
        b"other_ns.not_registered"
    )]
    sender: &[u8],
) {
    // Setup
    let core = create_default_core();

    // Create a non-sign-in message addressed to the coordinator from an unregistered component
    let message = MessageBuilder::new()
        .receiver(FullName::from_slice(receiver).unwrap())
        .sender(FullName::from_slice(sender).unwrap())
        .message_type(MessageType::Json.into())
        .payload_single(br#"{"jsonrpc":"2.0","method":"other_method","id":1}"#.to_vec())
        .build()
        .unwrap();

    // Test
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &Identity::Local {
            identity: UNREGISTERED_IDENTITY.to_vec(),
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
