//! E2E tests for ruleco-actor with ruleco-coordinator
//!
//! Tests the full integration of TestActor with TestCoordinator,
//! covering sign-in/sign-out, message routing, custom methods,
//! cross-coordinator routing, and shutdown.

mod common;

use common::{
    assert_error_response, assert_jsonrpc_valid, assert_success_response, components,
    find_free_port, namespaces, wait_for_condition, TestClient, TestCoordinator,
};
use ruleco_actor::TestActor;
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use ruleco_core::errors::LecoError;
use ruleco_core::full_name::FullName;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

fn shutdown_with_client(
    coordinator: &mut TestCoordinator,
    port: u16,
    namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TestClient::connect(port)?;
    client.sign_in("_shutdown_helper", Some(namespace))?;
    client.send_shutdown(None)?;
    // TODO: Replace with wait_for_condition on coordinator.is_running() once feasible
    thread::sleep(Duration::from_millis(500));
    coordinator.join_thread(Duration::from_secs(5))?;
    Ok(())
}

#[test]
fn actor_sign_in_sign_out_lifecycle() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor");

    let full_name = actor.sign_in(components::CA).expect("Sign-in failed");

    assert!(
        !full_name.namespace().is_empty(),
        "FullName should have a namespace"
    );
    assert_eq!(
        full_name.name(),
        components::CA.as_bytes(),
        "FullName name should match component name"
    );

    actor.sign_out().expect("Sign-out failed");
    assert!(actor.full_name().is_none());

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
}

#[test]
fn actor_duplicate_name_rejection() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor1 = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor1");
    actor1
        .sign_in(components::CA)
        .expect("First sign-in should succeed");

    let mut client2 = TestClient::connect(port).expect("Failed to create client2");
    let response = client2
        .sign_in(components::CA, Some(&ns))
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, LecoError::duplicate_name(None).code());

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
}

#[test]
fn actor_message_to_nonexistent_name_rejected() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let _actor = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor");

    let mut client = TestClient::connect(port).expect("Failed to create client");
    client
        .sign_in(components::CA, Some(&ns))
        .expect("Failed to sign in client");

    let unknown_receiver = FullName::from_strings(namespaces::N1, "unsigned_actor")
        .expect("Failed to create receiver name");
    client
        .send_jsonrpc_request("pong", None, Some(1), Some(&unknown_receiver))
        .expect("Failed to send message");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive error response");

    assert_jsonrpc_valid(&response);
    assert_error_response(&response, LecoError::receiver_unknown(None).code());

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
}

#[test]
fn actor_local_routing_get_parameters() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor_a = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor A");
    let _full_name_a = actor_a.sign_in(components::CA).expect("Sign-in A failed");

    let mut actor_b = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor B");
    let full_name_b = actor_b.sign_in(components::CB).expect("Sign-in B failed");

    let mut handle_a = actor_a.spawn().expect("Failed to spawn actor A");
    let mut handle_b = actor_b.spawn().expect("Failed to spawn actor B");

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actor failed to start");

    let params = json!({"parameters": ["some_key"]});
    let response = handle_a
        .call(&full_name_b, "get_parameters", Some(params))
        .expect("Call failed");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    let result = response.get("result").unwrap();
    assert!(
        result.is_object(),
        "get_parameters result should be an object"
    );

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
    let _ = handle_a.stop();
    let _ = handle_b.stop();
}

#[test]
fn actor_local_routing_set_then_get() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor_a = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor A");
    let _full_name_a = actor_a.sign_in(components::CA).expect("Sign-in A failed");

    let mut actor_b = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor B");
    let full_name_b = actor_b.sign_in(components::CB).expect("Sign-in B failed");

    let mut handle_a = actor_a.spawn().expect("Failed to spawn actor A");
    let mut handle_b = actor_b.spawn().expect("Failed to spawn actor B");

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actors failed to start");

    let set_params = json!({"parameters": {"temperature": 42.5}});
    let set_response = handle_a
        .call(&full_name_b, "set_parameters", Some(set_params))
        .expect("Set call failed");
    assert_success_response(&set_response);

    let get_params = json!({"parameters": ["temperature"]});
    let get_response = handle_a
        .call(&full_name_b, "get_parameters", Some(get_params))
        .expect("Get call failed");
    assert_success_response(&get_response);

    let result = get_response.get("result").unwrap();
    assert_eq!(result.get("temperature").unwrap(), &json!(42.5));

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
    let _ = handle_a.stop();
    let _ = handle_b.stop();
}

