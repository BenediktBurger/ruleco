//! CLI smoke tests for the ruleco-coordinator binary
//!
//! Tests that the coordinator binary can be started with CLI arguments,
//! responds to JSON-RPC requests, and handles --help correctly.

mod common;

use common::{
    assert_success_response, components, find_free_port, namespaces, wait_for_coordinator,
    TestClient,
};
use ruleco_coordinator::core::parameter_types::AddNodesParams;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

fn bin_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
        .join("ruleco-coordinator")
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            child: Some(child),
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn start_coordinator(namespace: &str, port: u16) -> ChildGuard {
    let bin = bin_path();
    let child = Command::new(&bin)
        .arg("--namespace")
        .arg(namespace)
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn coordinator binary at {:?}: {}", bin, e));
    ChildGuard::new(child)
}

/// Coordinator binary starts, accepts sign_in, and shuts down cleanly
#[test]
fn coordinator_binary_starts_and_responds() {
    let port = find_free_port();
    let mut guard = start_coordinator(namespaces::N1, port);

    thread::sleep(Duration::from_millis(500));

    let mut client = TestClient::connect(port).expect("Failed to connect to coordinator");
    wait_for_coordinator(&client, 50).expect("Coordinator not ready");

    let response = client
        .sign_in(components::CA, Some(namespaces::N1))
        .expect("Failed to sign in");
    assert_success_response(&response);

    client
        .send_shutdown(None)
        .expect("Failed to send shutdown");

    let status = guard
        .child
        .take()
        .unwrap()
        .wait()
        .expect("Failed to wait for child");
    assert!(status.success(), "Coordinator should exit successfully");
}

/// --help flag prints usage info and exits with code 0
#[test]
fn coordinator_binary_help_flag() {
    let bin = bin_path();
    let output = Command::new(&bin)
        .arg("--help")
        .output()
        .unwrap_or_else(|e| panic!("Failed to run --help: {}", e));

    assert!(output.status.success(), "--help should exit with 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--namespace"),
        "Help should mention --namespace, got: {}",
        stdout
    );
    assert!(
        stdout.contains("--port"),
        "Help should mention --port, got: {}",
        stdout
    );
    assert!(
        stdout.contains("--timeout-interval"),
        "Help should mention --timeout-interval, got: {}",
        stdout
    );
}

/// Two coordinator binaries communicate via add_nodes and cross-coordinator routing
#[test]
fn two_coordinator_binaries_communicate() {
    let port_n1 = find_free_port();
    let port_n2 = find_free_port();

    let mut guard_n1 = start_coordinator(namespaces::N1, port_n1);
    let mut guard_n2 = start_coordinator(namespaces::N2, port_n2);

    thread::sleep(Duration::from_millis(500));

    let mut client_n1 =
        TestClient::connect(port_n1).expect("Failed to connect to coordinator N1");
    wait_for_coordinator(&client_n1, 50).expect("Coordinator N1 not ready");

    let mut client_n2 =
        TestClient::connect(port_n2).expect("Failed to connect to coordinator N2");
    wait_for_coordinator(&client_n2, 50).expect("Coordinator N2 not ready");

    let response = client_n1
        .sign_in(components::CA, Some(namespaces::N1))
        .expect("Failed to sign in to N1");
    assert_success_response(&response);

    let params = AddNodesParams {
        nodes: HashMap::from([(
            namespaces::N2.to_string(),
            format!("tcp://127.0.0.1:{}", port_n2),
        )]),
    };

    client_n1
        .send_jsonrpc_request(
            "add_nodes",
            Some(serde_json::value::to_value(params).unwrap()),
            Some(1),
            None,
        )
        .expect("Failed to send add_nodes");

    let _ = client_n1
        .receive_jsonrpc_response(1000)
        .expect("Should receive add_nodes response");

    thread::sleep(Duration::from_millis(300));

    let response = client_n2
        .sign_in(components::CB, Some(namespaces::N2))
        .expect("Failed to sign in to N2");
    assert_success_response(&response);

    thread::sleep(Duration::from_millis(300));

    let mut query_client =
        TestClient::connect(port_n2).expect("Failed to create query client on N2");
    query_client
        .sign_in("query", Some(namespaces::N2))
        .expect("Failed to sign in query client");

    query_client
        .send_jsonrpc_request("send_global_components", None, Some(2), None)
        .expect("Failed to send send_global_components");

    let response = query_client
        .receive_jsonrpc_response(1000)
        .expect("Should receive global components response");

    let result = response.get("result").expect("Should have result");
    let global_dir = result.as_object().expect("Result should be an object");

    let n1_components = global_dir
        .get(namespaces::N1)
        .and_then(|v| v.as_array())
        .expect("N2 should have N1 components after directory sync");

    let has_ca = n1_components
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s.contains(components::CA)));
    assert!(has_ca, "N2 should know about N1.CA after directory sync");

    client_n1
        .send_shutdown(None)
        .expect("Failed to shutdown N1");
    client_n2
        .send_shutdown(None)
        .expect("Failed to shutdown N2");

    let status_n1 = guard_n1.child.take().unwrap().wait().expect("wait N1");
    let status_n2 = guard_n2.child.take().unwrap().wait().expect("wait N2");
    assert!(status_n1.success(), "N1 should exit successfully");
    assert!(status_n2.success(), "N2 should exit successfully");
}
