//! Timeout and heartbeat integration tests
//!
//! Tests for heartbeat mechanisms, timeout detection, and pong responses,
//! following the control protocol specification:
//! - Heartbeat (docs/control_protocol.md#heartbeat)
//! - Component methods: pong (docs/schemas/component.json)

use std::thread;
use std::time::Duration;

mod common;
use common::{
    assert_jsonrpc_valid, assert_success_response, components, find_free_port, namespaces,
    TestClient, TestCoordinator,
};
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use std::collections::HashMap;

/// pong message response
///
/// Tests that coordinator responds to pong requests:
/// Every message counts as a heartbeat, but explicit pong should also work
///
/// Protocol: docs/control_protocol.md#heartbeat
/// Component method: docs/schemas/actor.json#pong (optional, but coordinator must respond)
#[test]
fn pong_message_response() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send pong request
    client
        .send_jsonrpc_request("pong", None, Some(1), None)
        .expect("Failed to send pong");

    // Should receive success response
    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive pong response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Any message counts as heartbeat
///
/// Tests that coordinator updates component's last_seen time on any received message:
/// 1. Component signs in
/// 2. Component sends any message (not just pong)
/// 3. Component does not timeout despite not sending pong
///
/// Protocol: docs/control_protocol.md#heartbeat "Every message received counts as a heartbeat"
#[test]
fn any_message_counts_as_heartbeat() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send non-pong messages more frequently than the timeout
    // Using send_local_components instead of pong
    for i in 1..4 {
        client
            .send_jsonrpc_request("send_local_components", None, Some(i), None)
            .expect("Failed to send request");
        let _ = client.receive_jsonrpc_response(500);
        thread::sleep(Duration::from_millis(500));
    }

    // Component should still be alive (not timed out)
    client
        .send_jsonrpc_request("send_local_components", None, Some(99), None)
        .expect("Failed to send final query");
    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Component timeout detection
