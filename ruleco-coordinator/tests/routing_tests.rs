//! Message routing integration tests
//!
//! Tests for local and cross-coordinator message routing, following the
//! control protocol specification sections:
//! - Communication with the Coordinator (docs/control_protocol.md#communication-with-the-coordinator)
//! - Communication with other Components (docs/control_protocol.md#communication-with-other-components)
//! - Routing errors (docs/control_protocol.md#routing-errors)

mod common;
use std::collections::HashMap;
use std::str::FromStr;

use common::{
    assert_error_data, assert_error_response, assert_jsonrpc_valid, assert_success_response,
    components, find_free_port, namespaces, TestClient, TestCoordinator,
};
use jsonrpsee_types::{Id, Response, ResponsePayload};
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use ruleco_core::errors::LecoError;
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use serde_json::{json, Value};

use crate::common::extract_response;

/// Route message from local component to local component
///
/// Tests basic routing:
/// 1. CA and CB both sign in to coordinator
/// 2. CA sends message to CB
/// 3. Coordinator routes message to CB
/// 4. CB receives the message
///
/// Protocol: docs/control_protocol.md#communication-with-other-components (Example 1)
#[test]
fn route_local_component_to_component() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client CA");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    let mut client_cb = TestClient::connect(coordinator.port).expect("Failed to create client CB");
    client_cb
        .sign_in(components::CB, Some(&coordinator.namespace))
        .expect("Failed to sign in CB");

    // Create protocol message: N1.CA -> N1.CB
    let name_cb = FullName::from_strings(namespaces::N1, components::CB)
        .expect("Failed to create receiver name");

    client_ca
        .send_jsonrpc_request("test", None, Some(5), Some(&name_cb))
        .expect("Failed to send message");

    // CB should receive the message
    let received_frames = client_cb
        .receive_message(1000)
        .expect("Failed to receive message");
    assert!(
        !received_frames.is_empty(),
        "Should receive at least one message"
    );

    // Verify the content
    if received_frames.len() >= 5 {
        let content = String::from_utf8_lossy(&received_frames[4]);
        assert!(
            content.contains("test"),
            "Message content should contain \'test\'"
        );
    }

    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message from component to coordinator
///
/// Tests that component can send messages addressed to coordinator itself
///
/// Protocol: docs/control_protocol.md#communication-with-the-coordinator
#[test]
fn route_component_to_coordinator() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send a message to coordinator
    let response = client.send_jsonrpc_request("pong", None, Some(1), None);
    assert!(response.is_ok(), "Should send message to coordinator");

    let received = client.receive_jsonrpc_response(1000);
    assert!(received.is_ok(), "Should receive response from coordinator");

    let response = received.unwrap();
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message from unregistered component should fail
///
/// Tests that coordinator refuses messages from unsigned-in components:
/// 1. Component has not signed in
/// 2. Component tries to send message
/// 3. Coordinator returns error -32090
///
/// Protocol: docs/control_protocol.md#signing-in
/// Error: -32090 - Component not signed in yet
#[test]
fn route_from_unregistered_component() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    let _ = client.send_jsonrpc_request("test_method", None, Some(1), None);

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive error response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, LecoError::not_signed_in(None).code());
    assert_error_data(&response, "unknown");

    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");
    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message to unknown receiver should fail
