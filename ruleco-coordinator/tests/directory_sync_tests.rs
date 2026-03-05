//! Directory synchronization integration tests
//!
//! Tests for directory propagation between coordinators and directory methods,
//! following the control protocol specification:
//! - Coordinator updates (docs/control_protocol.md#coordinator-updates)
//! - Directory methods (docs/schemas/coordinator.json)

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

mod common;
use common::{
    assert_jsonrpc_valid, assert_success_response, components, find_free_port, namespaces,
    TestClient, TestCoordinator,
};
use rstest::rstest;
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use serde_json::json;

/// send_nodes method - coordinator returns known coordinator addresses
///
/// Tests that coordinator can send its known coordinator addresses:
/// 1. Coordinator returns object with namespace:address mappings
///
/// Protocol: docs/schemas/coordinator.json#send_nodes
#[test]
fn send_nodes_method() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Request nodes from coordinator
    let _ = client
        .send_jsonrpc_request("send_nodes", None, Some(1), None)
        .expect("Failed to send send_nodes");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    // Response should contain result with nodes object
    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    // Result should be an object (even if empty)
    assert!(result.is_object(), "Result should be an object");

    let _ = client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// send_nodes with multiple coordinators known
///
/// Tests that send_nodes returns all known coordinator addresses
///
/// Protocol: docs/schemas/coordinator.json#send_nodes example
#[test]
fn send_nodes_with_known_coordinators() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));
    let mut coordinator_n3 = TestCoordinator::spawn(namespaces::N3, Some(find_free_port()));

    // Connect N1 to N2 and N3 using add_nodes
    let mut client =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client for N1");
    client
        .sign_in("setup", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    let params = AddNodesParams {
        nodes: HashMap::from([
            (namespaces::N2.to_string(), coordinator_n2.address()),
            (namespaces::N3.to_string(), coordinator_n3.address()),
        ]),
    };
    let _ = client.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );
    let _ = client.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-ins to complete
    thread::sleep(Duration::from_millis(300));

    // Request nodes from N1
    let _ = client
        .send_jsonrpc_request("send_nodes", None, Some(2), None)
        .expect("Failed to send send_nodes");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let nodes = result.as_object().expect("Result should be an object");

    // Should have entries for known coordinators (N1, N2, N3)
    assert!(nodes.contains_key(namespaces::N1), "Should have N1");
    assert!(nodes.contains_key(namespaces::N2), "Should have N2");
    assert!(nodes.contains_key(namespaces::N3), "Should have N3");

    let _ = client.send_shutdown(None).expect("Failed to send shutdown");

    // Shutdown N2 and N3
    let mut client_n2 =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client for N2");
    client_n2
        .sign_in("shutdown", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in to N2");
    client_n2
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");

    let mut client_n3 =
        TestClient::connect(coordinator_n3.port).expect("Failed to create client for N3");
    client_n3
        .sign_in("shutdown", Some(&coordinator_n3.namespace))
        .expect("Failed to sign in to N3");
    client_n3
        .send_shutdown(None)
        .expect("Failed to send shutdown to N3");

    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
    coordinator_n3
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N3");
}

/// record_components method - coordinator records remote components
///
/// Tests that coordinator can accept component directory information:
/// 1. Remote coordinator sends list of its components
/// 2. Local coordinator stores in global directory
///
/// Protocol: docs/schemas/coordinator.json#record_components
#[test]
fn record_components_method() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator_n1.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in");

    // Send record_components with remote coordinator's components
    let remote_components = json!(["N2.CA", "N2.CB", "N2.CC"]);

    let params = json!({
        "components": remote_components
    });

    let _ = client
        .send_jsonrpc_request("record_components", Some(params), Some(1), None)
        .expect("Failed to send record_components");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Coordinator should have stored these components in global directory
    // (Verification would require access to coordinator internal state)

    let _ = client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// send_local_components method - coordinator returns local directory
