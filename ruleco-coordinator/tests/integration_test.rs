//! Legacy integration Test(s)
//!
//! This module contains the original integration test that has been
//! refactored to use the common test helpers.
//!
//! Note: The protocol doc at docs/control_protocol.md#signing-in covers sign-in behavior!

mod common;
use common::{
    assert_jsonrpc_valid, assert_success_response, components, find_free_port, TestClient,
    TestCoordinator,
};
use std::time::Duration;

/// Test coordinator sign-in using the refactored test helpers
///
/// This is the original test from integration_test.rs, now using
/// the TestClient and TestCoordinator helpers from common/mod.rs
#[test]
fn test_coordinator_sign_in() {
    let namespace = "test_namespace_sign_in";
    let port = find_free_port();

    let mut coordinator = TestCoordinator::spawn(namespace, Some(port));
    let mut client = TestClient::connect(port).expect("Failed to create client");

    let response = client
        .sign_in(components::CA, Some(&coordinator.namespace))
        .expect("Failed to sign in");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    // Verify the response contains result:null
    assert_eq!(
        response.get("result"),
        Some(&serde_json::Value::Null),
        "Response should have null result"
    );

    client.send_shutdown(None).expect("Failed to send shutdown");
    std::thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Thread did not finish");
}
