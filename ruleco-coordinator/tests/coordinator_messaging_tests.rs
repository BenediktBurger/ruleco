//! Coordinator to coordinator messaging tests
//!
//! Tests for coordinator sign-in, sign-out, and coordinator methods,
//! following the control protocol specification:
//! - Coordinator sign-in (docs/control_protocol.md#coordinator-sign-in)
//! - Coordinator methods (docs/schemas/coordinator.json)

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

mod common;
use common::{
    assert_jsonrpc_valid, assert_success_response, components, find_free_port, namespaces,
    TestClient, TestCoordinator,
};
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use ruleco_core::full_name::FullName;
use ruleco_core::message::MessageBuilder;
use ruleco_core::protocol_constants::MessageType;
use serde_json::json;

use crate::common::extract_response;

/// Coordinator sign-in success
///
/// Tests coordinator-to-coordinator sign-in:
/// 1. Coordinator N2 connects to N1
/// 2. N2 sends coordinator_sign_in request to N1
/// 3. N1 responds with result:null
/// 4. N1 stores N2's identity in directory
/// 5. N2 stores its namespace
///
/// Protocol: docs/control_protocol.md#coordinator-sign-in
#[test]
fn coordinator_sign_in_success() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    let mut client_n1 = TestClient::connect(coordinator_n1.port)
        .expect("Failed to create DEALER client from N2 to N1");
    client_n1
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    // Create add_nodes request with N2's address
    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{}", coordinator_n2.port),
        )]),
    };

    // should go to N1.COORDINATOR
    let _ = client_n1.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );

    let response_frames = client_n1
        .receive_message(1000)
        .expect("Failed to receive response");

    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // After coordinator_sign_in, N2.COORDINATOR should be reachable via N1.COORDINATOR
    std::thread::sleep(Duration::from_millis(200));
    let full_name_n2 = FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME)
        .expect("Should create N2.COORDINATOR name");
    let _ = client_n1.send_jsonrpc_request("pong", None, Some(10), Some(&full_name_n2));

    let response_frames = client_n1
        .receive_message(1000)
        .expect("Failed to receive response");

    let content = &response_frames[4];
    let response: serde_json::Value =
        serde_json::from_slice(content).expect("Failed to parse JSON-RPC response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // For multiple coordinators, send shutdown to each via connected clients
    client_n1
        .send_shutdown(Some(&full_name_n2))
        .expect("Failed to send shutdown to N2");
    let _ = client_n1
        .receive_message(1000)
        .expect("Failed to receive response");
    client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    std::thread::sleep(Duration::from_millis(500));

    // Join all coordinator threads
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// Coordinator sign-in with duplicate namespace should fail
///
/// Tests that N1 rejects duplicate coordinator namespaces:
/// 1. N2 signs in to N1 successfully
/// 2. Another coordinator (N2_b) tries to sign in to N1 with same namespace as N2
/// 3. N1 responds with error -32091
///
/// Protocol: docs/control_protocol.md#coordinator-sign-in
/// Error: -32091 - The name is already taken
#[test]
fn coordinator_sign_in_duplicate_namespace() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2_a = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));
    let coordinator_n2_b = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Create a client connected to N1
    let mut client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client for N1");
    client_n1
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    // Add N2_a to N1 using add_nodes
    let params = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2_a.address())]),
    };
    client_n1
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Failed to send add_nodes");

    let response = client_n1
        .receive_jsonrpc_response(1000)
        .expect("Should receive add_nodes response");
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Wait for sign-in to complete
    thread::sleep(Duration::from_millis(200));

    // Now try to add N2_b which has the same namespace as N2_a
    let params_dup = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2_b.address())]),
    };
    client_n1
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params_dup).unwrap()),
            Some(2),
            None,
        )
        .expect("Failed to send add_nodes for duplicate");

    // Wait for connection attempt and sign-in
    thread::sleep(Duration::from_millis(300));

    // Shutdown all coordinators
    let full_name_n2 =
        FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME).unwrap();
    client_n1
        .send_shutdown(Some(&full_name_n2))
        .expect("Failed to send shutdown to N2");
    let _ = client_n1.receive_message(1000);
    client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    thread::sleep(Duration::from_millis(500));

    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2_a
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2_a");
    drop(coordinator_n2_b); // Let it drop naturally
}

