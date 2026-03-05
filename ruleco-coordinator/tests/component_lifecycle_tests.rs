//! Component lifecycle integration tests
//!
//! Tests for component sign-in and sign-out operations, following the
//! control protocol specification sections:
//! - Signing-in (docs/control_protocol.md#signing-in)
//! - Signing-out (docs/control_protocol.md#signing-out)

use std::time::Duration;

mod common;
use common::{
    assert_error_data, assert_error_response, assert_jsonrpc_valid, assert_success_response,
    components, find_free_port, namespaces, FixtureBuilder, TestClient, TestCoordinator,
};
use rstest::rstest;

/// Successfully sign in to the coordinator
///
/// Tests the sign-in handshake:
/// 1. Component connects and sends sign_in request (without knowing namespace)
/// 2. Coordinator responds with result:null
/// 3. Component extracts namespace from coordinator's sender in response
///
/// Protocol: docs/control_protocol.md#signing-in
#[test]
fn component_sign_in_success() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    let response = client
        .sign_in(components::CA, None)
        .expect("Failed to sign in");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    assert_eq!(
        client.namespace.as_deref(),
        Some(coordinator.namespace.as_str())
    );
    assert_eq!(client.component_name.as_deref(), Some(components::CA));

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Sign in with duplicate component name should fail with error -32091
///
/// Tests that the coordinator rejects duplicate names:
/// 1. Component CA signs in successfully
/// 2. Component CB signs in successfully
/// 3. Another component tries to sign in as CA
/// 4. Coordinator responds with error code -32091
///
/// Protocol: docs/control_protocol.md#signing-in
/// Error: -32091 - The name is already taken
#[test]
fn component_sign_in_duplicate_name() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client1 = TestClient::connect(coordinator.port).expect("Failed to create client1");
    let mut client2 = TestClient::connect(coordinator.port).expect("Failed to create client2");

    // First component signs in successfully
    client1
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in CA");

    // Second component tries to sign in with duplicate name CA
    let response = client2
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to receive response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, -32091);
    assert_error_data(&response, components::CA);

    client1
        .send_shutdown(None)
        .expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}

/// Successfully sign out from the coordinator
///
/// Tests the sign-out flow:
/// 1. Component signs in
/// 2. Component sends sign_out request
/// 3. Coordinator responds with result:null
/// 4. Component is removed from local directory
///
/// Protocol: docs/control_protocol.md#signing-out
#[test]
fn component_sign_out_success() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    let response = client.sign_out().expect("Failed to sign out");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");
    coordinator
        .shutdown(&client)
        .expect("Failed to shutdown coordinator");
}

/// Sign out without signing in should fail
///
/// Tests that the coordinator rejects sign_out from unsigned-in components
///
/// Protocol: docs/control_protocol.md#signing-out
#[test]
fn component_sign_out_without_sign_in() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    let result = client.sign_out();

    assert!(result.is_err());

    let mut shutdown_client =
        TestClient::connect(coordinator.port).expect("Failed to create shutdown client");
    shutdown_client
        .sign_in("shutdown_helper", Some(&coordinator.namespace))
        .expect("Failed to sign in shutdown helper");

    coordinator
        .shutdown(&shutdown_client)
        .expect("Failed to shutdown coordinator");
}

/// Sign out from wrong identity should fail
///
/// Tests that sign_out must come from the same connection as sign_in:
/// 1. Component CA signs in from client1
/// 2. Client1 disconnects
/// 3. Client2 connects and tries to sign out as CA
/// 4. Coordinator rejects (wrong identity)
///
/// Protocol: docs/control_protocol.md#signing-out
#[test]
fn component_sign_out_wrong_identity() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));

    let mut client1 = TestClient::connect(coordinator.port).expect("Failed to create client1");
    client1
        .set_identity(b"client1")
        .expect("Failed to set identity");

    client1
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Drop client1 - this simulates disconnection
    drop(client1);
    std::thread::sleep(Duration::from_millis(100));

    // Client2 connects with different identity tries to sign out as CA
    let mut client2 = TestClient::connect(coordinator.port).expect("Failed to create client2");
    client2.component_name = Some(components::CA.to_string());
    client2.namespace = Some(namespaces::N1.to_string());

    let response = client2.sign_out().expect("Should receive response");

    // Should receive error response because identity doesn't match
    // Routing layer catches this before handler runs, returns duplicate_name error
    assert_error_response(&response, -32091);
    assert_error_data(&response, components::CA);

    // Use a new client to shutdown (client2 can't shutdown because it's not authenticated)
    let mut shutdown_client =
        TestClient::connect(coordinator.port).expect("Failed to create shutdown client");
    shutdown_client
        .sign_in("shutdown_helper", Some(&coordinator.namespace))
        .expect("Failed to sign in shutdown helper");
    coordinator
        .shutdown(&shutdown_client)
        .expect("Failed to shutdown coordinator");
}

/// Component auto-sign-out on timeout
///
/// Tests that a component is automatically signed out when it times out:
/// 1. Component signs in
/// 2. Coordinator sends ping messages and checks for responses
/// 3. Component stops responding
/// 4. After timeout, coordinator removes component from directory
///
/// Protocol: docs/control_protocol.md#heartbeat
/// Implementation: app.rs:77 TODO
#[ignore] // Requires timeout detection implementation
#[test]
fn component_auto_sign_out_on_timeout() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    // Wait longer than timeout interval (timeout is 10 seconds by default)
    // In real test, we'd need shorter timeout or time manipulation
    std::thread::sleep(Duration::from_secs(15));

    // Try to send a message - should fail because component is signed out
    let result = client.send_jsonrpc_request("some_method", None, Some(3), None);

    assert!(result.is_err());

    coordinator
        .shutdown(&client)
        .expect("Failed to shutdown coordinator");
}

/// Sign in with full name (namespace.component_name)
///
/// Tests that sign_in works when using full name as sender
///
/// Protocol: docs/control_protocol.md#signing-in
#[test]
fn component_sign_in_with_full_name() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    let response = client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    coordinator
        .shutdown(&client)
        .expect("Failed to shutdown coordinator");
}

/// Sign in component with special characters in name
///
/// Tests that component names support valid ASCII characters (excluding '.')
///
/// Protocol: docs/control_protocol.md#naming-scheme
#[rstest]
#[case("Component_A")]
#[case("Component-1")]
#[case("Component")]
#[case("MyComponent123")]
fn component_sign_in_special_characters(#[case] component_name: &str) {
    let mut builder = FixtureBuilder::new();
    let ns = builder.new_namespace();
    let mut coordinator = TestCoordinator::spawn(&ns, Some(find_free_port()));
    let mut client = TestClient::connect(coordinator.port).expect("Failed to create client");

    let response = client
        .sign_in(component_name, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    coordinator
        .shutdown(&client)
        .expect("Failed to shutdown coordinator");
}
