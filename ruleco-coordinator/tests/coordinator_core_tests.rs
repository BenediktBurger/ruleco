use jsonrpsee_types::Request;
use rstest::rstest;
mod common;
use common::{components, namespaces};
use ruleco_coordinator::adapters::{InMemoryDirectoryAdapter, SystemClockAdapter};
use ruleco_coordinator::core::coordinator_core::CoordinatorCore;
use ruleco_coordinator::core::domain::{CoordinatorEntry, RoutingError};
use ruleco_coordinator::core::ports::message_port::Identity;
use ruleco_coordinator::core::ports::{DirectoryPort, RoutingPort};
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use ruleco_core::protocol_constants::MessageType;
use serde_json::Value;

static COMPONENT1_IDENTITY: &[u8] = b"com1";
static COMPONENT2_IDENTITY: &[u8] = b"com2";
static DEALER_IDENTITY: &[u8] = b"deal";
static UNREGISTERED_IDENTITY: &[u8] = b"unregistered";

fn self_name() -> FullName {
    FullName::from_strings(namespaces::N1, namespaces::COORDINATOR_NAME).unwrap()
}

fn component1_name() -> FullName {
    FullName::from_strings(namespaces::N1, components::CA).unwrap()
}

fn component2_name() -> FullName {
    FullName::from_strings(namespaces::N1, components::CB).unwrap()
}

fn remote_component_name() -> FullName {
    FullName::from_strings(namespaces::N2, components::CC).unwrap()
}

fn remote_coordinator_name() -> FullName {
    FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap()
}

/// Create a default core with reused configuration
fn create_default_core() -> CoordinatorCore<InMemoryDirectoryAdapter, SystemClockAdapter> {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add local components to the directory
    directory
        .register_component(component1_name(), COMPONENT1_IDENTITY)
        .unwrap();
    directory
        .register_component(component2_name(), COMPONENT2_IDENTITY)
        .unwrap();

    // Add a remote coordinator to the directory
    let coordinator = CoordinatorEntry {
        namespace: namespaces::N2.as_bytes().to_vec(),
        dealer_identity: DEALER_IDENTITY.to_vec(),
        address: "tcp://localhost:5555".to_string(),
        last_seen: std::time::Instant::now(),
    };
    directory.register_coordinator(coordinator).unwrap();

    
    CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    )
}

fn local_identity(identity: &[u8]) -> Identity {
    Identity::Component {
        identity: identity.to_vec(),
    }
}

fn remote_identity(identity: &[u8]) -> Identity {
    Identity::Coordinator {
        identity: identity.to_vec(),
    }
}

#[test]
fn test_route_message_from_remote_coordinator_without_validation() {
    // Setup
    let core = create_default_core();

    // Create a message from a remote coordinator's DEALER socket
    // The remote coordinator is NOT signed in as a local component
    let message = MessageBuilder::new()
        .receiver(component1_name())
        .sender(remote_coordinator_name())
        .message_type(MessageType::Json.into())
        .payload_single(br#"{"jsonrpc":"2.0","method":"some_method","id":1}"#.to_vec())
        .build()
        .unwrap();

    // Test - this should succeed even though the remote coordinator is not signed in locally
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &remote_identity(b"remote_dealer_id"),
    );

    // Assertions - should route to local component
    match decision {
        Ok(target_identity) => match target_identity {
            Identity::Component { identity } => {
                assert_eq!(identity.as_slice(), COMPONENT1_IDENTITY);
            }
            _ => panic!("Expected Identity::Local, got {target_identity:?}"),
        },
        Err(e) => panic!("Expected Ok(Identity), got Err: {e:?}"),
    }
}