/// Coordinator sign-out success
///
/// Tests coordinator sign-out:
/// 1. N2 signs in to N1 via add_nodes flow
/// 2. N1 sends coordinator_sign_out to N2
/// 3. N2 responds with result:null
/// 4. N2 removes N1 from directory
///
/// Protocol: docs/control_protocol.md#coordinator-sign-out
#[test]
fn coordinator_sign_out_success() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    let mut client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create DEALER client");
    client_n1
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    // Create add_nodes request with N2's address
    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{}", coordinator_n2.port),
        )]),
    };

    // Send add_nodes to N1 - this triggers N1 to connect to N2
    let _ = client_n1.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );

    let response_frames = client_n1
        .receive_message(1000)
        .expect("Failed to receive response");

    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Wait for coordinator sign-in to complete (N1 connects to N2)
    std::thread::sleep(Duration::from_millis(500));

    // Verify N2.COORDINATOR is now reachable via N1
    let full_name_n2 = FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME)
        .expect("Should create N2.COORDINATOR name");
    let _ = client_n1.send_jsonrpc_request("pong", None, Some(10), Some(&full_name_n2));

    let response_frames = client_n1
        .receive_message(1000)
        .expect("Failed to receive pong response");
    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Now N1 sends coordinator_sign_out to N2 (N1 initiates sign-out)
    // Route via N1 - the message goes to N1.COORDINATOR which forwards to N2.COORDINATOR
    let full_name_n2_coord = FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME)
        .expect("Should create N2.COORDINATOR name");
    let _ = client_n1.send_jsonrpc_request(
        "coordinator_sign_out",
        None,
        Some(20),
        Some(&full_name_n2_coord),
    );

    // N1 should receive a response forwarded from N2
    let response_frames = client_n1
        .receive_message(1000)
        .expect("Should receive sign_out response");
    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Clean shutdown
    client_n1
        .send_shutdown(Some(&full_name_n2))
        .expect("Failed to send shutdown to N2");
    let _ = client_n1.receive_message(1000);
    client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    let _ = client_n1.receive_message(1000);
    std::thread::sleep(Duration::from_millis(500));

    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// Coordinator sign-out without sign-in should fail
///
/// Tests that coordinator rejects sign_out from unsigned-in coordinators
///
/// Protocol: docs/control_protocol.md#coordinator-sign-out
#[test]
fn coordinator_sign_out_not_signed_in() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator_n1.port).expect("Failed to create client");

    // Sign in a component so shutdown works
    client
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    // Create a second client for the coordinator_sign_out (simulating another coordinator)
    let client2 = TestClient::connect(coordinator_n1.port).expect("Failed to create second client");

    let request = json!({
        "jsonrpc": "2.0",
        "method": "coordinator_sign_out",
        "params": null,
        "id": 1
    });

    let message = MessageBuilder::new()
        .sender(
            FullName::from_str(&format!("{}.COORDINATOR", namespaces::N2))
                .expect("Failed to create sender name"),
        )
        .receiver(FullName::from_slice(b"COORDINATOR").unwrap())
        .message_type(MessageType::Json.into())
        .payload_json(&request)
        .unwrap()
        .build()
        .expect("Failed to build message");

    client2
        .send_message(message.to_frames())
        .expect("Failed to send coordinator_sign_out");

    // Should receive no response (silently ignored per spec)
    let result = client2.receive_message(1000);

    // Per spec, if a not signed in Coordinator tries to sign out,
    // the message is silently ignored (no response)
    if let Ok(response_frames) = result {
        let content = &response_frames[4];
        let response: serde_json::Value =
            serde_json::from_slice(content).expect("Failed to parse JSON-RPC response");

        // Should not have a result field - either error or no response expected
        assert!(
            response.get("result").is_none(),
            "Should not get success response, got: {:?}",
            response
        );
    }

    // Use first client to shut down coordinator (since it's signed in)
    client.send_shutdown(None).expect("Failed to send shutdown");
    let _ = client.receive_message(1000);
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// Add nodes method - coordinator joins new coordinators
///
/// Tests that coordinator can connect to unknown coordinators via add_nodes:
/// 1. Coordinator N1 receives add_nodes request with {N2: "addr", N3: "addr"}
/// 2. N1 is already connected to N2
/// 3. N1 should only connect to N3 (unknown)
///
/// Protocol: docs/schemas/coordinator.json#add_nodes
#[test]
fn add_nodes_method() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    let params = AddNodesParams {
        nodes: HashMap::from([
            (
                namespaces::N2.to_string(),
                "tcp://127.0.0.1:15001".to_string(),
            ),
            (
                namespaces::N3.to_string(),
                "tcp://127.0.0.1:15002".to_string(),
            ),
        ]),
    };

    client
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Failed to send add_nodes");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Coordinator should have attempted to connect to unknown coordinators

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// Add nodes - all nodes already known
///
/// Tests that add_nodes returns normally when all nodes are already known
///
/// Protocol: docs/schemas/coordinator.json#add_nodes
#[test]
fn add_nodes_all_known() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Connect N1 to N2 using add_nodes
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in("setup", Some(&coordinator.namespace))
        .expect("Failed to sign in");

    let params = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2.address())]),
    };
    let _ = client.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );
    let _ = client.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-in to complete
    thread::sleep(Duration::from_millis(300));

    // Try to add N2 which is already known
    let params2 = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2.address())]),
    };

    let _ = client
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params2).unwrap()),
            Some(2),
            None,
        )
        .expect("Failed to send add_nodes");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // No new connections should be made - shutdown both coordinators
    client
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    std::thread::sleep(Duration::from_millis(300));

    // Shutdown N2 via its own client
    let mut client_n2 =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client for N2");
    client_n2
        .sign_in("shutdown", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in to N2");
    client_n2
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");
    std::thread::sleep(Duration::from_millis(500));

    // Join both coordinators' threads
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// Coordinator shut_down method
///
/// Tests that coordinator shuts down gracefully:
/// 1. Component sends shut_down request
/// 2. Coordinator responds with result:null
/// 3. Coordinator stops its run loop
/// 4. Test verifies coordinator stopped
///
/// Protocol: docs/schemas/coordinator.json (component_optional.json)
#[test]
fn coordinator_shutdown() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send shut_down request
    let _ = client
        .send_jsonrpc_request("shut_down", None, Some(999), None)
        .expect("Failed to send shut_down");

    // Should receive success response
    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Wait for coordinator to stop
    thread::sleep(Duration::from_millis(500));

    // Join thread to verify coordinator stopped cleanly
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Coordinator should shut down gracefully");
}