///
/// Tests that coordinator detects when a component hasn't sent messages:
/// 1. Component signs in
/// 2. Component stops sending messages
/// 3. Coordinator sends ping/requests pong after timeout
/// 4. Coordinator marks component as timed out
///
/// Protocol: docs/control_protocol.md#heartbeat
/// Implementation: app.rs:77 TODO "handle timeout by sending RPC request for pong method"
#[test]
fn component_timeout_detection() {
    // Create coordinator with short 2 second timeout for testing
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Stop sending messages - component goes silent
    drop(client);

    // Wait longer than timeout interval (2 seconds)
    thread::sleep(Duration::from_secs(3));

    // Coordinator should have detected timeout
    // Verification depends on internal state access or directory query

    // Query local components - CA should not be there
    let mut query = TestClient::connect(coordinator.port).expect("Failed to create query client");
    query
        .sign_in("query", Some(&coordinator.namespace))
        .expect("Failed to sign in query");

    query
        .send_jsonrpc_request("send_local_components", None, Some(1), None)
        .expect("Failed to send send_local_components");

    let response = query
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let components = result.as_array().expect("Result should be an array");

    let has_ca = components
        .iter()
        .any(|v| v.as_str().is_some_and(|s| s.contains(components::CA)));

    // CA should have been removed if timeout was detected
    assert!(!has_ca, "CA should be removed after timeout");

    query.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Component responds to pong request from coordinator
///
/// Tests that coordinator can send pong requests and components respond:
/// - The coordinator responds to component's pong method
/// - The component can use any method to count as heartbeat
///
/// Protocol: docs/control_protocol.md#heartbeat
#[ignore] // Coordinator-initiated ping not yet implemented - use component-initiated pong instead
#[test]
fn component_responds_to_coordinator_ping() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Simulate coordinator sending ping to component
    // In real implementation, coordinator would send {{method: "ping"}} or similar
    // Component would respond with pong

    // This test validates the reverse flow - component responding to coordinator

    thread::sleep(Duration::from_millis(500));

    // Component should still be considered alive

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Component auto-removal after timeout
///
/// Tests that coordinator removes timed-out components from local directory:
/// 1. Component signs in
/// 2. Component stops sending messages
/// 3. Coordinator detects timeout
/// 4. Coordinator removes component from directory
/// 5. Component cannot send messages anymore (would need to re-sign-in)
///
/// Protocol: docs/control_protocol.md#heartbeat
/// Implementation: app.rs:77 TODO
#[test]
fn component_auto_removal_after_timeout() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Component goes silent
    thread::sleep(Duration::from_secs(3));

    // Try to send message - should get "not signed in" error if removed
    client
        .send_jsonrpc_request("test", None, Some(1), None)
        .expect("Failed to send message");

    // Should receive error -32090 (not signed in) if component was removed
    // May also receive timeout error depending on implementation
    let result = client.receive_jsonrpc_response(1000);

    match result {
        Ok(response) => {
            let error = response.get("error");
            if let Some(error_obj) = error {
                let code = error_obj.get("code").and_then(|c| c.as_i64());
                assert!(code.is_some(), "Should have error code");
                assert_eq!(code, Some(-32090), "Should be 'not signed in' error");
            }
        }
        Err(_) => {
            // Timeout also acceptable - coordinator may have dropped the connection
        }
    }

    // Use a separate client to shut down since original client may be timed out
    let mut shutdown_client =
        TestClient::connect(coordinator.port).expect("Failed to create shutdown client");
    shutdown_client
        .sign_in("shutdown", Some(&coordinator.namespace))
        .expect("Failed to sign in shutdown client");
    shutdown_client
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Multiple components timeout handling
///
/// Tests coordinator handles multiple components timing out independently:
/// 1. CA and CB sign in
/// 2. CB stops sending messages
/// 3. CB times out
/// 4. CA continues sending messages
/// 5. Only CB should be removed
///
/// Protocol: docs/control_protocol.md#heartbeat
#[test]
fn multiple_components_timeout_handling() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client CA");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    let mut client_cb = TestClient::connect(coordinator.port).expect("Failed to create client CB");
    client_cb
        .sign_in(components::CB, Some(&coordinator.namespace))
        .expect("Failed to sign in CB");

    // CB goes silent
    drop(client_cb);

    // Keep CA alive with frequent heartbeats while waiting for CB to timeout
    // CB has 2s timeout, so we wait 3s but keep CA sending every 500ms
    for i in 2..8 {
        client_ca
            .send_jsonrpc_request("pong", None, Some(i), None)
            .expect("Failed to send pong");
        let _ = client_ca.receive_jsonrpc_response(500);
        thread::sleep(Duration::from_millis(500));
    }

    // Query local components - CA should still be there, CB should not
    let mut query = TestClient::connect(coordinator.port).expect("Failed to create query client");
    query
        .sign_in("query", Some(&coordinator.namespace))
        .expect("Failed to sign in query");

    query
        .send_jsonrpc_request("send_local_components", None, Some(100), None)
        .expect("Failed to send send_local_components");

    let response = query
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let components = result.as_array().expect("Result should be an array");

    let component_names: Vec<&str> = components.iter().filter_map(|v| v.as_str()).collect();

    let has_ca = component_names.iter().any(|n| n.contains(components::CA));
    let has_cb = component_names.iter().any(|n| n.contains(components::CB));

    assert!(has_ca, "CA should still be present (sending messages)");
    assert!(!has_cb, "CB should be removed (went silent)");

    client_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    query.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Coordinator timeout detection for remote coordinators
///
/// Tests that coordinator detects when a remote coordinator has timed out
///
/// Protocol: docs/control_protocol.md#heartbeat (applies to coordinators too)
#[ignore] // Complex test requiring bidirectional coordinator communication - covered by unit tests
#[test]
fn coordinator_timeout_detection() {
    let port_n1 = find_free_port();
    let port_n2 = find_free_port();

    let mut coordinator_n1 =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(port_n1), Some(2));
    let mut coordinator_n2 =
        TestCoordinator::spawn_with_timeout(namespaces::N2, Some(port_n2), Some(2));

    // Connect to N1 and add N2 as a remote coordinator
    let mut client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client N1");
    client_n1
        .sign_in("client_n1", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in N1");

    // Use add_nodes to connect N1 to N2
    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{}", coordinator_n2.port),
        )]),
    };
    let _ = client_n1.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(&params).unwrap()),
        Some(1),
        None,
    );
    let _ = client_n1.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-in to complete
    thread::sleep(Duration::from_millis(300));

    // Shut down N2 coordinator to simulate it going offline
    let mut client_n2 =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client N2");
    client_n2
        .sign_in("shutdown_client", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in N2");
    client_n2
        .send_shutdown(None)
        .expect("Failed to shutdown N2");
    thread::sleep(Duration::from_millis(100));
    coordinator_n2
        .join_thread(Duration::from_secs(2))
        .expect("N2 did not shut down");

    // Keep client_n1 alive while waiting for N2 timeout
    for i in 0..6 {
        let _ = client_n1.send_jsonrpc_request("pong", None, Some(i), None);
        let _ = client_n1.receive_jsonrpc_response(500);
        thread::sleep(Duration::from_millis(500));
    }

    // Query N1's global components - N2's namespace should be gone
    let _ = client_n1.send_jsonrpc_request("send_global_components", None, Some(10), None);
    let response = client_n1
        .receive_jsonrpc_response(1000)
        .expect("Should receive global components response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let global_components = result.as_object().expect("Result should be an object");

    // N2 namespace should not be in global components
    assert!(
        !global_components.contains_key(namespaces::N2),
        "N2 should be removed after timeout"
    );

    client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown N1");
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(10))
        .expect("Failed to join N1 thread");
}

