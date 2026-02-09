use ruleco_coordinator::app::CoordinatorApp;
use std::thread;
use std::time::Duration;
use zmq;

#[test]
#[ignore]
fn test_coordinator_sign_in_response() {
    // Setup coordinator
    let namespace = "test_namespace";
    let port = 5555; // Use a fixed port for testing
    let mut coordinator =
        CoordinatorApp::new(namespace, Some(port), None).expect("Failed to create coordinator");

    // Start coordinator in a separate thread
    let _coordinator_thread = thread::spawn(move || {
        coordinator.run().expect("Coordinator failed to run");
    });

    // Allow time for coordinator to start
    thread::sleep(Duration::from_millis(100));

    // Setup test client
    let context = zmq::Context::new();
    let client_socket = context.socket(zmq::DEALER).unwrap();
    client_socket
        .connect(&format!("tcp://localhost:{}", port))
        .expect("Failed to connect client");

    // Prepare sign_in request
    let sign_in_request = r#"{"jsonrpc":"2.0","method":"sign_in","id":1}"#;

    // Send request
    client_socket
        .send(sign_in_request, 0)
        .expect("Failed to send request");

    // Receive response
    let response = client_socket
        .recv_string(0)
        .expect("Failed to receive response")
        .unwrap();

    // Validate response
    assert!(
        response.contains(r#""result":null"#),
        "Response did not contain expected result"
    );

    // Cleanup
    // Note: In a real scenario, you'd want a more graceful shutdown mechanism
    // For now, we'll just drop the coordinator thread
}