#[test]
fn actor_coordinator_method_call() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor");
    let _full_name = actor.sign_in(components::CA).expect("Sign-in failed");

    let mut handle = actor.spawn().expect("Failed to spawn actor");

    wait_for_condition(
        || handle.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actor failed to start");

    let coordinator_fn = FullName::from_strings(&ns, namespaces::COORDINATOR_NAME)
        .expect("Failed to create coordinator name");

    let response = handle
        .call(&coordinator_fn, "send_local_components", None)
        .expect("Call to coordinator failed");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    let result = response.get("result").unwrap();
    let components_str = match result {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>()
            .join(","),
        Value::String(s) => s.clone(),
        _ => String::new(),
    };
    assert!(
        components_str.contains(components::CA),
        "Component list should contain CA, got: {result}"
    );

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
    let _ = handle.stop();
}

#[test]
fn actor_custom_method_routing() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor_a = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor A");
    let full_name_a = actor_a.sign_in(components::CA).expect("Sign-in A failed");

    let mut actor_b = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor B");
    let _full_name_b = actor_b.sign_in(components::CB).expect("Sign-in B failed");

    actor_a.register_method(
        "echo",
        Box::new(|params| Ok(params.unwrap_or(Value::Null))),
    );

    let mut handle_a = actor_a.spawn().expect("Failed to spawn actor A");
    let mut handle_b = actor_b.spawn().expect("Failed to spawn actor B");

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actors failed to start");

    let echo_params = json!("hello_world");
    let response = handle_b
        .call(&full_name_a, "echo", Some(echo_params.clone()))
        .expect("Echo call failed");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);
    assert_eq!(
        response.get("result").unwrap(),
        &echo_params,
        "Echo should return the same params"
    );

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
    let _ = handle_a.stop();
    let _ = handle_b.stop();
}

#[test]
fn actor_pong_response() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor");
    let _full_name = actor.sign_in(components::CA).expect("Sign-in failed");

    let mut handle = actor.spawn().expect("Failed to spawn actor");

    wait_for_condition(
        || handle.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actor failed to start");

    let mut client = TestClient::connect(port).expect("Failed to create client");
    client
        .sign_in(components::CB, Some(&ns))
        .expect("Failed to sign in client");

    let actor_fn = handle.full_name().expect("Actor should have full name");
    client
        .send_jsonrpc_request("pong", None, Some(42), Some(&actor_fn))
        .expect("Failed to send pong to actor");

    let response = client
        .receive_jsonrpc_response(1000)
        .expect("Should receive response");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);
    assert_eq!(response.get("result").unwrap(), &Value::Null);
    assert_eq!(
        response.get("id").unwrap(),
        &json!(42),
        "Response ID should match request ID"
    );

    shutdown_with_client(&mut coordinator, port, &ns).expect("Failed to shutdown");
    let _ = handle.stop();
}

#[test]
fn actor_cross_coordinator_routing() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));
    let port_n1 = coordinator_n1.port;
    let port_n2 = coordinator_n2.port;
    let ns_n1 = coordinator_n1.namespace.clone();

    let mut actor_a = TestActor::connect(&format!("tcp://127.0.0.1:{port_n1}"))
        .expect("Failed to connect actor A to N1");
    let _full_name_a = actor_a.sign_in(components::CA).expect("Sign-in A failed");

    let mut actor_b = TestActor::connect(&format!("tcp://127.0.0.1:{port_n2}"))
        .expect("Failed to connect actor B to N2");
    let full_name_b = actor_b.sign_in(components::CB).expect("Sign-in B failed");

    let mut handle_a = actor_a.spawn().expect("Failed to spawn actor A");
    let mut handle_b = actor_b.spawn().expect("Failed to spawn actor B");

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actors failed to start");

    let mut link_client =
        TestClient::connect(port_n1).expect("Failed to create link client");
    link_client
        .sign_in("link_helper", Some(&ns_n1))
        .expect("Failed to sign in link client");

    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{port_n2}"),
        )]),
    };

    link_client
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Failed to send add_nodes");

    let _ = link_client.receive_jsonrpc_response(1000);

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(50),
    ).expect("Actors should remain running after link setup");

    // Allow time for cross-coordinator link to propagate
    thread::sleep(Duration::from_millis(200));

    let get_params = json!({"parameters": ["some_key"]});
    let response = handle_a
        .call(&full_name_b, "get_parameters", Some(get_params))
        .expect("Cross-coordinator call failed");

    assert_jsonrpc_valid(&response);
    assert_success_response(&response);

    shutdown_with_client(&mut coordinator_n1, port_n1, &ns_n1).expect("Failed to shutdown N1");
    shutdown_with_client(&mut coordinator_n2, port_n2, namespaces::N2).expect("Failed to shutdown N2");
    let _ = handle_a.stop();
    let _ = handle_b.stop();
}