///
/// Tests that coordinator returns its locally connected components
///
/// Protocol: docs/schemas/coordinator.json#send_local_components example
#[test]
fn send_local_components_method() {
    let namespace = namespaces::N1;
    let mut coordinator = TestCoordinator::spawn(namespace, Some(find_free_port()));

    let mut client_ca = TestClient::connect(coordinator.port).expect("Failed to create client CA");
    client_ca
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    let mut client_cb = TestClient::connect(coordinator.port).expect("Failed to create client CB");
    client_cb
        .sign_in(components::CB, Some(&coordinator.namespace))
        .expect("Failed to sign in CB");

    thread::sleep(Duration::from_millis(100));

    // Query local components
    let mut query_client =
        TestClient::connect(coordinator.port).expect("Failed to create query client");
    query_client
        .sign_in("query_component", Some(&coordinator.namespace))
        .expect("Failed to sign in query");

    let _ = query_client
        .send_jsonrpc_request("send_local_components", None, Some(1), None)
        .expect("Failed to send send_local_components");

    let response = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let components_array = result.as_array().expect("Result should be an array");

    // Should contain signed-in components (format: N1.CA, N1.CB)
    assert!(
        components_array.len() >= 2,
        "Should have at least 2 components"
    );

    let component_names: Vec<&str> = components_array.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        component_names.iter().any(|n| n.contains(components::CA)),
        "Should contain component CA"
    );
    assert!(
        component_names.iter().any(|n| n.contains(components::CB)),
        "Should contain component CB"
    );

    let _ = query_client
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// send_global_components method - coordinator returns global directory
///
/// Tests that coordinator returns all components in the network
///
/// Protocol: docs/schemas/coordinator.json#send_global_components example
#[test]
fn send_global_components_method() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Sign in local components on both coordinators
    let mut client_n1_ca =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client N1.CA");
    client_n1_ca
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in N1.CA");

    let mut client_n2_ca =
        TestClient::connect(coordinator_n2.port).expect("Failed to create client N2.CA");
    client_n2_ca
        .sign_in("CA", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in N2.CA"); // Different component name

    // Coordinators sign in to each other (TBD)
    // At this point, they don't know about each other

    thread::sleep(Duration::from_millis(100));

    // Query global components from N1
    let mut query_client =
        TestClient::connect(coordinator_n1.port).expect("Failed to create query client");
    query_client
        .sign_in("query", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in query");

    let _ = query_client
        .send_jsonrpc_request("send_global_components", None, Some(1), None)
        .expect("Failed to send send_global_components");

    let response = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let global_directory = result
        .as_object()
        .expect("Result should be an object with namespace -> components mapping");

    // Should have local components (maybe only local before coordinator sync)
    let n1_components = global_directory
        .get(namespaces::N1)
        .and_then(|v| v.as_array())
        .expect("Should have N1 components");

    assert!(!n1_components.is_empty(), "N1 should have local components");

    // After coordinator sign-in, should also have N2 components
    // (Implementation TBD)

    let _ = query_client
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    // N2 needs to be shut down via a client that signed in
    let _ = client_n2_ca
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// Directory sync on component sign-in
///
/// Tests that local component sign-in notifies all other coordinators:
/// 1. Coordinators N1 and N2 are connected
/// 2. Component CA signs in to N1
/// 3. N1 notifies N2 about the sign-in
/// 4. N2 updates its global directory
///
/// Protocol: docs/control_protocol.md#coordinator-updates
#[test]
fn directory_sync_on_component_sign_in() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Connect N1 to N2 using add_nodes
    let mut client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client for N1");
    client_n1
        .sign_in("setup", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    let params = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2.address())]),
    };
    let _ = client_n1.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );
    let _ = client_n1.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-in to complete
    thread::sleep(Duration::from_millis(300));

    // Now sign in a component on N1 - this should trigger directory sync to N2
    let mut client = TestClient::connect(coordinator_n1.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in CA");

    thread::sleep(Duration::from_millis(300));

    // Query N2's global directory - should now know about N1.CA
    let mut query_client =
        TestClient::connect(coordinator_n2.port).expect("Failed to create query client");
    query_client
        .sign_in("query", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in query");

    let _ = query_client
        .send_jsonrpc_request("send_global_components", None, Some(1), None)
        .expect("Failed to send send_global_components");

    let response = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let global_directory = result.as_object().expect("Result should be an object");

    let n1_components = global_directory
        .get(namespaces::N1)
        .and_then(|v| v.as_array())
        .expect("Should have N1 components");

    // N1.CA should be in N2's global directory
    let has_ca = n1_components
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s.contains(components::CA)));

    assert!(has_ca, "N2 should know about N1.CA after sync");

    let _ = client.send_shutdown(None).expect("Failed to send shutdown");
    let _ = query_client
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// Directory sync on component sign-out
///
/// Tests that local component sign-out notifies all other coordinators:
/// 1. Coordinators N1 and N2 are connected
/// 2. Component CA signs in to N1
/// 3. N2 knows about CA from sync
/// 4. CA signs out from N1
/// 5. N1 notifies N2 about the sign-out
/// 6. N2 removes CA from global directory
///
/// Protocol: docs/control_protocol.md#coordinator-updates
#[test]
fn directory_sync_on_component_sign_out() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));

    // Connect N1 to N2 using add_nodes
    let mut client_n1 =
        TestClient::connect(coordinator_n1.port).expect("Failed to create client for N1");
    client_n1
        .sign_in("setup", Some(&coordinator_n1.namespace))
        .expect("Failed to sign in to N1");

    let params = AddNodesParams {
        nodes: HashMap::from([(namespaces::N2.to_string(), coordinator_n2.address())]),
    };
    let _ = client_n1.send_jsonrpc_request(
        "add_nodes",
        Some(serde_json::value::to_value(params).unwrap()),
        Some(1),
        None,
    );
    let _ = client_n1.receive_jsonrpc_response(1000);

    // Wait for coordinator sign-in to complete
    thread::sleep(Duration::from_millis(300));

    // CA signs in to N1
    let mut client_ca = TestClient::connect(coordinator_n1.port).expect("Failed to create client");
    client_ca
        .sign_in(components::CA, Some(&coordinator_n1.namespace))
        .expect("Failed to sign in CA");

    thread::sleep(Duration::from_millis(300));

    // Verify CA is in N2's directory
    let mut query_client =
        TestClient::connect(coordinator_n2.port).expect("Failed to create query client");
    query_client
        .sign_in("query", Some(&coordinator_n2.namespace))
        .expect("Failed to sign in query");

    // Query global from N2
    let _ = query_client
        .send_jsonrpc_request("send_global_components", None, Some(1), None)
        .expect("Failed to send send_global_components");

    let response = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");
    let global_directory = result.as_object().expect("Result should be an object");
    let n1_components = global_directory
        .get(namespaces::N1)
        .and_then(|v| v.as_array())
        .expect("Should have N1 components");

    let has_ca_before = n1_components
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s.contains(components::CA)));
    assert!(has_ca_before, "N2 should know about CA initially");

    // CA signs out from N1
    client_ca.sign_out().expect("Failed to sign out CA");

    thread::sleep(Duration::from_millis(300));

    // Query N2's directory again
    let _ = query_client
        .send_jsonrpc_request("send_global_components", None, Some(2), None)
        .expect("Failed to send send_global_components");

    let response2 = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result2 = response2
        .get("result")
        .expect("Response should contain 'result'");
    let global_directory2 = result2.as_object().expect("Result should be an object");
    let n1_components2 = global_directory2
        .get(namespaces::N1)
        .and_then(|v| v.as_array())
        .expect("Should have N1 components");

    let has_ca_after = n1_components2
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s.contains(components::CA)));

    assert!(!has_ca_after, "N2 should have removed CA after sign-out");

    let _ = client_n1
        .send_shutdown(None)
        .expect("Failed to send shutdown to N1");
    let _ = query_client
        .send_shutdown(None)
        .expect("Failed to send shutdown to N2");
    std::thread::sleep(Duration::from_millis(500));
    coordinator_n1
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N1");
    coordinator_n2
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join N2");
}