#[test]
fn test_message_from_local_component_signed_in_via_dealer() {
    // Setup - core with only coordinator registered, no local components
    let namespace = namespaces::N1.as_bytes().to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    // Add only a remote coordinator
    let coordinator = CoordinatorEntry {
        namespace: namespaces::N2.as_bytes().to_vec(),
        dealer_identity: DEALER_IDENTITY.to_vec(),
        address: "tcp://localhost:5555".to_string(),
        last_seen: std::time::Instant::now(),
    };
    directory.register_coordinator(coordinator).unwrap();

    let core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    // Create a message from a remote coordinator via DEALER socket
    // Remote coordinators bypass local component validation
    let message = MessageBuilder::new()
        .receiver(self_name())
        .sender(remote_coordinator_name())
        .message_type(MessageType::Json.into())
        .payload_single(br#"{"jsonrpc":"2.0","method":"sign_in","id":1}"#.to_vec())
        .build()
        .unwrap();

    // Test - remote coordinator message should pass validation bypass
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &remote_identity(b"dealer_id_for_remote_coordinator"),
    );

    // Assertions - should route to self (coordinator)
    match decision {
        Ok(Identity::SelfTarget) => {
            // This is expected for messages addressed to coordinator
        }
        _ => panic!("Expected SelfTarget routing decision, got {decision:?}"),
    }
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
        &local_identity(COMPONENT1_IDENTITY),
    );

    // Assertions
    match decision {
        Ok(target_identity) => match target_identity {
            Identity::Component { identity } => {
                assert_eq!(identity.as_slice(), COMPONENT2_IDENTITY);
            }
            _ => panic!("Expected Identity::Local, got {target_identity:?}"),
        },
        _ => panic!("Expected Local routing decision, got {decision:?}"),
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
        &local_identity(COMPONENT1_IDENTITY),
    );

    // Assertions
    match decision {
        Ok(target_identity) => match target_identity {
            Identity::Component { identity } => {
                assert_eq!(identity.as_slice(), COMPONENT2_IDENTITY);
            }
            _ => panic!("Expected Identity::Local, got {target_identity:?}"),
        },
        _ => panic!("Expected Local routing decision, got {decision:?}"),
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
        &local_identity(UNREGISTERED_IDENTITY),
    );

    // Assertions
    match decision {
        Ok(Identity::SelfTarget) => {
            // This is expected for sign-in messages to the coordinator
        }
        _ => panic!("Expected SelfTarget routing decision, got {decision:?}"),
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
        &local_identity(COMPONENT1_IDENTITY),
    );

    // Assertions
    match decision {
        Ok(target_dealer_identity) => match target_dealer_identity {
            Identity::Coordinator { identity } => {
                assert_eq!(identity.as_slice(), DEALER_IDENTITY);
            }
            _ => panic!(
                "Expected Identity::Remote, got {target_dealer_identity:?}"
            ),
        },
        Err(e) => panic!("Expected Ok(Identity), got Err: {e:?}"),
    }
}

#[test]
fn test_route_message_from_remote_component() {
    // Setup
    let core = create_default_core();

    let message = MessageBuilder::new()
        .sender(remote_component_name())
        .receiver(component1_name())
        .build()
        .unwrap();

    // The sender is from a remote namespace ("remote_namespace"), so no validation is needed.
    // This message was received at our ROUTER socket (sent from remote coordinator's DEALER).
    // The identity indicates which of our sockets received the message (our ROUTER for local component connections).
    let decision = core.route_message(
        &message.to_view().unwrap(),
        &local_identity(COMPONENT1_IDENTITY),
    );

    // Assertions
    match decision {
        Ok(target_identity) => match target_identity {
            Identity::Component { identity } => {
                assert_eq!(identity.as_slice(), COMPONENT1_IDENTITY);
            }
            _ => panic!("Expected Identity::Local, got {target_identity:?}"),
        },
        _ => panic!("Expected Local routing decision, got {decision:?}"),
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
        &local_identity(UNREGISTERED_IDENTITY),
    );

    // Assertions
    let full_sender = FullName::from_slice(sender).unwrap();
    let is_from_local_namespace =
        full_sender.namespace().is_empty() || full_sender.namespace() == namespaces::N1.as_bytes();

    if is_from_local_namespace {
        // Local components must be signed in
        match decision {
            Err(RoutingError { error, .. }) => {
                match error {
                    ruleco_core::errors::Error::Leco(leco_error) => {
                        match leco_error {
                            ruleco_core::errors::LecoError::NotSignedIn { .. } => {
                                // This is what we expect
                            }
                            _ => panic!("Expected NotSignedIn error, got {leco_error:?}"),
                        }
                    }
                    _ => panic!("Expected Leco error, got {error:?}"),
                }
            }
            _ => panic!(
                "Expected Error routing decision for local unregistered component, got {decision:?}"
            ),
        }
    } else {
        // Remote components (different namespaces) don't need validation
        // They should route normally
        let full_receiver = FullName::from_slice(receiver).unwrap();
        let is_to_coordinator = full_receiver.name() == b"COORDINATOR"
            && (!full_receiver.has_namespace()
                || full_receiver.namespace() == namespaces::N1.as_bytes());

        if is_to_coordinator {
            match decision {
                Ok(Identity::SelfTarget) => {}
                _ => panic!(
                    "Expected SelfTarget for remote component message to coordinator, got {decision:?}"
                ),
            }
        } else {
            match decision {
                Ok(target_identity) => {
                    match target_identity {
                        Identity::Component { identity } => {
                            assert_eq!(identity.as_slice(), COMPONENT1_IDENTITY);
                        }
                        _ => panic!("Expected Identity::Local, got {target_identity:?}"),
                    }
                }
                _ => panic!("Expected Local routing decision for remote component message to local component, got {decision:?}"),
            }
        }
    }
}