/// Timeout with configurable interval
///
/// Tests that coordinator respects the timeout_interval configuration:
/// Create coordinator with custom timeout and verify behavior
///
/// Protocol: docs/control_protocol.md#heartbeat
#[test]
fn timeout_with_custom_interval() {
    // Create coordinator with custom timeout via configuration
    // For now, we'll use the timeout_interval parameter in CoordinatorApp::new

    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Wait longer than custom timeout (2s)
    thread::sleep(Duration::from_secs(3));

    // Component should have timed out - verify by querying components
    let mut query = TestClient::connect(coordinator.port).expect("Failed to create query client");
    query
        .sign_in("query", Some(&coordinator.namespace))
        .expect("Failed to sign in query");

    query
        .send_jsonrpc_request("send_local_components", None, Some(1), None)
        .expect("Failed to send send_local_components");

    let response = query
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let components = result.as_array().expect("Result should be an array");

    let has_ca = components
        .iter()
        .any(|v| v.as_str().is_some_and(|s| s.contains(components::CA)));

    assert!(!has_ca, "CA should be removed after timeout");

    query.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Component re-signs in after timeout
///
/// Tests that a component can sign in again after being removed due to timeout:
/// 1. Component signs in
/// 2. Component times out
/// 3. Component tries to sign in again with same name
/// 4. Should succeed (since it was removed)
///
/// Protocol: docs/control_protocol.md#heartbeat
#[test]
fn component_re_signs_in_after_timeout() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client1 = TestClient::connect(coordinator.port).expect("Failed to create client");
    client1
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    // Wait for timeout
    thread::sleep(Duration::from_secs(3));

    // Component times out, should be removed

    // Re-sign in with same name - should succeed
    let mut client2 = TestClient::connect(coordinator.port).expect("Failed to create new client");
    let response = client2
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to re-sign in");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    client2
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Heartbeat with high message activity
///
/// Tests that coordinator handles rapid heartbeat messages without issues:
/// 1. Component signs in
/// 2. Component sends many rapid messages
/// 3. Coordinator tracks all as heartbeats
/// 4. Component doesn't timeout
///
/// Protocol: docs/control_protocol.md#heartbeat
#[test]
fn heartbeat_with_high_message_activity() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(2));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send many rapid messages and collect responses in batches
    for i in 1..21 {
        client
            .send_jsonrpc_request("pong", None, Some(i), None)
            .expect("Failed to send pong");
        thread::sleep(Duration::from_millis(10));

        // Collect response every 5 messages
        if i % 5 == 0 {
            for _ in 0..5 {
                let _ = client.receive_jsonrpc_response(200);
            }
        }
    }

    // Component should still be considered alive
    client
        .send_jsonrpc_request("send_local_components", None, Some(999), None)
        .expect("Failed to send query");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// No false positive timeout on sleeping component
///
/// Tests that component doesn't timeout when it's actively communicating
/// even if there are pauses between messages (as long as < timeout)
///
/// Protocol: docs/control_protocol.md#heartbeat
#[test]
fn no_false_positive_timeout_with_pauses() {
    let mut coordinator =
        TestCoordinator::spawn_with_timeout(namespaces::N1, Some(find_free_port()), Some(3));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Send messages with pauses shorter than timeout
    for i in 1..4 {
        client
            .send_jsonrpc_request("pong", None, Some(i), None)
            .expect("Failed to send pong");
        let _ = client
            .receive_jsonrpc_response(500)
            .expect("Should receive pong response");

        // Wait less than timeout (3s)
        thread::sleep(Duration::from_secs(1));
    }

    // Component should still be active
    client
        .send_jsonrpc_request("send_local_components", None, Some(100), None)
        .expect("Failed to send query");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}