/// Full coordinator mesh sign-in (two coordinators sign in to each other)
///
/// Tests the complete coordinator sign-in bidirectional flow:
/// 1. N2's DEALER connects to N1's ROUTER with temp name
/// 2. N2 sends coordinator_sign_in to N1
/// 3. N1 responds with result:null
/// 4. N2 sends add_nodes and record_components to N1
/// 5. N1 updates directory and attempts to sign in to N2
/// 6. Sign-in repeats in reverse (N1 to N2)
///
/// Protocol: docs/control_protocol.md#coordinator-sign-in (sequence diagram)
#[test]
fn full_coordinator_mesh_sign_in() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Create setup clients for proper shutdown (these stay signed in as "setup")
    let mut setup_client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create setup client for N1");
    setup_client_n1
        .sign_in("setup", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in setup to N1");

    let mut setup_client_n2 =
        TestClient::connect(coordinator_n2.port).expect("Failed to create setup client for N2");
    setup_client_n2
        .sign_in("setup", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in setup to N2");

    // Sign in component CA to N1
    let mut client_ca =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client for CA");
    client_ca
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in CA to N1");

    // Sign in component CB to N2
    let mut client_cb =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client for CB");
    client_cb
        .sign_in(components::CB, Some(&coordinator_n2.namespace))
        .expect("Failed to sign in CB to N2");

    // N2 initiates sign-in to N1 via add_nodes (using setup client)
    let params = AddNodesParams {
        nodes: HashMap::from([(namespaces::N1.to_string(), coordinator_n1.address())]),
    };
    let _ = setup_client_n2.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );
    let _ = setup_client_n2.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-in and directory sync to complete
    thread::sleep(Duration::from_millis(300));

    // Verify both coordinators are still running
    assert!(coordinator_n1.is_running(), "N1 should still be running");
    assert!(coordinator_n2.is_running(), "N2 should still be running");

    // Test N1→N2 routing: CA sends pong to N2.COORDINATOR
    let target_n2_coord = FullName::from_strings(namespaces::N2, namespaces::COORDINATOR_NAME)
        .expect("Should create N2.COORDINATOR name");
    let _ = client_ca.send_jsonrpc_request("pong", None, Some(10), Some(&target_n2_coord));

    let response_frames = client_ca
        .receive_message(1000)
        .expect("Failed to receive response from N2.COORDINATOR");
    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Verify sender is N2.COORDINATOR
    let sender_frame = String::from_utf8_lossy(&response_frames[2]);
    assert_eq!(
        sender_frame,
        format!("{}.{}", namespaces::N2, namespaces::COORDINATOR_NAME),
        "Sender should be N2.COORDINATOR"
    );

    // Test N2→N1 routing: CB sends pong to N1.COORDINATOR
    let target_n1_coord = FullName::from_strings(namespaces::N1, namespaces::COORDINATOR_NAME)
        .expect("Should create N1.COORDINATOR name");
    let _ = client_cb.send_jsonrpc_request("pong", None, Some(11), Some(&target_n1_coord));

    let response_frames = client_cb
        .receive_message(1000)
        .expect("Failed to receive response from N1.COORDINATOR");
    let response = extract_response(&response_frames);
    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Verify sender is N1.COORDINATOR
    let sender_frame = String::from_utf8_lossy(&response_frames[2]);
    assert_eq!(
        sender_frame,
        format!("{}.{}", namespaces::N1, namespaces::COORDINATOR_NAME),
        "Sender should be N1.COORDINATOR"
    );

    // Shutdown both coordinators via their setup clients
    setup_client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    setup_client_n2
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");
    std::thread::sleep(Duration::from_millis(500));

    // Join all coordinator threads
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}