#[test]
fn actor_state_through_coordinator_chain() {
    let mut coordinator_n1 = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let mut coordinator_n2 = TestCoordinator::spawn(namespaces::N2, Some(find_free_port()));
    let port_n1 = coordinator_n1.port;
    let port_n2 = coordinator_n2.port;
    let ns_n1 = coordinator_n1.namespace.clone();

    let mut actor_a = TestActor::connect(&format!("tcp://127.0.0.1:{port_n1}"))
        .expect("Failed to connect actor A to N1");
    actor_a.sign_in(components::CA).expect("Sign-in A failed");

    let mut actor_b = TestActor::connect(&format!("tcp://127.0.0.1:{port_n2}"))
        .expect("Failed to connect actor B to N2");
    let full_name_b = actor_b.sign_in(components::CB).expect("Sign-in B failed");

    let mut handle_a = actor_a.spawn().expect("Failed to spawn actor A");
    let mut handle_b = actor_b.spawn().expect("Failed to spawn actor B");

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actors failed to start");

    let mut link_client =
        TestClient::connect(port_n1).expect("Failed to create link client");
    link_client
        .sign_in("link_helper2", Some(&ns_n1))
        .expect("Failed to sign in link client");

    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{port_n2}"),
        )]),
    };

    link_client
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Failed to send add_nodes");

    let _ = link_client.receive_jsonrpc_response(1000);

    wait_for_condition(
        || handle_a.is_running() && handle_b.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(50),
    ).expect("Actors should remain running after link setup");

    // Allow time for cross-coordinator link to propagate
    thread::sleep(Duration::from_millis(200));

    let set_result = handle_a.set_test(&full_name_b, "pressure", json!(101.3));
    assert!(set_result.is_ok(), "set_test should succeed");

    let get_result = handle_a.get_test(&full_name_b, "pressure");
    assert!(
        get_result.is_ok(),
        "get_test should succeed"
    );
    assert_eq!(get_result.unwrap(), json!(101.3));

    assert_eq!(
        handle_b.get_state_direct("pressure"),
        Some(json!(101.3)),
        "State should be set in actor B's shared state"
    );

    shutdown_with_client(&mut coordinator_n1, port_n1, &ns_n1).expect("Failed to shutdown N1");
    shutdown_with_client(&mut coordinator_n2, port_n2, namespaces::N2).expect("Failed to shutdown N2");
    let _ = handle_a.stop();
    let _ = handle_b.stop();
}

#[test]
fn actor_shutdown_via_coordinator() {
    let mut coordinator = TestCoordinator::spawn(namespaces::N1, Some(find_free_port()));
    let port = coordinator.port;
    let ns = coordinator.namespace.clone();

    let mut actor = TestActor::connect(&format!("tcp://127.0.0.1:{port}"))
        .expect("Failed to connect actor");
    actor.sign_in(components::CA).expect("Sign-in failed");

    let mut handle = actor.spawn().expect("Failed to spawn actor");

    wait_for_condition(
        || handle.is_running(),
        Duration::from_secs(2),
        Duration::from_millis(10),
    ).expect("Actor failed to start");

    let mut client = TestClient::connect(port).expect("Failed to create client");
    client
        .sign_in("shutdown_caller", Some(&ns))
        .expect("Failed to sign in client");

    let coordinator_fn = FullName::from_strings(&ns, namespaces::COORDINATOR_NAME)
        .expect("Failed to create coordinator name");

    client
        .send_jsonrpc_request("shut_down", None, Some(999), Some(&coordinator_fn))
        .expect("Failed to send shut_down");

    let _ = client.receive_jsonrpc_response(1000);

    client.send_shutdown(None).expect("Failed to send final shutdown");
    // TODO: Replace with wait_for_condition on coordinator.is_running() once feasible
    thread::sleep(Duration::from_millis(500));
    coordinator
        .join_thread(Duration::from_secs(5))
        .expect("Coordinator should shut down");

    assert!(!coordinator.is_running());

    let _ = handle.stop();
}