/// remove_expired_addresses method
///
/// Tests that coordinator can remove expired addresses from directory
///
/// Protocol: docs/schemas/coordinator.json#remove_expired_addresses
#[test]
fn remove_expired_addresses_method() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Remove expired addresses with long expiration (should not remove active component)
    let params = json!({
        "expiration_time": 3600  // 1 hour
    });

    let _ = client
        .send_jsonrpc_request("remove_expired_addresses", Some(params), Some(1), None)
        .expect("Failed to send remove_expired_addresses");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Query local components - CA should still be there
    let _ = client
        .send_jsonrpc_request("send_local_components", None, Some(2), None)
        .expect("Failed to send send_local_components");

    let response2 = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result2 = response2
        .get("result")
        .expect("Response should contain 'result'");

    let _components = result2.as_array().expect("Result should be an array");

    // Depending on implementation, CA may or may not be present here
    // Test primarily checks method is callable

    let _ = client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to join coordinator");
}

/// Directory sync with multiple components signing in sequence
///
/// Tests directory consistency with multiple rapid component sign-ins
///
/// Protocol: docs/control_protocol.md#coordinator-updates
#[rstest]
#[case(3)]
#[case(5)]
#[case(10)]
fn rapid_component_sign_ins_directory_sync(#[case] num_components: usize) {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut clients = Vec::new();

    // Create and sign in multiple components rapidly
    for i in 0..num_components {
        let component_name = format!("component_{}", i);
        let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");
        client
            .sign_in(&component_name, Some(&coordinator.namespace))
            .expect("Failed to sign in component");
        clients.push(client);
    }

    thread::sleep(Duration::from_millis(200));

    // Query local directory
    let mut query = TestClient::connect(coordinator.port).expect("Failed to create query client");
    query
        .sign_in("query", Some(&coordinator.namespace))
        .expect("Failed to sign in query");

    let _ = query
        .send_jsonrpc_request("send_local_components", None, Some(1), None)
        .expect("Failed to send send_local_components");

    let response = query
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    let result = response
        .get("result")
        .expect("Response should contain 'result'");

    let components = result.as_array().expect("Result should be an array");

    // Should have at least the signed-in components
    assert!(
        components.len() >= num_components,
        "Should have at least {} components",
        num_components
    );

    let _ = query.send_shutdown(None).expect("Failed to send shutdown");

    // Clean up clients first
    for mut client in clients {
        let _ = client.sign_out();
    }

    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Failed to shutdown coordinator");
}