///
/// Tests that coordinator returns error for unknown components:
/// 1. CA signs in
/// 2. CA tries to send message to unsigned-in CB
/// 3. Coordinator returns error -32093
///
/// Protocol: docs/control_protocol.md#routing-errors
/// Error: -32093 - Receiver is not in addresses list
#[test]
fn route_to_unknown_receiver() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    let name_cb =
        FullName::from_strings(namespaces::N1, components::CB).expect("Failed to create fullname");
    client_ca
        .send_jsonrpc_request("pong", None, Some(1), Some(&name_cb))
        .expect("Failed to send message to CB");

    // Should receive error response
    let response = client_ca
        .receive_jsonrpc_response(1000)
        .expect("Should receive error response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, LecoError::receiver_unknown(None).code());
    assert_error_data(&response, &format!("{}.{}", namespaces::N1, components::CB));

    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message to unknown node should fail
///
/// Tests that coordinator returns error for unknown namespaces:
/// 1. CA signs in to N1
/// 2. CA tries to send message to N2.CB (unknown namespace)
/// 3. Coordinator returns error -32092
///
/// Protocol: docs/control_protocol.md#routing-errors
/// Error: -32092 - Node is unknown
#[test]
fn route_to_unknown_node() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    // CA tries to send message to unknown namespace N2
    let receiver = FullName::from_strings(namespaces::N2, components::CB)
        .expect("Failed to create receiver name");

    client_ca
        .send_jsonrpc_request("pong", None, Some(5), Some(&receiver))
        .expect("Failed to send message to unknown node");
    // Should receive error response
    let response = client_ca
        .receive_jsonrpc_response(1000)
        .expect("Should receive error response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, LecoError::node_unknown(None).code());
    assert_error_data(&response, namespaces::N2);

    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message via partial name (component name only, no namespace)
///
/// Tests that components can send messages using just component name
/// when the receiver is in the same namespace
///
/// Protocol: docs/control_protocol.md#naming-scheme "receiver may be specified by Component name alone"
#[test]
fn route_message_with_partial_receiver_name() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    let mut client_cb = TestClient::connect(coordinator.port).expect("Failed to create client");
    client_cb
        .sign_in(components::CB, Some(&coordinator.namespace))
        .expect("Failed to sign in CB");

    // Use partial receiver name (just "CB" instead of "N1.CB")
    let receiver = FullName::from_str(components::CB).expect("Failed to create receiver name");

    client_ca
        .send_jsonrpc_request("test_partial_name", None, Some(1), Some(&receiver))
        .expect("Failed to send message with partial receiver name");
    // CB should receive the message
    let received_frames = client_cb
        .receive_message(1000)
        .expect("Failed to receive message");
    assert!(!received_frames.is_empty());

    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route message from remote component via coordinator
///
/// Tests cross-coordinator routing:
/// 1. Coordinator N1 and Coordinator N2 both running
/// 2. Coordinators sign in to each other
/// 3. N1.CA sends message to N2.CB
/// 4. Message flows: CA -> N1.COORDINATOR -> N2.COORDINATOR -> CB
/// 5. CB receives the message and responds back to CA via the same path: CB -> N2.COORDINATOR -> N1.COORDINATOR -> CA
///
/// Protocol: docs/control_protocol.md#communication-with-other-components (Example 2)
#[test]
fn route_cross_coordinator() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    let mut client_ca =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client CA");
    client_ca
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in CA");

    // Create add_nodes request with N2's address
    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{}", coordinator_n2.port),
        )]),
    };

    // should go to N1.COORDINATOR
    client_ca
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Should send add_nodes request");

    let _ = client_ca
        .receive_message(1000)
        .expect("Failed to receive response");

    // Connect client CB to N2 and sign in
    let mut client_cb =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client CB");
    client_cb
        .sign_in(components::CB, Some(&coordinator_n2.namespace))
        .expect("Failed to sign in CB");

    std::thread::sleep(std::time::Duration::from_millis(200));
    // CA sends message to N2.CB
    let name_n2_cb = FullName::from_strings(namespaces::N2, components::CB)
        .expect("Failed to create receiver name");

    let id = 5;
    client_ca
        .send_jsonrpc_request("pong", None, Some(id), Some(&name_n2_cb))
        .expect("Failed to send message to N2.CB");

    // CB should receive the message via both coordinators
    let received_frames = client_cb
        .receive_message(1000)
        .expect("Failed to receive message");
    assert!(!received_frames.is_empty());
    assert_eq!(
        received_frames[2],
        format!("{}.{}", namespaces::N1, components::CA).as_bytes()
    );

    // Check whether the message can be routed back to N1.CA via the same path
    client_cb
        .send_jsonrpc_request(
            "pong",
            None,
            Some(5),
            Some(&FullName::from_strings(namespaces::N1, namespaces::COORDINATOR_NAME).unwrap()),
        )
        .expect("Failed to send response from CB to coordinator");
    let received_frames = client_cb
        .receive_message(1000)
        .expect("Failed to receive response");
    assert!(!received_frames.is_empty());
    let response = extract_response(&received_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);
    assert_eq!(
        received_frames[2],
        format!("{}.{}", namespaces::N1, namespaces::COORDINATOR_NAME).as_bytes()
    );

    // CB responds back to CA
    let name_n1_ca = FullName::from_strings(namespaces::N1, components::CA)
        .expect("Failed to create receiver name for CA");
    let response = Response::new(
        ResponsePayload::Success(std::borrow::Cow::Borrowed(&Value::Null)),
        Id::Number(id),
    );
    let message = MessageBuilder::new()
        .sender(name_n2_cb)
        .receiver(name_n1_ca)
        .payload_json(&response)
        .unwrap()
        .build()
        .unwrap();
    client_cb
        .send_message(message.to_frames())
        .expect("Failed to send response from CB to CA");

    // CA should receive the response
    let received_frames = client_ca
        .receive_message(1000)
        .expect("Failed to receive response");
    let response = extract_response(&received_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);
    assert_eq!(
        received_frames[2],
        format!("{}.{}", namespaces::N2, components::CB).as_bytes()
    );

    // Shutdown both coordinators
    client_ca
        .send_shutdown(Some(
            &FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap(),
        ))
        .expect("Failed to send shutdown");
    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator_n1
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator N1");
    coordinator_n2
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator N2");
}