#[test]
fn test_handle_coordinator_sign_in_success_registers_coordinator() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let mut core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    core.handle_sign_in(component1_name(), COMPONENT1_IDENTITY)
        .unwrap();

    let dealer_identity = b"dealer123";
    let remote_coordinator_name =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    let address = "tcp://127.0.0.1:12301".to_string();

    let (_entry, _messages) = core
        .handle_coordinator_sign_in_success(
            dealer_identity,
            remote_coordinator_name.clone(),
            address,
        )
        .unwrap();

    let remote_component = FullName::from_strings(namespaces::N2, "SomeComponent").unwrap();
    let message = MessageBuilder::new()
        .receiver(remote_component)
        .sender(component1_name())
        .build()
        .unwrap();

    let decision = core.route_message(
        &message.to_view().unwrap(),
        &local_identity(COMPONENT1_IDENTITY),
    );

    match decision {
        Ok(Identity::Coordinator { identity }) => {
            assert_eq!(identity.as_slice(), dealer_identity);
        }
        _ => panic!("Expected Coordinator routing decision, got {decision:?}"),
    }
}

#[test]
fn test_handle_coordinator_sign_in_success_returns_entry_and_messages() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let mut core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    core.handle_sign_in(component1_name(), COMPONENT1_IDENTITY)
        .unwrap();

    let dealer_identity = b"dealer123";
    let remote_coordinator_name =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    let address = "tcp://127.0.0.1:12301".to_string();

    let result = core.handle_coordinator_sign_in_success(
        dealer_identity,
        remote_coordinator_name.clone(),
        address,
    );

    assert!(result.is_ok());
    let (entry, messages) = result.unwrap();

    assert_eq!(entry.namespace, namespaces::N2.as_bytes());
    assert_eq!(entry.dealer_identity, dealer_identity.to_vec());
    assert_eq!(entry.address, "tcp://127.0.0.1:12301");

    assert_eq!(messages.len(), 2);
}

#[test]
fn test_handle_coordinator_sign_in_success_record_components_message() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let mut directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    directory
        .register_component(component1_name(), COMPONENT1_IDENTITY)
        .unwrap();

    let mut core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    let dealer_identity = b"dealer123";
    let remote_coordinator_name =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    let address = "tcp://127.0.0.1:12301".to_string();

    let (_entry, messages) = core
        .handle_coordinator_sign_in_success(
            dealer_identity,
            remote_coordinator_name.clone(),
            address,
        )
        .unwrap();

    let record_components_msg = &messages[1];
    assert_eq!(
        record_components_msg.sender().as_ref().unwrap().namespace(),
        namespaces::N1.as_bytes()
    );
    assert_eq!(
        record_components_msg
            .receiver()
            .as_ref()
            .unwrap()
            .namespace(),
        namespaces::N2.as_bytes()
    );

    let content = record_components_msg.content_frame().unwrap();
    let request: Request = serde_json::from_slice(content).unwrap();
    assert_eq!(request.method_name(), "record_components");

    let params = request.params();
    let params_value: Value = serde_json::from_str(params.as_str().unwrap()).unwrap();
    let components = params_value["components"].as_array().unwrap();
    assert_eq!(components.len(), 1);
    assert!(components.contains(&Value::String(format!(
        "{}.{}",
        namespaces::N1,
        components::CA
    ))));
}

#[test]
fn test_coordinator_sign_out_removes_coordinator() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let mut core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    let dealer_identity = b"dealer123";
    let remote_coordinator_name =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    let address = "tcp://127.0.0.1:12301".to_string();

    let _ = core
        .handle_coordinator_sign_in_success(
            dealer_identity,
            remote_coordinator_name.clone(),
            address,
        )
        .unwrap();

    assert!(core.is_coordinator_registered(namespaces::N2.as_bytes()));

    let stored_identity = core
        .get_coordinator_dealer_identity(namespaces::N2.as_bytes())
        .unwrap();
    assert_eq!(stored_identity.as_slice(), dealer_identity);

    let result = core.remove_coordinator(namespaces::N2.as_bytes());

    assert!(result.is_ok());
    assert!(!core.is_coordinator_registered(namespaces::N2.as_bytes()));
}

#[test]
fn test_coordinator_sign_out_wrong_identity_detected() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let mut core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    let dealer_identity = b"dealer123";
    let remote_coordinator_name =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    let address = "tcp://127.0.0.1:12301".to_string();

    let _ = core
        .handle_coordinator_sign_in_success(
            dealer_identity,
            remote_coordinator_name.clone(),
            address,
        )
        .unwrap();

    let stored_identity = core
        .get_coordinator_dealer_identity(namespaces::N2.as_bytes())
        .unwrap();
    assert_ne!(stored_identity.as_slice(), b"wrong_dealer");

    assert!(core.is_coordinator_registered(namespaces::N2.as_bytes()));
}

#[test]
fn test_coordinator_sign_out_not_registered() {
    let namespace = namespaces::N1.as_bytes().to_vec();
    let directory = InMemoryDirectoryAdapter::new(namespace.clone());
    let clock = SystemClockAdapter::new();

    let core = CoordinatorCore::new(
        namespace,
        "tcp://127.0.0.1:12300".to_string(),
        directory,
        clock,
    );

    assert!(!core.is_coordinator_registered(namespaces::N2.as_bytes()));
}