/// Route message to coordinator sent by partial name
///
/// Tests coordinator routing when receiver is namespaces::COORDINATOR_NAME (partial name)
///
/// Protocol: docs/control_protocol.md#communication-with-the-coordinator
#[test]
fn route_to_coordinator_partial_name() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send message using partial receiver name namespaces::COORDINATOR_NAME
    let _ = client.send_jsonrpc_request("pong", None, Some(1), None);

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}

/// Route batch JSON-RPC requests
///
/// Tests that coordinator can process batch requests
///
/// Protocol: docs/control_protocol.md#message-layer (JSON-RPC batch support)
#[test]
fn route_batch_jsonrpc_requests() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send batch request
    let batch_request = json!([
        {"jsonrpc": "2.0", "method": "pong", "params": null, "id": 1},
        {"jsonrpc": "2.0", "method": "pong", "params": null, "id": 2},
        {"jsonrpc": "2.0", "method": "pong", "params": null, "id": 3}
    ]);

    let frames = vec![
        vec![0x01u8],                                                  // Protocol version
        b"COORDINATOR".to_vec(),                                       // Receiver
        format!("{}.{}", namespaces::N1, components::CA).into_bytes(), // Sender
        client.create_content_header(),                                // Header
        serde_json::to_vec(&batch_request).unwrap(),                   // Payload
    ];

    client
        .send_message(frames)
        .expect("Failed to send batch request");

    // Should receive batch response
    let received_frames = client
        .receive_message(1000)
        .expect("Failed to receive response");
    assert!(received_frames.len() >= 5);

    let content_bytes = &received_frames[4];
    let response: serde_json::Value =
        serde_json::from_slice(content_bytes).expect("Failed to parse JSON-RPC response");

    assert!(response.is_array(), "Response should be an array");
    assert_eq!(
        response.as_array().unwrap().len(),
        3,
        "Should have 3 responses"
    );

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(std::time::Duration::from_millis(500));
    coordinator
        .join_thread(std::time::Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}
